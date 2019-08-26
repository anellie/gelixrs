/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/26/19 6:48 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/24/19 8:18 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

mod builder;
mod passes;

use builder::MIRBuilder;
use std::collections::HashMap;
use crate::mir::mir::{MIRVariable, MIRType, MIRExpression};
use crate::ast::declaration::{DeclarationList, Function, FuncSignature};
use crate::ast::expression::{Expression};
use crate::mir::MIR;
use crate::lexer::token::Token;
use std::rc::Rc;
use crate::mir::generator::passes::declare::DeclarePass;
use crate::mir::generator::passes::{PreMIRPass, PostMIRPass};
use crate::mir::generator::passes::fill_struct::FillStructPass;
use crate::mir::generator::passes::typecheck::TypecheckPass;
use crate::ast::literal::Literal;

type Res<T> = Result<T, Error>;

pub struct MIRGenerator {
    builder: MIRBuilder,
    environments: Vec<HashMap<Rc<String>, Rc<MIRVariable>>>,

    is_in_loop: bool,
    current_loop_ret_type: Option<MIRType>,
}

impl MIRGenerator {
    pub fn generate(self, list: DeclarationList) -> Option<MIR> {
        self.run(list)
            .or_else(|err| {
                eprintln!(
                    "[MIRGenerator] {}:
{} |
{} | {}
{} |",
                    err.message,
                    err.line
                        .map(|l| (l - 1).to_string())
                        .unwrap_or("?".to_string()),
                    err.line.map(|l| l.to_string()).unwrap_or("?".to_string()),
                    err.code,
                    err.line
                        .map(|l| (l + 1).to_string())
                        .unwrap_or("?".to_string()),
                );
                Err(())
            })
            .ok()
    }

    fn run(mut self, mut list: DeclarationList) -> Res<MIR> {
        // Run all pre-MIR passes
        DeclarePass::new(&mut self).run(&mut list)?;
        FillStructPass::new(&mut self).run(&mut list)?;

        // Generate the MIR
        self.generate_mir(list)?;

        // Run post-MIR passes
        TypecheckPass::new(&mut self).run()?;

        // Finally, return the finished MIR
        Ok(self.builder.get_mir())
    }

    fn generate_mir(&mut self, list: DeclarationList) -> Result<(), Error> {
        for func in list.functions.into_iter().chain(
            list.classes
                .into_iter()
                .map(|class| class.methods)
                .flatten()
        ) {
            self.generate_function(func)?;
        }

        Ok(())
    }

    fn generate_function(&mut self, func: Function) -> Res<()> {
        let function_rc = self.builder.find_function(&func.sig.name.lexeme).unwrap();
        let mut function = function_rc.borrow_mut();
        function.append_block("entry".to_string());
        drop(function);
        self.builder.set_pointer(Rc::clone(&function_rc), Rc::new("entry".to_string()));

        self.begin_scope();
        for param in function_rc.borrow().parameters.iter() {
            self.insert_variable(Rc::clone(param), false, func.sig.name.line)?;
        }

        let body_type = self.generate_expression(func.body)?.get_type();
        if body_type != function_rc.borrow().ret_type {
            Err(Error::new_fn(
                "Function return type does not match body type",
                &func.sig,
            ))?;
        }

        self.end_scope();
        Ok(())
    }

