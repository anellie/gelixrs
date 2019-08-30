/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/31/19 1:04 AM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

mod builder;
mod passes;

use crate::ast::declaration::{DeclarationList, Function};
use crate::ast::expression::Expression;
use crate::ast::literal::Literal;
use crate::lexer::token::{Token, Type};
use crate::mir::generator::passes::declare::DeclarePass;
use crate::mir::generator::passes::fill_struct::FillStructPass;
use crate::mir::generator::passes::PreMIRPass;
use crate::mir::nodes::{MIRExpression, MIRFlow, MIRFunction, MIRStructMem, MIRType, MIRVariable};
use crate::mir::{MutRc, MIR};
use crate::{Error, Res};
use builder::MIRBuilder;
use std::collections::HashMap;
use std::rc::Rc;

/// The MIRGenerator turns a list of declarations produced by the parser
/// into their MIR representation.
///
/// MIR is an intermediate format between the AST and LLVM IR.
///
/// The generator not only generates MIR, but also checks the code
/// for correctness (type-checking, scoping, etc.).
pub struct MIRGenerator {
    /// The builder used to build the MIR.
    builder: MIRBuilder,
    /// An environment is a scope that variables live in.
    /// This variable is used like a stack.
    /// See the begin_scope and end_scope functions for more info.
    environments: Vec<HashMap<Rc<String>, Rc<MIRVariable>>>,

    /// If the current position is inside a loop.
    is_in_loop: bool,
    /// The current return type of the loop, determined by break expressions.
    current_loop_ret_type: Option<MIRType>,
    /// The block to jump to when the current loop finishes.
    current_loop_cont_block: Option<Rc<String>>,
}

impl MIRGenerator {
    /// Will do everything needed to generate MIR from the AST.
    pub fn generate(mut self, mut list: DeclarationList) -> Res<MIR> {
        // Run all pre-MIR passes
        DeclarePass::new(&mut self).run(&mut list)?;
        FillStructPass::new(&mut self).run(&mut list)?;

        // Generate the MIR
        self.generate_mir(list)?;

        // Return the finished MIR
        Ok(MIR {
            types: self.builder.get_types(),
            functions: self.environments.remove(0),
        })
    }

    fn generate_mir(&mut self, list: DeclarationList) -> Res<()> {
        for func in list.functions.into_iter().chain(
            list.classes
                .into_iter()
                .map(|class| class.methods)
                .flatten(),
        ) {
            self.generate_function(func)?;
        }

        Ok(())
    }

    fn generate_function(&mut self, func: Function) -> Res<()> {
        let function_rc = self.builder.find_function(&func.sig.name.lexeme).unwrap();
        let mut function = function_rc.borrow_mut();
        let func_type = function.ret_type.clone();
        function.append_block("entry".to_string());
        drop(function);
        self.builder
            .set_pointer(Rc::clone(&function_rc), Rc::new("entry".to_string()));

        self.begin_scope();
        for param in function_rc.borrow().parameters.iter() {
            self.insert_variable(Rc::clone(param), false, func.sig.name.line)?;
        }

        let body = self.generate_expression(&func.body)?;
        if func_type != MIRType::None {
            if func_type == body.get_type() {
                self.builder.set_return(MIRFlow::Return(body));
            } else {
                return Err(Self::error(
                    &func.sig.name,
                    func.sig.return_type.as_ref().unwrap_or(&func.sig.name),
                    &format!(
                        "Function return type ({}) does not match body type ({}).",
                        func_type,
                        body.get_type()
                    ),
                ));
            }
        } else {
            self.builder.insert_at_ptr(body)
        }

        self.end_scope();
        Ok(())
    }

