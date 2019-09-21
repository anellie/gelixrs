/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/21/19 4:30 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use either::Either;

use builder::MIRBuilder;

use crate::{Error, ModulePath};
use crate::ast::declaration::Function;
use crate::ast::expression::Expression;
use crate::ast::literal::Literal;
use crate::ast::module::Module;
use crate::lexer::token::{Token, Type};
use crate::mir::{MIRModule, MutRc};
use crate::mir::nodes::{
    MIRArray, MIRExpression, MIRFlow, MIRFunction, MIRStructMem, MIRType, MIRVariable,
};

mod builder;
pub mod module;
mod passes;

type Res<T> = Result<T, MIRError>;

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

    /// The current loop, if in one.
    current_loop: Option<ForLoop>,
}

impl MIRGenerator {
    fn generate_mir(&mut self, module: &Module) -> Res<()> {
        for func in module
            .functions
            .iter()
            .chain(module.classes.iter().map(|class| &class.methods).flatten())
        {
            self.generate_function(func)?;
        }

        Ok(())
    }

    fn generate_function(&mut self, func: &Function) -> Res<()> {
        let function_rc = self.builder.find_function(&func.sig.name.lexeme).unwrap();
        let mut function = function_rc.borrow_mut();
        let func_type = function.ret_type.clone();
        let entry = function.append_block("entry".to_string());
        drop(function);
        self.builder
            .set_pointer(Rc::clone(&function_rc), Rc::clone(&entry));

        self.begin_scope();
        for param in function_rc.borrow().parameters.iter() {
            self.insert_variable(Rc::clone(param), false, func.sig.name.line)?;
        }

        let body = self.generate_expression(&func.body)?;
        if func_type != MIRType::None {
            if func_type == body.get_type() {
                self.builder.set_return(MIRFlow::Return(body));
            } else {
                return Err(self.error(
                    &func.sig.name,
                    func.sig
                        .return_type
                        .as_ref()
                        .map(|t| t.get_token())
                        .flatten()
                        .unwrap_or(&func.sig.name),
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
                        return Err(self.error(
                            &name,
                            &name,
                            &format!("Variable {} is a different type", name.lexeme),
                        ));
                    }
                } else {
                    return Err(self.error(
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

                if (left.get_type() == MIRType::I64) && (right.get_type() == MIRType::I64) {
                    self.builder.build_binary(left, operator.t_type, right)
                } else {
                    return Err(self.error(
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
                if self.current_loop.is_none() {
                    return Err(self.anon_err(
                        expr.as_ref().map(|e| e.get_token()).flatten(),
                        "Break is only allowed in loops.",
                    ));
                }

                if let Some(expression) = expr {
                    let expression = self.generate_expression(&**expression)?;
                    self.get_or_create_loop_var(&expression.get_type())?;
                    let cur_block = self.builder.cur_block_name();
                    self.cur_loop().phi_nodes.push((expression, cur_block));
                }

                let cont_block = Rc::clone(&self.cur_loop().cont_block);
                self.builder.build_jump(&cont_block);
                Self::any_const()
            }

            Expression::Call { callee, arguments } => {
                match &**callee {
                    // Method call
                    Expression::Get { object, name } => {
                        let (object, field) = self.get_class_field(object, name)?;
                        let func = field.right().ok_or_else(|| {
                            self.error(name, name, "Class members cannot be called.")
                        })?;
                        let args =
                            self.generate_func_args(Rc::clone(&func), arguments, Some(object))?;
                        return Ok(self.builder.build_call(MIRExpression::Function(func), args));
                    }

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
                    let args = self.generate_func_args(func, arguments, None)?;
                    self.builder.build_call(callee_mir, args)
                } else {
                    return Err(self.anon_err(
                        callee.get_token(),
                        "Only functions are allowed to be called",
                    ));
                }
            }

            Expression::CallWithGeneric { .. } => unimplemented!(),

            Expression::For {
                condition,
                body,
                else_b,
            } => {
                let loop_block = self.builder.append_block("for-loop");
                let mut else_block = self.builder.append_block("for-else");
                let cont_block = self.builder.append_block("for-cont");

                let prev_loop =
                    std::mem::replace(&mut self.current_loop, Some(ForLoop::new(&cont_block)));

                let cond = self.generate_expression(&**condition)?;
                if cond.get_type() != MIRType::Bool {
                    return Err(
                        self.anon_err(condition.get_token(), "For condition must be a boolean.")
                    );
                }

                self.builder
                    .build_branch(cond.clone(), &loop_block, &else_block);

                self.builder.set_block(&loop_block);
                let body = self.generate_expression(&**body)?;
                let body_type = body.get_type();

                let loop_end_block = self.builder.cur_block_name();
                let body_alloca = self.get_or_create_loop_var(&body_type)?;

                let store = self.builder.build_store(Rc::clone(&body_alloca), body);
                self.builder.insert_at_ptr(store);
                self.builder.build_branch(cond, &loop_block, &cont_block);

                let mut ret = Self::none_const();
                if let Some(else_b) = else_b {
                    self.builder.set_block(&else_block);
                    let else_val = self.generate_expression(&**else_b)?;
                    else_block = self.builder.cur_block_name();

                    if else_val.get_type() == body_type {
                        self.builder.set_block(&cont_block);

                        let load = self.builder.build_load(body_alloca);
                        self.cur_loop()
                            .phi_nodes
                            .push((load, Rc::clone(&loop_end_block)));
                        self.cur_loop()
                            .phi_nodes
                            .push((else_val, Rc::clone(&else_block)));

                        ret = self
                            .builder
                            .build_phi(self.current_loop.take().unwrap().phi_nodes)
                    }
                }

                self.builder.set_block(&else_block);
                self.builder.build_jump(&cont_block);
                self.builder.set_block(&cont_block);
                self.current_loop = prev_loop;

                ret
            }

            Expression::Get { object, name } => {
                let (object, field) = self.get_class_field(&**object, name)?;
                let field = field.left().ok_or_else(|| {
                    self.error(name, name, "Cannot get class method (must be called)")
                })?;
                self.builder.build_struct_get(object, field)
            }

            Expression::Grouping(expr) => self.generate_expression(&**expr)?,

            Expression::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond = self.generate_expression(&**condition)?;
                if cond.get_type() != MIRType::Bool {
                    return Err(self.anon_err(
                        condition.get_token().or_else(|| then_branch.get_token()),
                        "If condition must be a boolean",
                    ));
                }

                let mut then_block = self.builder.append_block("then");
                let mut else_block = self.builder.append_block("else");
                let cont_block = self.builder.append_block("cont");

                self.builder.build_branch(cond, &then_block, &else_block);

                self.builder.set_block(&then_block);
                let then_val = self.generate_expression(&**then_branch)?;
                then_block = self.builder.cur_block_name();

                self.builder.set_block(&else_block);
                if let Some(else_branch) = else_branch {
                    let else_val = self.generate_expression(&**else_branch)?;
                    else_block = self.builder.cur_block_name();

                    if then_val.get_type() == else_val.get_type() {
                        self.builder.build_jump(&cont_block);
                        self.builder.set_block(&then_block);
                        self.builder.build_jump(&cont_block);

                        self.builder.set_block(&cont_block);
                        return Ok(self
                            .builder
                            .build_phi(vec![(then_val, then_block), (else_val, else_block)]));
                    } else {
                        self.builder.insert_at_ptr(else_val);
                        self.builder.build_jump(&cont_block);
                    }
                } else {
                    self.builder.set_block(&else_block);
                    self.builder.build_jump(&cont_block);
                }

                self.builder.set_block(&then_block);
                self.builder.insert_at_ptr(then_val);
                self.builder.build_jump(&cont_block);

                self.builder.set_block(&cont_block);
                Self::none_const()
            }

            Expression::Literal(literal) => {
                if let Literal::Array(arr) = literal {
                    let ast_values = arr.as_ref().left().unwrap();
                    let mut values_mir = Vec::new();
                    let mut ast_values = ast_values.iter();
                    let first = self.generate_expression(ast_values.next().unwrap())?;
                    let arr_type = first.get_type();

                    values_mir.push(first);
                    for value in ast_values {
                        let mir_val = self.generate_expression(value)?;

                        if mir_val.get_type() != arr_type {
                            return Err(self.anon_err(
                                value.get_token(),
                                &format!(
                                    "Type of array value ({}) does not rest of array ({}).",
                                    mir_val.get_type(),
                                    arr_type
                                ),
                            ));
                        }

                        values_mir.push(mir_val);
                    }

                    self.builder
                        .build_literal(Literal::Array(Either::Right(MIRArray {
                            values: values_mir,
                            type_: arr_type,
                        })))
                } else {
                    self.builder.build_literal(literal.clone())
                }
            }

            Expression::Return(val) => {
                let value = val
                    .as_ref()
                    .map(|v| self.generate_expression(&*v))
                    .transpose()?
                    .unwrap_or_else(Self::none_const);

                if value.get_type() != self.builder.cur_fn().borrow().ret_type {
                    return Err(self.anon_err(
                        val.as_ref().map(|v| v.get_token()).flatten(),
                        "Return expression in function has wrong type",
                    ));
                }

                self.builder.set_return(MIRFlow::Return(value));
                Self::any_const()
            }

            Expression::Set {
                object,
                name,
                value,
            } => {
                let (object, field) = self.get_class_field(&**object, name)?;
                let field = field
                    .left()
                    .ok_or_else(|| self.error(name, name, "Cannot set class method"))?;
                let value = self.generate_expression(&**value)?;

                if value.get_type() != field._type {
                    return Err(self.error(name, name, "Class member is a different type"));
                }
                if !field.mutable {
                    return Err(self.error(name, name, "Cannot set immutable class member"));
                }

                self.builder.build_struct_set(object, field, value)
            }

            Expression::Unary { operator, right } => {
                let right = self.generate_expression(&**right)?;

                match operator.t_type {
                    Type::Bang if right.get_type() != MIRType::Bool => Err(self.error(
                        operator,
                        operator,
                        "'!' can only be used on boolean values",
                    )),

                    _ => Ok(()),
                }?;

                self.builder.build_unary(right, operator.t_type)
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

                let else_b = self.builder.append_block("when-else");
                let cont_b = self.builder.append_block("when-cont");

                self.builder.set_block(&else_b);
                let else_val = self.generate_expression(else_branch)?;
                let branch_type = else_val.get_type();
                self.builder.build_jump(&cont_b);

                let mut cases = Vec::with_capacity(branches.len());
                let mut phi_nodes = Vec::with_capacity(branches.len());
                for (b_val, branch) in branches.iter() {
                    self.builder.set_block(&start_b);
                    let val = self.generate_expression(b_val)?;
                    if val.get_type() != val_type {
                        return Err(self.anon_err(
                            b_val.get_token(),
                            "Branches of when must be of same type as the value compared.",
                        ));
                    }
                    let val = self
                        .builder
                        .build_binary(val, Type::EqualEqual, value.clone());

                    let branch_b = self.builder.append_block("when-br");
                    self.builder.set_block(&branch_b);
                    let branch_val = self.generate_expression(branch)?;
                    if branch_val.get_type() != branch_type {
                        return Err(self
                            .anon_err(branch.get_token(), "Branch results must be of same type."));
                    }
                    self.builder.build_jump(&cont_b);

                    let branch_b = self.builder.cur_block_name();
                    cases.push((val, Rc::clone(&branch_b)));
                    phi_nodes.push((branch_val, branch_b))
                }

                phi_nodes.push((else_val, Rc::clone(&else_b)));

                self.builder.set_block(&start_b);
                self.builder.set_return(MIRFlow::Switch {
                    cases,
                    default: else_b,
                });

                self.builder.set_block(&cont_b);
                self.builder.build_phi(phi_nodes)
            }

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
        self.insert_variable(Rc::clone(&def), true, token.line)
            .unwrap_or(());
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
            return Err(self.error(
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

        self.builder.find_global(&token.lexeme).ok_or_else(|| {
            self.error(
                token,
                token,
                &format!("Variable '{}' is not defined", token.lexeme),
            )
        })
    }

    /// Returns the variable of the current loop or creates it if it does not exist yet
    fn get_or_create_loop_var(&mut self, type_: &MIRType) -> Res<Rc<MIRVariable>> {
        let var = self.cur_loop().result_var.clone().unwrap_or_else(|| {
            self.define_variable(
                &Token::generic_identifier("for-body".to_string()),
                true,
                type_.clone(),
            )
        });
        self.cur_loop().result_var = Some(Rc::clone(&var));

        if &var._type != type_ {
            Err(self.anon_err(None, "Break expressions + for body must have same type"))
        } else {
            Ok(var)
        }
    }

    fn get_class_field(
        &mut self,
        object: &Expression,
        name: &Token,
    ) -> Res<(MIRExpression, Either<Rc<MIRStructMem>, MutRc<MIRFunction>>)> {
        let object = self.generate_expression(object)?;

        if let MIRType::Struct(struc) = object.get_type() {
            let struc = struc.borrow();

            // Class fields
            let field = struc.members.get(&name.lexeme);
            if let Some(field) = field {
                return Ok((object, Either::Left(Rc::clone(field))));
            }

            // Class methods
            let method = struc.methods.get(&name.lexeme);
            if let Some(method) = method {
                return Ok((object, Either::Right(Self::var_to_function(method))));
            }

            // Superclass methods
            let mut sclass = struc.super_struct.clone();
            while let Some(struc) = sclass {
                let struc = struc.borrow();
                let method = struc.methods.get(&name.lexeme);
                if let Some(method) = method {
                    return Ok((object, Either::Right(Self::var_to_function(method))));
                }
                sclass = struc.super_struct.clone();
            }

            // Nothing found...
            Err(self.error(name, name, "Unknown class field"))
        } else {
            Err(self.error(
                name,
                name,
                "Get syntax is only supported on class instances",
            ))
        }
    }

    fn var_to_function(var: &Rc<MIRVariable>) -> MutRc<MIRFunction> {
        if let MIRType::Function(f) = &var._type {
            Rc::clone(&f)
        } else {
            panic!()
        }
    }

    fn generate_func_args(
        &mut self,
        func_ref: MutRc<MIRFunction>,
        arguments: &[Expression],
        first_arg: Option<MIRExpression>,
    ) -> Res<Vec<MIRExpression>> {
        let func = func_ref.borrow();

        let args_len = arguments.len() + (first_arg.is_some() as usize);
        if func.parameters.len() != args_len {
            return Err(self.anon_err(
                arguments.first().map(|e| e.get_token()).flatten(),
                &format!(
                    "Incorrect amount of function arguments. (Expected {}; got {})",
                    func.parameters.len(),
                    arguments.len()
                ),
            ));
        }

        let mut result = Vec::with_capacity(args_len);
        let first_arg_is_some = first_arg.is_some();
        if let Some(arg) = first_arg {
            let ty = &func.parameters[0]._type;
            let arg = self
                .check_call_arg_type(arg, ty)
                .expect("internal error: method call");
            result.push(arg)
        }
        for (argument, parameter) in arguments
            .iter()
            .zip(func.parameters.iter().skip(first_arg_is_some as usize))
        {
            let arg = self.generate_expression(argument)?;
            let arg_type = arg.get_type();
            let arg = self
                .check_call_arg_type(arg, &parameter._type)
                .ok_or_else(|| {
                    self.anon_err(
                        argument.get_token(),
                        &format!(
                            "Call argument is the wrong type (was {}, expected {})",
                            arg_type, parameter._type
                        ),
                    )
                })?;
            result.push(arg)
        }

        Ok(result)
    }

    /// Checks if the arg parameter is of the given type ty.
    /// Will do class casts if needed to make the types match;
    /// returns the new expression that should be used in case a cast happened.
    fn check_call_arg_type(&self, mut arg: MIRExpression, ty: &MIRType) -> Option<MIRExpression> {
        let arg_type = arg.get_type();
        if &arg_type == ty {
            Some(arg)
        } else if let MIRType::Struct(struc) = arg_type {
            let mut sclass = struc.borrow().super_struct.clone();
            while let Some(struc) = sclass {
                arg = self.builder.build_bitcast(arg, &struc);
                if ty == &MIRType::Struct(Rc::clone(&struc)) {
                    return Some(arg);
                }
                sclass = struc.borrow().super_struct.clone();
            }
            None
        } else {
            None
        }
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

    fn cur_loop(&mut self) -> &mut ForLoop {
        self.current_loop.as_mut().unwrap()
    }

    fn any_const() -> MIRExpression {
        MIRExpression::Literal(Literal::Any)
    }

    fn none_const() -> MIRExpression {
        MIRExpression::Literal(Literal::None)
    }

    fn error(&self, start: &Token, end: &Token, message: &str) -> MIRError {
        MIRError {
            error: Error::new(start, end, "MIRGenerator", message.to_string()),
            module: self.builder.module_path(),
        }
    }

    /// Produces an error when the caller cannot guarantee that the expression contains a token.
    /// If it doesn't, the function creates a generic "unknown location" token.
    fn anon_err(&self, tok: Option<&Token>, message: &str) -> MIRError {
        let generic = Token::generic_token(Type::Identifier);
        let tok = tok.unwrap_or_else(|| &generic);
        MIRError {
            error: Error::new(tok, tok, "MIRGenerator", message.to_string()),
            module: self.builder.module_path(),
        }
    }

    pub fn new(path: Rc<ModulePath>) -> Self {
        MIRGenerator {
            builder: MIRBuilder::new(MIRModule::new(path)),
            environments: Vec::with_capacity(5),
            current_loop: None,
        }
    }
}

/// All data of a loop.
struct ForLoop {
    /// The alloca of the for loop. Can be None for loops that return None type.
    result_var: Option<Rc<MIRVariable>>,
    /// The block to jump to when the current loop finishes.
    cont_block: Rc<String>,
    /// The phi nodes of the loop (loops are expressions).
    phi_nodes: Vec<(MIRExpression, Rc<String>)>,
}

impl ForLoop {
    fn new(cont_block: &Rc<String>) -> ForLoop {
        ForLoop {
            result_var: None,
            cont_block: Rc::clone(cont_block),
            phi_nodes: vec![],
        }
    }
}

pub struct MIRError {
    pub error: Error,
    pub module: Rc<ModulePath>,
}

impl MIRError {
    pub fn to_string(&self, mut root: PathBuf) -> String {
        // skip(1): Skip the root of the module to prevent having it in the path twice
        for path in self.module.iter().skip(1) {
            root.push(&*path.clone());
        }
        if root.is_dir() {
            // Can be a dir if the module is named module.gel, in which case its path
            // will be its containing directory.
            root.push("module.gel")
        }
        root.set_extension("gel");

        let code = std::fs::read_to_string(root.clone());

        format!(
            "Error in file {}:\n{}",
            root.display(),
            self.error.to_string(&code.unwrap())
        )
    }
}
