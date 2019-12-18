/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/18/19 3:53 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use either::Either;

use crate::ast::{Expression as ASTExpr, Literal};
use crate::ast::declaration::Variable as ASTVar;
use crate::ast::Type as ASTType;
use crate::error::{Error, Res};
use crate::lexer::token::{Token, TType};
use crate::mir::generator::{ForLoop, MIRGenerator};
use crate::mir::MutRc;
use crate::mir::nodes::{ArrayLiteral, Class, Expr, Flow, Type, Variable};
use crate::mir::result::ToMIRResult;

/// This impl contains all code of the generator that directly
/// produces expressions.
/// This is split into its own file for readability reasons;
/// a 1500-line file containing everything is difficult to navigate.
impl MIRGenerator {
    pub fn expression(&mut self, expression: &ASTExpr) -> Res<Expr> {
        match expression {
            ASTExpr::Assignment { name, value } => self.assignment(name, value),

            ASTExpr::Binary {
                left,
                operator,
                right,
            } => self.binary(left, operator, right),

            ASTExpr::Block(expressions, _) => self.block(expressions),

            ASTExpr::Break(expr, tok) => self.break_(expr, tok),

            ASTExpr::Call { callee, arguments } => self.call(callee, arguments),

            ASTExpr::For {
                condition,
                body,
                else_b,
            } => self.for_(condition, body, else_b),

            ASTExpr::Get { object, name } => self.get(object, name),

            ASTExpr::If {
                condition,
                then_branch,
                else_branch,
            } => self.if_(condition, then_branch, else_branch),

            ASTExpr::Literal(literal, _) => self.literal(literal),

            ASTExpr::Return(val, err_tok) => self.return_(val, err_tok),

            ASTExpr::Set {
                object,
                name,
                value,
            } => self.set(object, name, value),

            ASTExpr::Unary { operator, right } => self.unary(operator, right),

            ASTExpr::Variable(var) => self.var(var),

            ASTExpr::VarWithGenerics { name, generics } => self.var_with_generics(name, generics),

            ASTExpr::When {
                value,
                branches,
                else_branch,
            } => self.when(value, branches, else_branch),

            ASTExpr::VarDef(var) => self.var_def(var),
        }
    }

    fn assignment(&mut self, name: &Token, value: &ASTExpr) -> Res<Expr> {
        let var = self.find_var(&name)?;
        let value = self.expression(value)?;
        let val_ty = value.get_type();

        match var.mutable {
            true if val_ty == var.type_ => Ok(Expr::store(&var, value)),
            false => Err(self.err(
                &name,
                &format!("Variable {} is not assignable (val)", name.lexeme),
            )),
            _ => Err(self.err(
                &name,
                &format!("Variable {} is a different type", name.lexeme),
            )),
        }
    }

    fn binary(&mut self, left: &ASTExpr, operator: &Token, right: &ASTExpr) -> Res<Expr> {
        let left = self.expression(left)?;
        let right = self.expression(right)?;
        self.binary_mir(left, operator, right)
    }

    fn binary_mir(&mut self, left: Expr, operator: &Token, right: Expr) -> Res<Expr> {
        let left_ty = left.get_type();
        let right_ty = right.get_type();

        if left_ty == right_ty && left_ty.is_number() {
            Ok(Expr::binary(left, operator.t_type, right))
        } else {
            let method_var = self
                .get_operator_overloading_method(operator.t_type, &left_ty, &right_ty)
                .or_err(
                    &self.builder.path,
                    operator,
                    "No implementation of operator found for types.",
                )?;

            let mut expr = Expr::call(Expr::load(&method_var), vec![left, right]);
            if operator.t_type == TType::BangEqual {
                expr = Expr::unary(expr, TType::Bang);
            }
            Ok(expr)
        }
    }

    fn block(&mut self, expressions: &[ASTExpr]) -> Res<Expr> {
        if expressions.is_empty() {
            return Ok(Expr::none_const());
        }

        self.begin_scope();
        for expression in expressions.iter().take(expressions.len() - 1) {
            let expression = self.expression(&expression)?;
            self.insert_at_ptr(expression);
        }
        let last = self.expression(expressions.last().unwrap())?;

        self.end_scope();
        Ok(last)
    }

    fn break_(&mut self, expr: &Option<Box<ASTExpr>>, err_tok: &Token) -> Res<Expr> {
        if self.current_loop.is_none() {
            return Err(self.err(err_tok, "Break is only allowed in loops."));
        }

        if let Some(expression) = expr {
            let expression = self.expression(&**expression)?;
            self.get_or_create_loop_var(&expression.get_type())?;
            let cur_block = self.cur_block_name();
            self.cur_loop().phi_nodes.push((expression, cur_block));
        }

        let jmp = Expr::jump(&self.cur_loop().cont_block);
        self.insert_at_ptr(jmp);
        Ok(Expr::any_const())
    }