    fn generate_expression(&mut self, expression: Expression) -> Res<MIRExpression> {
        Ok(match expression {
            Expression::Assignment { name, value } => {
                let var = self.find_var(&name)?;
                if var.mutable {
                    let value = self.generate_expression(*value)?;
                    self.builder.build_store(var, value)
                } else {
                    Err(Error::new(
                        Some(name.line),
                        &format!("Variable {} is not assignable (val)", name.lexeme),
                        name.lexeme.to_string(),
                    ))?
                }
            }

            Expression::Binary { left, operator, right } => {
                let left = self.generate_expression(*left)?;
                let right = self.generate_expression(*right)?;
                self.builder.build_binary(left, operator, right)
            }

            Expression::Block(expressions) => {
                expressions
                    .into_iter()
                    .try_fold(MIRGenerator::none_const(), |_, expr| self.generate_expression(expr))?
            }

            Expression::Break(_) => unimplemented!(),

            Expression::Call { callee, arguments } => {
                let callee = self.generate_expression(*callee)?;

                let mut args = Vec::with_capacity(arguments.len());
                for arg in arguments {
                    args.push(self.generate_expression(arg)?)
                }

                self.builder.build_call(callee, args)
            }

            Expression::For { condition: _, body: _ } => unimplemented!(),
            Expression::Get { object: _, name: _ } => unimplemented!(),
            Expression::Grouping(_) => unimplemented!(),
            Expression::If { condition: _, then_branch: _, else_branch: _ } => unimplemented!(),
            Expression::Literal(_) => unimplemented!(),
            Expression::Return(_) => unimplemented!(),
            Expression::Set { object: _, name: _, value: _ } => unimplemented!(),
            Expression::Unary { operator: _, right: _ } => unimplemented!(),
            Expression::Variable(_) => unimplemented!(),
            Expression::When { value: _, branches: _, else_branch: _ } => unimplemented!(),
            Expression::VarDef(_) => unimplemented!(),
        })
    }

    fn define_variable(
        &mut self,
        token: &mut Token,
        mutable: bool,
        _type: MIRType,
    ) -> Res<()> {
        let def = Rc::new(MIRVariable::new(Rc::clone(&token.lexeme), _type, mutable));
        self.builder.add_function_variable(Rc::clone(&def));
        self.insert_variable(def, true, token.line)?;
        Ok(())
    }

    fn insert_variable(&mut self, var: Rc<MIRVariable>, allow_redefine: bool, line: usize) -> Res<()> {
        let cur_env = self.environments.last_mut().unwrap();
        let was_defined = cur_env.insert(Rc::clone(&var.name), Rc::clone(&var)).is_some();
        if was_defined && !allow_redefine {
            Err(Error {
                line: Some(line),
                message: format!("Cannot redefine variable '{}' in the same scope.", &var.name),
                code: (*var.name).clone(),
            })?;
        }

        Ok(())
    }

    fn find_var(&mut self, token: &Token) -> Res<Rc<MIRVariable>> {
        for env in self.environments.iter().rev() {
            if let Some(var) = env.get(&token.lexeme) {
                return Ok(Rc::clone(var))
            }
        }

        Err(Error {
            line: Some(token.line),
            message: format!("Variable '{}' is not defined", token.lexeme),
            code: (*token.lexeme).clone(),
        })
    }

    fn begin_scope(&mut self) {
        self.environments.push(HashMap::new());
    }

    fn end_scope(&mut self) {
        self.environments.pop();
    }

    fn none_const() -> MIRExpression {
        MIRExpression::Literal(Literal::None)
    }

    pub fn new() -> MIRGenerator {
        MIRGenerator {
            builder: MIRBuilder::new(),
            environments: Vec::with_capacity(5),

            is_in_loop: false,
            current_loop_ret_type: None,
        }
    }
}


pub struct Error {
    pub line: Option<usize>,
    pub message: String,
    pub code: String,
}

impl Error {
    pub fn new_expr(message: String, expr: &Expression) -> Error {
        Error {
            line: expr.get_line(),
            message,
            code: expr.to_string(),
        }
    }

    pub fn new_fn(message: &str, func_sig: &FuncSignature) -> Error {
        Error {
            line: Some(func_sig.name.line),
            message: message.to_string(),
            code: format!("func {}(...)", func_sig.name.lexeme),
        }
    }

    pub fn new(line: Option<usize>, message: &str, code: String) -> Error {
        Error { line, message: message.to_owned(), code }
    }
}
