/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 2/3/20 1:50 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::{
    ast,
    ast::{
        declaration::{FuncSignature, FunctionParam, Variable as ASTVar, Visibility},
        literal::Closure,
        Expression as ASTExpr, Literal, Type as ASTType,
    },
    error::Res,
    lexer::token::{TType, Token},
    mir::{
        generator::{
            passes::declaring_globals::{generate_mir_fn, insert_global_and_type},
            ForLoop, MIRGenerator,
        },
        nodes::{catch_up_passes, Class, Expr, Type, Variable},
        result::ToMIRResult,
        MutRc,
    },
};
use either::Either::{Left, Right};

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

            ASTExpr::IndexGet {
                indexed,
                index,
                bracket,
            } => self.index_get(indexed, index, bracket),

            ASTExpr::IndexSet {
                indexed,
                index,
                value,
            } => self.index_set(indexed, index, value),

            ASTExpr::Literal(literal, token) => self.literal(literal, token),

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

        if val_ty == var.type_ && var.mutable {
            Ok(Expr::store(&var, value, false))
        } else if !var.mutable {
            Err(self.err(
                &name,
                &format!("Variable {} is a different type", name.lexeme),
            ))
        } else {
            Err(self.err(
                &name,
                &format!("Variable {} is not assignable (val)", name.lexeme),
            ))
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
        let exprs = expressions.iter().map(|e| self.expression(e)).collect::<Res<_>>()?;
        self.end_scope();

        Ok(Expr::Block(exprs))
    }

    fn break_(&mut self, expr: &Option<Box<ASTExpr>>, err_tok: &Token) -> Res<Expr> {
        if self.current_loop.is_none() {
            return Err(self.err(err_tok, "Break is only allowed in loops."));
        }

        let expr = expr.as_ref().map(|expr| {
            let expression = self.expression(&expr)?;
            self.get_or_create_loop_var(&expression.get_type())?;
            Ok(expression)
        }).transpose()?;

        Ok(Expr::break_(expr))
    }

    fn call(&mut self, callee: &ASTExpr, arguments: &[ASTExpr]) -> Res<Expr> {
        if let Some(expr) = self.try_method_or_constructor(callee, arguments)? {
            return Ok(expr);
        }

        // method above fell through, its either a function/closure call or invalid
        let callee_mir = self.expression(callee)?;
        if let Type::Function(func) = callee_mir.get_type() {
            let args = self.generate_func_args(
                func.borrow().parameters.iter().map(|p| &p.type_),
                arguments,
                None,
                func.borrow()
                    .ast
                    .as_ref()
                    .map(|a| a.sig.variadic)
                    .unwrap_or(false),
                callee.get_token(),
            )?;
            Ok(Expr::call(callee_mir, args))
        } else if let Type::Closure(closure) = callee_mir.get_type() {
            let args = self.generate_func_args(
                closure.parameters.iter(),
                arguments,
                None,
                false,
                callee.get_token(),
            )?;
            Ok(Expr::call(callee_mir, args))
        } else {
            Err(self.err(
                callee.get_token(),
                "Only functions are allowed to be called",
            ))
        }
    }

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

                match func {
                    Left(func) => {
                        let args = self.generate_func_args(
                            func.type_
                                .as_function()
                                .borrow()
                                .parameters
                                .iter()
                                .map(|p| &p.type_),
                            arguments,
                            Some(object),
                            false,
                            name,
                        )?;

                        Ok(Some(Expr::call(Expr::load(&func), args)))
                    }

                    Right(index) => {
                        let ty = object.get_type();
                        let iface = ty.as_interface().borrow();
                        let params = &iface.methods.get_index(index).unwrap().1.parameters;
                        let args = self.generate_func_args(
                            // 'params' need to have the 'this' parameter, this is the result...
                            Some(ty.clone()).iter().chain(params.iter()),
                            arguments,
                            Some(object),
                            false,
                            name,
                        )?;

                        Ok(Some(Expr::call_dyn(ty.as_interface(), index, args)))
                    }
                }
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
                let proto = self.module.borrow().find_prototype(&name.lexeme);

                if proto.is_some() && proto.unwrap().ast.is_class() {
                    let ty = self.builder.find_type(&ASTType::Generic {
                        token: name.clone(),
                        types: generics.clone(),
                    })?;
                    Ok(Some(self.generate_class_instantiation(
                        Rc::clone(ty.as_class()),
                        arguments,
                        name,
                    )?))
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
        let args = args
            .iter()
            .map(|arg| self.expression(arg))
            .collect::<Res<Vec<Expr>>>()?;

        let cls = class.borrow();
        let constructor: &Rc<Variable> = cls
            .constructors
            .iter()
            .find(|constructor| {
                let constructor = constructor.type_.as_function().borrow();
                if constructor.parameters.len() - 1 != args.len() {
                    return false;
                }
                for (param, arg) in constructor.parameters.iter().skip(1).zip(args.iter()) {
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

        Ok(Expr::alloc_class(&class, constructor, args))
    }

    fn for_(
        &mut self,
        condition: &ASTExpr,
        body: &ASTExpr,
        else_b: &Option<Box<ASTExpr>>,
    ) -> Res<Expr> {
        let prev_loop = std::mem::replace(&mut self.current_loop, Some(ForLoop::default()));

        let cond = self.expression(condition)?;
        if cond.get_type() != Type::Bool {
            return Err(self.err(condition.get_token(), "For condition must be a boolean."));
        }

        let body = self.expression(body)?;
        let body_type = body.get_type();
        let body_alloca = self.get_or_create_loop_var(&body_type)?;

        let (else_, result_store) = if let Some(else_b) = else_b {
            let else_val = self.expression(&else_b)?;
            if else_val.get_type() == body_type {
                (Some(else_val), Some(self.get_or_create_loop_var(&body_type)?))
            } else {
                (Some(else_val), None)
            }
        } else {
            (None, None)
        };

        let loop_data = std::mem::replace(&mut self.current_loop, prev_loop).unwrap();
        Ok(Expr::loop_(cond, body, else_, loop_data.variables, result_store))
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
        let mut phi = false;
        let cond = self.expression(condition)?;
        if cond.get_type() != Type::Bool {
            return Err(self.err(condition.get_token(), "If condition must be a boolean"));
        }

        let then_val = self.expression(then_branch)?;
        let else_val = else_branch.as_ref()
            .map(|else_branch| self.expression(&else_branch))
            .unwrap_or(Ok(Expr::none_const()))?;
        let then_ty = then_val.get_type();
        let else_ty = else_val.get_type();
        let phi =
            then_ty == else_val.get_type() && (then_ty != Type::None || else_ty != Type::None);

        Ok(Expr::if_(cond, then_val, else_val, phi))
    }

    fn index_get(&mut self, indexed: &ASTExpr, index: &ASTExpr, bracket: &Token) -> Res<Expr> {
        let obj = self.expression(indexed)?;
        let index = self.expression(index)?;
        self.binary_mir(obj, bracket, index)
    }

    fn index_set(
        &mut self,
        indexed: &ASTExpr,
        ast_index: &ASTExpr,
        ast_value: &ASTExpr,
    ) -> Res<Expr> {
        let obj = self.expression(indexed)?;
        let index = self.expression(ast_index)?;
        let value = self.expression(ast_value)?;
        let method = self
            .get_operator_overloading_method(
                TType::RightBracket,
                &obj.get_type(),
                &index.get_type(),
            )
            .or_err(
                &self.builder.path,
                ast_index.get_token(),
                "No implementation of operator found for types.",
            )?;

        if value.get_type() != method.type_.as_function().borrow().parameters[2].type_ {
            Err(self.err(ast_value.get_token(), "Setter is of wrong type."))
        } else {
            Ok(Expr::call(Expr::load(&method), vec![obj, index, value]))
        }
    }

    fn literal(&mut self, literal: &Literal, token: &Token) -> Res<Expr> {
        match literal {
            Literal::Array(arr) => self.array_literal(arr.as_ref().left().unwrap()),
            Literal::Closure(closure) => self.closure(closure, token),
            _ => Ok(Expr::Literal(literal.clone())),
        }
    }

    fn array_literal(&mut self, literal: &[ASTExpr]) -> Res<Expr> {
        let mut values_mir = Vec::new();
        let mut ast_values = literal.iter();
        let first = self.expression(ast_values.next().unwrap())?;
        let elem_type = first.get_type();

        values_mir.push(first);
        for value in ast_values {
            let mir_val = self.expression(value)?;

            if mir_val.get_type() != elem_type {
                return Err(self.err(
                    value.get_token(),
                    &format!(
                        "Type of array value ({}) does not match rest of array ({}).",
                        mir_val.get_type(),
                        elem_type
                    ),
                ));
            }

            values_mir.push(mir_val);
        }

        let arr_proto = self
            .module
            .borrow()
            .find_prototype(&"Array".to_string())
            .unwrap();
        let array_type: MutRc<Class> = Rc::clone(
            arr_proto
                .build(
                    vec![elem_type],
                    &Token::generic_token(TType::RightBracket),
                    Rc::clone(&arr_proto),
                )?
                .as_class(),
        );

        let dummy_tok = Token::generic_token(TType::Var);
        let push_method = {
            let arr = array_type.borrow();
            Rc::clone(arr.methods.get(&Rc::new("push".to_string())).unwrap())
        };

        let array = self.generate_class_instantiation(
            array_type,
            &[ASTExpr::Literal(
                Literal::I64(values_mir.len() as u64),
                dummy_tok.clone(),
            )],
            &dummy_tok,
        )?;

        for value in values_mir {
            self.insert_at_ptr(Expr::call(
                Expr::load(&push_method),
                vec![array.clone(), value],
            ))
        }

        Ok(array)
    }

    fn closure(&mut self, closure: &Closure, token: &Token) -> Res<Expr> {
        let mut name = token.clone();
        name.lexeme = Rc::new(format!("closure-{}:{}", token.line, token.index));
        let ast_func = ast::Function {
            sig: FuncSignature {
                name: name.clone(),
                visibility: Visibility::Public,
                generics: None,
                return_type: closure.ret_ty.clone(),
                parameters: closure
                    .parameters
                    .iter()
                    .map(|p| FunctionParam {
                        type_: p.type_.as_ref().unwrap().clone(),
                        name: p.name.clone(),
                    })
                    .collect(),
                variadic: false,
            },
            body: Some(closure.body.clone()),
        };

        let mut gen = Self::for_closure(self);
        let function = generate_mir_fn(
            &gen.builder,
            Right(ast_func),
            String::clone(&name.lexeme),
            Some(FunctionParam::this_param(&Token::generic_identifier(
                "i64".to_string(),
            ))),
        )?;
        let global = Variable::new(false, Type::Function(Rc::clone(&function)), &name.lexeme);
        insert_global_and_type(&gen.module, &global);

        catch_up_passes(&mut gen, &Type::Function(Rc::clone(&function)))?;
        let closure_data = gen.end_closure(self);

        let captured = Rc::new(closure_data.captured);
        function.borrow_mut().parameters[0] = Variable::new(
            false,
            Type::ClosureCaptured(Rc::clone(&captured)),
            &Rc::new("CLOSURE-CAPTURED".to_string()),
        );

        let expr = Expr::construct_closure(&global, captured);
        let var = self.define_variable(
            &Token::generic_identifier("closure-literal".to_string()),
            false,
            expr.get_type(),
        );
        if let Some(loopdat) = &mut self.current_loop {
            loopdat.variables.push(Rc::clone(&var));
            var.as_local.set(false);
        }
        Ok(Expr::store(&var, expr, true))
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

        Ok(Expr::ret(value))
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

        let first_set = self.uninitialized_this_members.remove(&field);
        Ok(Expr::struct_set(object, field.index, value, first_set))
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
        let function = self.builder.find_type(&ASTType::Generic {
            token: name.clone(),
            types: Vec::from(generics),
        })?;

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
        let value = self.expression(value)?;
        let cond_type = value.get_type();

        let else_val = self.expression(else_branch)?;
        let branch_type = else_val.get_type();

        let mut cases = Vec::with_capacity(branches.len());
        for (br_cond_ast, branch) in branches.iter() {
            let br_cond = self.expression(br_cond_ast)?;
            if br_cond.get_type() != cond_type {
                return Err(self.err(
                    br_cond_ast.get_token(),
                    "Branches of when must be of same type as the value compared.",
                ));
            }

            // Small hack to get a token that gives the user
            // a useful error without having to add complexity
            // to binary_mir()
            let mut optok = br_cond_ast.get_token().clone();
            optok.t_type = TType::EqualEqual;
            let cond = self.binary_mir(br_cond, &optok, value.clone())?;

            let branch_val = self.expression(branch)?;
            if branch_val.get_type() != branch_type {
                return Err(self.err(branch.get_token(), "Branch results must be of same type."));
            }

            cases.push((cond, branch_val))
        }

        Ok(Expr::when(cases, Some(else_val), Some(branch_type)))
    }

    fn var_def(&mut self, var: &ASTVar) -> Res<Expr> {
        let init = self.expression(&var.initializer)?;
        let _type = init.get_type();
        let var = self.define_variable(&var.name, var.mutable, _type);
        if let Some(loopdat) = &mut self.current_loop {
            loopdat.variables.push(Rc::clone(&var));
            var.as_local.set(false);
        }
        Ok(Expr::store(&var, init, true))
    }
}