    fn call(&mut self, callee: &ASTExpr, arguments: &[ASTExpr]) -> Res<Expr> {
        if let Some(expr) = self.try_method_or_constructor(callee, arguments)? {
            return Ok(expr);
        }

        // method above fell through, its either a function call or invalid
        let callee_mir = self.expression(callee)?;
        if let Type::Function(func) = callee_mir.get_type() {
            let args = self.generate_func_args(func, arguments, None, callee.get_token())?;
            Ok(Expr::call(callee_mir, args))
        } else {
            Err(self.err(
                callee.get_token(),
                "Only functions are allowed to be called",
            ))
        }
    }

    /// TODO
    fn try_method_or_constructor(
        &mut self,
        callee: &ASTExpr,
        arguments: &[ASTExpr],
    ) -> Res<Option<Expr>> {
        match callee {
            // Method call
            ASTExpr::Get { object, name } => {
                if !self.uninitialized_this_members.is_empty() {
                    return Err(self.err(name, "Cannot call methods in constructors until all class members are initialized."));
                }

                let (object, field) = self.get_field(object, name)?;
                let func = field.right().or_err(
                    &self.builder.path,
                    name,
                    "Class members cannot be called.",
                )?;

                let args = self.generate_func_args(
                    Rc::clone(func.type_.as_function()),
                    arguments,
                    Some(object),
                    name,
                )?;

                Ok(Some(Expr::call(Expr::load(&func), args)))
            }

            // Class constructor
            ASTExpr::Variable(name) => {
                let ty = self.builder.find_type(&ASTType::Ident(name.clone()));
                if let Ok(Type::Class(class)) = ty {
                    Ok(Some(
                        self.generate_class_instantiation(class, arguments, name)?,
                    ))
                } else {
                    Ok(None)
                }
            }

            // Prototype constructor
            ASTExpr::VarWithGenerics { name, generics } => {
                let ty = self.builder.find_type(&ASTType::Generic { token: name.clone(), types: generics.clone() });
                if let Ok(Type::Class(class)) = ty {
                    Ok(Some(
                        self.generate_class_instantiation(class, arguments, name)?,
                    ))
                } else {
                    Ok(None)
                }
            }

            _ => Ok(None),
        }
    }

    fn generate_class_instantiation(
        &mut self,
        class: MutRc<Class>,
        args: &[ASTExpr],
        err_tok: &Token,
    ) -> Res<Expr> {
        let mut args = args
            .iter()
            .map(|arg| self.expression(arg))
            .collect::<Res<Vec<Expr>>>()?;
        let inst = self.build_class_inst(Rc::clone(&class));
        args.insert(0, inst.clone());

        let class = class.borrow();
        let constructor: &Rc<Variable> = class
            .constructors
            .iter()
            .find(|constructor| {
                let constructor = constructor.type_.as_function().borrow();
                if constructor.parameters.len() != args.len() {
                    return false;
                }
                for (param, arg) in constructor.parameters.iter().zip(args.iter()) {
                    if param.type_ != arg.get_type() {
                        return false;
                    }
                }
                true
            })
            .or_err(
                &self.builder.path,
                err_tok,
                "No matching constructor found for arguments.",
            )?;

        let call = Expr::call(Expr::load(constructor), args);
        self.insert_at_ptr(call);
        Ok(inst)
    }

    /// Builds a class instance and returns an expression that loads the instance.
    /// The expression returned can be safely cloned to reuse the instance.
    fn build_class_inst(&mut self, class_ref: MutRc<Class>) -> Expr {
        let call = {
            let class = class_ref.borrow();
            Expr::call(Expr::load(&class.instantiator), vec![])
        };

        let var = Rc::new(Variable {
            mutable: true,
            type_: Type::Class(class_ref),
            name: Rc::new("tmp-constructor-var".to_string()),
        });
        self.add_function_variable(Rc::clone(&var));
        self.insert_at_ptr(Expr::store(&var, call));

        Expr::load(&var)
    }