    fn generate_expression(&mut self, expression: &Expression) -> Res<MIRExpression> {
        Ok(match expression {
            Expression::Assignment { name, value } => {
                let var = self.find_var(&name)?;
                if var.mutable {
                    let value = self.generate_expression(&**value)?;
                    if value.get_type() == var._type {
                        self.builder.build_store(var, value)
                    } else {
                        return Err(Self::error(
                            &name,
                            &name,
                            &format!("Variable {} is a different type", name.lexeme),
                        ));
                    }
                } else {
                    return Err(Self::error(
                        &name,
                        &name,
                        &format!("Variable {} is not assignable (val)", name.lexeme),
                    ));
                }
            }

            Expression::Binary {
                left,
                operator,
                right,
            } => {
                let left = self.generate_expression(&**left)?;
                let right = self.generate_expression(&**right)?;

                if (left.get_type() == MIRType::Int) && (right.get_type() == MIRType::Int) {
                    self.builder.build_binary(left, operator.t_type, right)
                } else {
                    return Err(Self::error(
                        &operator,
                        &operator,
                        "Binary operations are only allowed on i64.",
                    ));
                }
            }

            Expression::Block(expressions) => {
                if expressions.is_empty() {
                    return Ok(Self::none_const());
                }

                self.begin_scope();

                for expression in expressions.iter().take(expressions.len() - 1) {
                    let expression = self.generate_expression(&expression)?;
                    self.builder.insert_at_ptr(expression);
                }
                let last = self.generate_expression(expressions.last().unwrap())?;

                self.end_scope();
                last
            }

            Expression::Break(expr) => {
                if !self.is_in_loop {
                    return Err(Self::anon_err(
                        expr.as_ref().map(|e| e.get_token()).flatten(),
                        "Break is only allowed in loops.",
                    ));
                }

                if let Some(expression) = expr {
                    let expression = self.generate_expression(&**expression)?;
                    let body_alloca = self.find_or_create_var(
                        &expression.get_type(),
                        Token::generic_identifier("for-body".to_string()),
                    )?;
                    self.builder.build_store(body_alloca, expression);
                }

                self.builder.set_return(MIRFlow::Jump(Rc::clone(
                    self.current_loop_cont_block.as_ref().unwrap(),
                )));
                Self::none_const()
            }

            Expression::Call { callee, arguments } => {
                match &**callee {
                    // Method call
                    Expression::Get { object: _, name: _ } => unimplemented!(),

                    // Might be class constructor
                    Expression::Variable(name) => {
                        if let Some(struc) = self.builder.find_struct(&name.lexeme) {
                            return Ok(self.builder.build_constructor(struc));
                        }
                    }

                    _ => (),
                }

                // match above fell through, its either a function call or invalid
                let callee_mir = self.generate_expression(&**callee)?;
                if let MIRType::Function(func) = callee_mir.get_type() {
                    let args = self.generate_func_args(func, arguments)?;
                    self.builder.build_call(callee_mir, args)
                } else {
                    return Err(Self::anon_err(
                        callee.get_token(),
                        "Only functions are allowed to be called",
                    ));
                }
            }

            Expression::For { condition, body, else_b } => {
                let cur_fn_rc = self.builder.cur_fn();
                let mut cur_fn = cur_fn_rc.borrow_mut();
                let loop_block = cur_fn.append_block("for-loop".to_string());
                let else_block = cur_fn.append_block("for-else".to_string());
                let cont_block = cur_fn.append_block("for-cont".to_string());

                let prev_ret_type = std::mem::replace(&mut self.current_loop_ret_type, None);
                let prev_cont_block = std::mem::replace(
                    &mut self.current_loop_cont_block,
                    Some(Rc::clone(&cont_block)),
                );
                let was_in_loop = std::mem::replace(&mut self.is_in_loop, true);

                drop(cur_fn);

                let cond = self.generate_expression(&**condition)?;
                if cond.get_type() != MIRType::Bool {
                    return Err(Self::anon_err(
                        condition.get_token(),
                        "For condition must be a boolean.",
                    ));
                }

                self.builder.set_return(MIRFlow::Branch {
                    condition: cond.clone(),
                    then_b: Rc::clone(&loop_block),
                    else_b: Rc::clone(&else_block)
                });

                self.builder.set_block(&loop_block);
                let body = self.generate_expression(&**body)?;
                let body_type = body.get_type();
                let body_alloca = self.find_or_create_var(
                    &body_type,
                    Token::generic_identifier("for-body".to_string()),
                )?;

                let store = self.builder.build_store(Rc::clone(&body_alloca), body);
                self.builder.insert_at_ptr(store);
                self.builder.set_return(MIRFlow::Branch {
                    condition: cond,
                    then_b: Rc::clone(&loop_block),
                    else_b: Rc::clone(&cont_block)
                });

                self.builder.set_block(&else_block);
                self.builder.set_return(MIRFlow::Jump(Rc::clone(&cont_block)));
                let ret = if let Some(else_b) = else_b {
                    let else_b = self.generate_expression(&**else_b)?;
                    if else_b.get_type() == body_type {
                        self.builder.set_block(&cont_block);
                        self.builder.build_phi(vec![
                            (else_b, else_block),
                            (self.builder.build_load(body_alloca), loop_block)
                        ])
                    } else {
                        Self::none_const()
                    }
                } else {
                    Self::none_const()
                };

                self.builder.set_block(&cont_block);
                self.current_loop_ret_type = prev_ret_type;
                self.current_loop_cont_block = prev_cont_block;
                self.is_in_loop = was_in_loop;

                ret
            }

            Expression::Get { object, name } => {
                let (object, field) = self.get_class_field(&**object, name)?;
                self.builder.build_struct_get(object, field)
            }

            Expression::Grouping(expr) => self.generate_expression(&**expr)?,

            Expression::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond = self.generate_expression(&**condition)?;
                if let MIRType::Bool = cond.get_type() {
                } else {
                    return Err(Self::anon_err(
                        condition.get_token().or_else(|| then_branch.get_token()),
                        "If condition must be a boolean",
                    ));
                };

                let func = self.builder.cur_fn();
                let mut func = func.borrow_mut();
                let mut then_b = func.append_block("then".to_string());
                let mut else_b = func.append_block("else".to_string());
                let cont_b = func.append_block("cont".to_string());
                // Setting return will mutably borrow the func; drop this one to prevent panic
                drop(func);

                self.builder.set_return(MIRFlow::Branch {
                    condition: cond,
                    then_b: Rc::clone(&then_b),
                    else_b: Rc::clone(&else_b),
                });

                self.builder.set_block(&then_b);
                let then_val = self.generate_expression(&**then_branch)?;
                then_b = self.builder.cur_block_name();
                self.builder.set_return(MIRFlow::Jump(Rc::clone(&cont_b)));

                self.builder.set_block(&else_b);
                if let Some(else_branch) = else_branch {
                    let else_val = self.generate_expression(&**else_branch)?;
                    else_b = self.builder.cur_block_name();
                    self.builder.set_return(MIRFlow::Jump(Rc::clone(&cont_b)));
                    self.builder.set_block(&cont_b);

                    if then_val.get_type() == else_val.get_type() {
                        return Ok(self.builder.build_phi(vec![
                            (then_val, Rc::clone(&then_b)),
                            (else_val, Rc::clone(&else_b)),
                        ]));
                    } else {
                        self.builder.set_block(&else_b);
                        self.builder.insert_at_ptr(else_val);
                    }
                }

                self.builder.set_block(&then_b);
                self.builder.insert_at_ptr(then_val);

                self.builder.set_block(&else_b);
                self.builder.set_return(MIRFlow::Jump(Rc::clone(&cont_b)));

                self.builder.set_block(&cont_b);
                MIRGenerator::none_const()
            }

            Expression::Literal(literal) => self.builder.build_literal(literal.clone()),

            Expression::Return(val) => {
                let value = val
                    .as_ref()
                    .map(|v| self.generate_expression(&*v))
                    .transpose()?;
                let type_ = value
                    .as_ref()
                    .map(|v| v.get_type())
                    .unwrap_or(MIRType::None);

                if type_ != self.builder.cur_fn().borrow().ret_type {
                    return Err(Self::anon_err(
                        val.as_ref().map(|v| v.get_token()).flatten(),
                        "Return expression in function has wrong type",
                    ));
                }

                self.builder
                    .set_return(MIRFlow::Return(value.unwrap_or_else(Self::none_const)));
                Self::none_const()
            }

            Expression::Set {
                object,
                name,
                value,
            } => {
                let (object, field) = self.get_class_field(&**object, name)?;
                let value = self.generate_expression(&**value)?;

                if value.get_type() != field._type {
                    return Err(Self::error(name, name, "Struct member is a different type"));
                }
                if !field.mutable {
                    return Err(Self::error(name, name, "Cannot set immutable class member"));
                }

                self.builder.build_struct_set(object, field, value)
            }

            Expression::Unary { operator, right } => {
                let right = self.generate_expression(&**right)?;

                match operator.t_type {
                    Type::Minus => self.builder.build_unary(right, operator.t_type),
                    Type::Bang => {
                        if right.get_type() == MIRType::Bool {
                            self.builder.build_unary(right, operator.t_type)
                        } else {
                            return Err(Self::error(
                                operator,
                                operator,
                                "'!' can only be used on boolean values",
                            ));
                        }
                    }
                    _ => panic!("Invalid unary expression"),
                }
            }

            Expression::Variable(var) => {
                let var = self.find_var(&var)?;
                self.builder.build_load(var)
            }

            Expression::When {
                value,
                branches,
                else_branch,
            } => {
                let start_b = self.builder.cur_block_name();

                let value = self.generate_expression(value)?;
                let val_type = value.get_type();

                let function_rc = self.builder.cur_fn();
                let mut function = function_rc.borrow_mut();
                let else_b = function.append_block("when-else".to_string());
                let cont_b = function.append_block("when-cont".to_string());
                println!("{:#?}", function);
                drop(function);

                self.builder.set_block(&else_b);
                let else_val = self.generate_expression(else_branch)?;
                let branch_type = else_val.get_type();
                self.builder.set_return(MIRFlow::Jump(Rc::clone(&cont_b)));

                let mut cases = Vec::with_capacity(branches.len());
                let mut phi_nodes = Vec::with_capacity(branches.len());
                for (b_val, branch) in branches.iter() {
                    self.builder.set_block(&start_b);
                    let val = self.generate_expression(b_val)?;
                    if val.get_type() != val_type {
                        return Err(Self::anon_err(
                            b_val.get_token(), "Branches of when must be of same type as the value compared."
                        ))
                    }
                    let val = self.builder.build_binary(val, Type::EqualEqual, value.clone());

                    let mut function = function_rc.borrow_mut();
                    let branch_b = function.append_block("when-br".to_string());
                    drop(function);
                    self.builder.set_block(&branch_b);
                    let branch_val = self.generate_expression(branch)?;
                    if branch_val.get_type() != branch_type {
                        return Err(Self::anon_err(
                            branch.get_token(), "Branch results must be of same type."
                        ))
                    }
                    self.builder.set_return(MIRFlow::Jump(Rc::clone(&cont_b)));

                    let branch_b = self.builder.cur_block_name();
                    cases.push((val, Rc::clone(&branch_b)));
                    phi_nodes.push((branch_val, branch_b))
                }

                phi_nodes.push((else_val, Rc::clone(&else_b)));

                self.builder.set_block(&start_b);
                self.builder.set_return(MIRFlow::Switch {
                    cases,
                    default: else_b
                });

                self.builder.set_block(&cont_b);
                self.builder.build_phi(phi_nodes)
            },

            Expression::VarDef(var) => {
                let init = self.generate_expression(&var.initializer)?;
                let _type = init.get_type();
                let var = self.define_variable(&var.name, !var.is_val, _type);
                self.builder.build_store(var, init)
            }
        })
    }

    /// Defines a new variable. It is put into the variable list in the current function
    /// and placed in the topmost scope.
    fn define_variable(&mut self, token: &Token, mutable: bool, _type: MIRType) -> Rc<MIRVariable> {
        let def = Rc::new(MIRVariable::new(Rc::clone(&token.lexeme), _type, mutable));
        self.builder.add_function_variable(Rc::clone(&def));
        self.insert_variable(Rc::clone(&def), true, token.line).unwrap_or(());
        def
    }

    /// Inserts a variable into the topmost scope.
    /// Note that the variable does NOT get added to the function!
    fn insert_variable(
        &mut self,
        var: Rc<MIRVariable>,
        allow_redefine: bool,
        line: usize,
    ) -> Res<()> {
        let cur_env = self.environments.last_mut().unwrap();
        let was_defined = cur_env
            .insert(Rc::clone(&var.name), Rc::clone(&var))
            .is_some();
        if was_defined && !allow_redefine {
            let mut tok = Token::generic_identifier((*var.name).clone());
            tok.line = line;
            return Err(Self::error(
                &tok,
                &tok,
                &format!(
                    "Cannot redefine variable '{}' in the same scope.",
                    &var.name
                ),
            ));
        }

        Ok(())
    }

    /// Searches all scopes for a variable, starting at the top.
    fn find_var(&mut self, token: &Token) -> Res<Rc<MIRVariable>> {
        for env in self.environments.iter().rev() {
            if let Some(var) = env.get(&token.lexeme) {
                return Ok(Rc::clone(var));
            }
        }

        Err(Self::error(
            token,
            token,
            &format!("Variable '{}' is not defined", token.lexeme),
        ))
    }

    /// Will search for a variable and create it in the topmost scope if it does not exist.
    fn find_or_create_var(&mut self, type_: &MIRType, name: Token) -> Res<Rc<MIRVariable>> {
        let var = self
            .find_var(&name)
            .unwrap_or_else(|_| self.define_variable(&name, true, type_.clone()));

        if &var._type != type_ {
            Err(Self::error(
                &name,
                &name,
                "Break expressions + for body must have same type",
            ))
        } else {
            Ok(var)
        }
    }

    fn get_class_field(
        &mut self,
        object: &Expression,
        name: &Token,
    ) -> Res<(MIRExpression, Rc<MIRStructMem>)> {
        let object = self.generate_expression(object)?;

        if let MIRType::Struct(struc) = object.get_type() {
            Ok((
                object,
                Rc::clone(
                    struc
                        .borrow()
                        .members
                        .get(&name.lexeme)
                        .ok_or_else(|| Self::error(name, name, "Unknown class member"))?,
                ),
            ))
        } else {
            Err(Self::error(
                name,
                name,
                "Get syntax is only supported on class instances",
            ))
        }
    }

    fn generate_func_args(
        &mut self,
        func_ref: MutRc<MIRFunction>,
        arguments: &Vec<Expression>,
    ) -> Res<Vec<MIRExpression>> {
        let func = func_ref.borrow();

        if func.parameters.len() != arguments.len() {
            return Err(Self::anon_err(
                arguments.first().map(|e| e.get_token()).flatten(),
                &format!(
                    "Incorrect amount of function arguments. (Expected {}; got {})",
                    func.parameters.len(),
                    arguments.len()
                ),
            ));
        }

        let mut result = Vec::with_capacity(arguments.len());
        for (argument, parameter) in arguments.iter().zip(func.parameters.iter()) {
            let arg = self.generate_expression(argument)?;
            if arg.get_type() != parameter._type {
                return Err(Self::anon_err(
                    argument.get_token(),
                    &format!("Call argument is the wrong type (was {}; expected {})", arg.get_type(), parameter._type),
                ));
            }
            result.push(arg)
        }

        Ok(result)
    }

    /// Creates a new scope. A new scope is created for every function and block,
    /// in addition to the bottom global scope.
    ///
    /// # Example
    /// (global scope #1)
    /// func main() {       <- new scope (#2) for the class main
    ///     var a = 5       <- a now in scope #2
    ///     {               <- new scope (#3)
    ///         var b = 1   <- b now in scope #3
    ///     }               <- scope #3 gets removed, along with b
    /// }                   <- scope #2 gets removed, along with a
    fn begin_scope(&mut self) {
        self.environments.push(HashMap::new());
    }

    /// Removes the topmost scope.
    fn end_scope(&mut self) {
        self.environments.pop();
    }

    fn none_const() -> MIRExpression {
        MIRExpression::Literal(Literal::None)
    }

    fn error(start: &Token, end: &Token, message: &str) -> Error {
        Error::new(start, end, "MIRGenerator", message.to_string())
    }

    /// Produces an error when the caller cannot gurantee that the expression contains a token.
    /// If it doesn't, the function creates a generic "unknown location" token.
    fn anon_err(tok: Option<&Token>, message: &str) -> Error {
        let generic = Token::generic_token(Type::Identifier);
        let tok = tok.unwrap_or_else(|| &generic);
        Error::new(tok, tok, "MIRGenerator", message.to_string())
    }

    pub fn new() -> Self {
        let mut generator = MIRGenerator {
            builder: MIRBuilder::new(),
            environments: Vec::with_capacity(5),

            is_in_loop: false,
            current_loop_ret_type: None,
            current_loop_cont_block: None,
        };

        // Global scope
        generator.begin_scope();

        generator
    }
}