    fn for_(
        &mut self,
        condition: &ASTExpr,
        body: &ASTExpr,
        else_b: &Option<Box<ASTExpr>>,
    ) -> Res<Expr> {
        let loop_block = self.append_block("for-loop");
        let mut else_block = self.append_block("for-else");
        let cont_block = self.append_block("for-cont");

        let prev_loop = std::mem::replace(&mut self.current_loop, Some(ForLoop::new(&cont_block)));

        let cond = self.expression(condition)?;
        if cond.get_type() != Type::Bool {
            return Err(self.err(condition.get_token(), "For condition must be a boolean."));
        }

        self.insert_at_ptr(Expr::branch(cond.clone(), &loop_block, &else_block));

        self.set_block(&loop_block);
        let body = self.expression(body)?;
        let body_type = body.get_type();

        let loop_end_block = self.cur_block_name();
        let body_alloca = self.get_or_create_loop_var(&body_type)?;

        self.insert_at_ptr(Expr::store(&body_alloca, body));
        self.insert_at_ptr(Expr::branch(cond, &loop_block, &cont_block));

        let mut ret = Expr::none_const();
        if let Some(else_b) = else_b {
            self.set_block(&else_block);
            let else_val = self.expression(&**else_b)?;
            else_block = self.cur_block_name();

            if else_val.get_type() == body_type {
                self.set_block(&cont_block);

                let load = Expr::load(&body_alloca);
                self.cur_loop()
                    .phi_nodes
                    .push((load, Rc::clone(&loop_end_block)));
                self.cur_loop()
                    .phi_nodes
                    .push((else_val, Rc::clone(&else_block)));

                ret = Expr::phi(self.current_loop.take().unwrap().phi_nodes)
            }
        }

        self.set_block(&else_block);
        self.insert_at_ptr(Expr::jump(&cont_block));
        self.set_block(&cont_block);
        self.current_loop = prev_loop;

        Ok(ret)
    }

    fn get(&mut self, object: &ASTExpr, name: &Token) -> Res<Expr> {
        let (object, field) = self.get_field(object, name)?;
        let field = field.left().or_err(
            &self.builder.path,
            name,
            "Cannot get class method (must be called)",
        )?;

        if self.uninitialized_this_members.contains(&field) {
            return Err(self.err(name, "Cannot get uninitialized class member."));
        }
        Ok(Expr::struct_get(object, &field))
    }

    fn if_(
        &mut self,
        condition: &ASTExpr,
        then_branch: &ASTExpr,
        else_branch: &Option<Box<ASTExpr>>,
    ) -> Res<Expr> {
        let cond = self.expression(condition)?;
        if cond.get_type() != Type::Bool {
            return Err(self.err(condition.get_token(), "If condition must be a boolean"));
        }

        let mut then_block = self.append_block("then");
        let mut else_block = self.append_block("else");
        let cont_block = self.append_block("cont");

        self.insert_at_ptr(Expr::branch(cond, &then_block, &else_block));

        self.set_block(&then_block);
        let then_val = self.expression(then_branch)?;
        then_block = self.cur_block_name();

        self.set_block(&else_block);
        if let Some(else_branch) = else_branch {
            let else_val = self.expression(&**else_branch)?;
            else_block = self.cur_block_name();

            if then_val.get_type() == else_val.get_type() {
                self.insert_at_ptr(Expr::jump(&cont_block));
                self.set_block(&then_block);
                self.insert_at_ptr(Expr::jump(&cont_block));

                self.set_block(&cont_block);
                return Ok(Expr::phi(vec![
                    (then_val, then_block),
                    (else_val, else_block),
                ]));
            } else {
                self.insert_at_ptr(else_val);
                self.insert_at_ptr(Expr::jump(&cont_block));
            }
        } else {
            self.set_block(&else_block);
            self.insert_at_ptr(Expr::jump(&cont_block));
        }

        self.set_block(&then_block);
        self.insert_at_ptr(then_val);
        self.insert_at_ptr(Expr::jump(&cont_block));

        self.set_block(&cont_block);
        Ok(Expr::none_const())
    }

    fn literal(&mut self, literal: &Literal) -> Res<Expr> {
        if let Literal::Array(arr) = literal {
            let ast_values = arr.as_ref().left().unwrap();
            let mut values_mir = Vec::new();
            let mut ast_values = ast_values.iter();
            let first = self.expression(ast_values.next().unwrap())?;
            let arr_type = first.get_type();

            values_mir.push(first);
            for value in ast_values {
                let mir_val = self.expression(value)?;

                if mir_val.get_type() != arr_type {
                    return Err(self.err(
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

            Ok(Expr::Literal(Literal::Array(Either::Right(ArrayLiteral {
                values: values_mir,
                type_: arr_type,
            }))))
        } else {
            Ok(Expr::Literal(literal.clone()))
        }
    }

    fn return_(&mut self, val: &Option<Box<ASTExpr>>, err_tok: &Token) -> Res<Expr> {
        let value = val
            .as_ref()
            .map(|v| self.expression(&*v))
            .transpose()?
            .unwrap_or_else(Expr::none_const);

        if value.get_type() != self.cur_fn().borrow().ret_type.clone() {
            return Err(self.err(err_tok, "Return expression in function has wrong type"));
        }

        self.insert_at_ptr(Expr::ret(value));
        Ok(Expr::any_const())
    }

    fn set(&mut self, object: &ASTExpr, name: &Token, value: &ASTExpr) -> Res<Expr> {
        let (object, field) = self.get_field(object, name)?;
        let field = field
            .left()
            .or_err(&self.builder.path, name, "Cannot set class method")?;
        let value = self.expression(value)?;

        if value.get_type() != field.type_ {
            return Err(self.err(name, "Class member is a different type"));
        }
        if !field.mutable && !self.uninitialized_this_members.contains(&field) {
            return Err(self.err(name, "Cannot set immutable class member"));
        }

        self.uninitialized_this_members.remove(&field);
        Ok(Expr::struct_set(object, field, value))
    }

    fn unary(&mut self, operator: &Token, right: &ASTExpr) -> Res<Expr> {
        let right = self.expression(right)?;

        match operator.t_type {
            TType::Bang if right.get_type() != Type::Bool => {
                Err(self.err(operator, "'!' can only be used on boolean values"))
            }

            _ => Ok(()),
        }?;

        Ok(Expr::unary(right, operator.t_type))
    }

    fn var(&mut self, var: &Token) -> Res<Expr> {
        Ok(Expr::load(&self.find_var(&var)?))
    }

    fn var_with_generics(&mut self, name: &Token, generics: &[ASTType]) -> Res<Expr> {
        // All valid cases of this are function prototypes.
        // Class prototypes can only be called and not assigned;
        // which would be handled in the ASTExpr::Call branch.
        let prototype = self.module.borrow().find_prototype(&name.lexeme).or_err(
            &self.builder.path,
            &name,
            "Unknown prototype.",
        )?;
        let types = generics
            .iter()
            .map(|ty| self.builder.find_type(ty))
            .collect::<Result<Vec<Type>, Error>>()?;

        let function = prototype.build(types, name)?;
        if let Type::Function(func) = function {
            Ok(Expr::load(
                &self
                    .module
                    .borrow()
                    .find_global(&func.borrow().name)
                    .unwrap(),
            ))
        } else {
            Err(self.err(&name, "Can only instantiate function prototypes here"))
        }
    }

    fn when(
        &mut self,
        value: &ASTExpr,
        branches: &[(ASTExpr, ASTExpr)],
        else_branch: &ASTExpr,
    ) -> Res<Expr> {
        let start_b = self.cur_block_name();

        let value = self.expression(value)?;
        let val_type = value.get_type();

        let else_b = self.append_block("when-else");
        let cont_b = self.append_block("when-cont");

        self.set_block(&else_b);
        let else_val = self.expression(else_branch)?;
        let branch_type = else_val.get_type();
        self.insert_at_ptr(Expr::jump(&cont_b));

        let mut cases = Vec::with_capacity(branches.len());
        let mut phi_nodes = Vec::with_capacity(branches.len());
        for (b_val, branch) in branches.iter() {
            self.set_block(&start_b);
            let val = self.expression(b_val)?;
            if val.get_type() != val_type {
                return Err(self.err(
                    b_val.get_token(),
                    "Branches of when must be of same type as the value compared.",
                ));
            }

            // Small hack to get a token that gives the user
            // a useful error without having to add complexity
            // to binary_mir()
            let mut optok = b_val.get_token().clone();
            optok.t_type = TType::EqualEqual;
            let val = self.binary_mir(val, &optok, value.clone())?;

            let branch_b = self.append_block("when-br");
            self.set_block(&branch_b);
            let branch_val = self.expression(branch)?;
            if branch_val.get_type() != branch_type {
                return Err(self.err(branch.get_token(), "Branch results must be of same type."));
            }
            self.insert_at_ptr(Expr::jump(&cont_b));

            let branch_b = self.cur_block_name();
            cases.push((val, Rc::clone(&branch_b)));
            phi_nodes.push((branch_val, branch_b))
        }

        phi_nodes.push((else_val, Rc::clone(&else_b)));

        self.set_block(&start_b);
        self.insert_at_ptr(Expr::Flow(Box::new(Flow::Switch {
            cases,
            default: else_b,
        })));

        self.set_block(&cont_b);
        Ok(Expr::phi(phi_nodes))
    }

    fn var_def(&mut self, var: &ASTVar) -> Res<Expr> {
        let init = self.expression(&var.initializer)?;
        let _type = init.get_type();
        let var = self.define_variable(&var.name, var.mutable, _type);
        Ok(Expr::store(&var, init))
    }
}
