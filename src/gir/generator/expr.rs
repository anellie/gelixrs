use smol_str::SmolStr;

use crate::{
    ast,
    ast::{
        declaration::{FuncSignature, FunctionParam, Visibility},
        literal::Closure,
        Expression as AExpr, Literal,
    },
    error::Res,
    gir::{
        generator::GIRGenerator,
        nodes::{
            declaration::{ADTType, LocalVariable, Variable, ADT},
            expression::{CastType, Expr},
            types::{
                Instance, Type, TypeArguments, TypeParameter, TypeParameters, TypeVariable,
                VariableModifier,
            },
        },
        result::EmitGIRError,
        MutRc,
    },
    lexer::token::{TType, Token},
};
use std::rc::Rc;

/// This impl contains all code of the generator that directly
/// produces expressions.
/// This is split into its own file for readability reasons;
/// a 1500-line file containing everything is difficult to navigate.
impl GIRGenerator {
    pub fn expression(&mut self, expression: &AExpr) -> Expr {
        let expr = match expression {
            AExpr::Assignment { name, value } => self.assignment(name, value),

            AExpr::Binary {
                left,
                operator,
                right,
            } => self.binary(left, operator, right),

            AExpr::Block(expressions, _) => self.block(expressions),

            AExpr::Break(expr, tok) => self.break_(expr, tok),

            AExpr::Call { callee, arguments } => self.call(callee, arguments),

            AExpr::ForCond {
                condition,
                body,
                else_b,
            } => self.for_cond(condition, body, else_b),

            AExpr::ForIter {
                elem_name,
                iterator,
                body,
                else_b,
            } => self.for_iter(elem_name, iterator, body, else_b),

            AExpr::Get {
                object,
                name,
                type_args,
            } => {
                if !type_args.is_empty() {
                    Err(self.err_(name, "Can only call generic methods directly".to_string()))
                } else {
                    self.get(object, name)
                }
            }

            AExpr::GetStatic { object, name } => self.get_static(object, name, true),

            AExpr::If {
                condition,
                then_branch,
                else_branch,
            } => self.if_(condition, then_branch, else_branch),

            AExpr::IndexGet {
                indexed,
                index,
                bracket,
            } => self.index_get(indexed, index, bracket),

            AExpr::IndexSet {
                indexed,
                index,
                value,
            } => self.index_set(indexed, index, value),

            AExpr::Literal(literal, token) => self.literal(literal, token),

            AExpr::Return(val, err_tok) => self.return_(val, err_tok),

            AExpr::Set {
                object,
                name,
                value,
            } => self.set(object, name, value),

            AExpr::Unary { operator, right } => self.unary(operator, right),

            AExpr::Variable(var) => self.var(var),

            AExpr::VarWithGenerics { name, generics } => self.var_with_generics(name, generics),

            AExpr::When {
                value,
                branches,
                else_branch,
            } => self.when(value, branches, else_branch),

            AExpr::VarDef(var) => self.var_def(var),
        };

        self.eat(expr)
            .unwrap_or_else(|| Expr::literal(Literal::Any))
    }

    fn assignment(&mut self, name: &Token, value: &AExpr) -> Res<Expr> {
        let var = self.find_local_var(&name)?;
        let value = self.expression(value);
        let (value, matching_types) = self.resolver.try_cast(value, &var.ty);

        if matching_types && var.mutable {
            Ok(Expr::store(Expr::lvar(&var), value, false))
        } else if var.mutable {
            Err(self.err_(
                &name,
                format!("Variable {} is a different type", name.lexeme),
            ))
        } else {
            Err(self.err_(
                &name,
                format!("Variable {} is not assignable (val)", name.lexeme),
            ))
        }
    }

    fn binary(&mut self, left: &AExpr, operator: &Token, right: &AExpr) -> Res<Expr> {
        let left = self.expression(left);

        // Account for an edge case with simple enums, where `A:A` incorrectly gets
        // turned into a regular value instead of a type get.
        let right = match right {
            AExpr::GetStatic { object, name } if operator.t_type == TType::Is => {
                self.get_static(object, name, false)?
            }
            _ => self.expression(right),
        };

        self.binary_gir(left, operator, right)
    }

    fn binary_gir(&mut self, mut left: Expr, operator: &Token, mut right: Expr) -> Res<Expr> {
        let left_ty = left.get_type();
        let right_ty = right.get_type();

        if (left_ty == right_ty && left_ty.is_number())
            || (left_ty.is_int() && right_ty.is_int())
            || left_ty.is_float() && right_ty.is_float()
            || (operator.t_type == TType::Is && right_ty.is_type())
        {
            if operator.t_type == TType::And || operator.t_type == TType::Or {
                Ok(Self::binary_logic(left, operator.t_type, right))
            } else {
                let (_, left, right) = self.resolver.try_unify_type(left, right);
                Ok(Expr::binary(operator.clone(), left, right))
            }
        } else {
            let method_var = self
                .get_operator_overloading_method(operator.t_type, &mut left, &mut right)
                .on_err(
                    &self.path,
                    operator,
                    "No implementation of operator found for types.",
                )?;

            let mut expr = Expr::call(Expr::var(Variable::Function(method_var)), vec![left, right]);
            if operator.t_type == TType::BangEqual {
                // Invert the result if this is `!=`, as the method is from `Eq`
                expr = Expr::unary(Token::generic_token(TType::Bang), expr);
            }
            Ok(expr)
        }
    }

    /// Logic operators need special treatment for shortcircuiting behavior
    fn binary_logic(left: Expr, operator: TType, right: Expr) -> Expr {
        if operator == TType::And {
            // a and b --> if (a) b else false
            Expr::if_(
                left,
                right,
                Expr::literal(Literal::Bool(false)),
                Some(Type::Bool),
            )
        } else {
            // a or b --> if (a) true else b
            Expr::if_(
                left,
                Expr::literal(Literal::Bool(true)),
                right,
                Some(Type::Bool),
            )
        }
    }

    fn block(&mut self, expressions: &[AExpr]) -> Res<Expr> {
        if expressions.is_empty() {
            return Ok(Expr::none_const_());
        }

        self.begin_scope();
        let exprs = expressions.iter().map(|e| self.expression(e)).collect();
        self.end_scope();

        Ok(Expr::Block(exprs))
    }

    fn break_(&mut self, expr: &Option<Box<AExpr>>, err_tok: &Token) -> Res<Expr> {
        if self.current_loop_ty.is_none() {
            self.err(err_tok, "Break is only allowed in loops.".to_string());
        }

        let expr = expr
            .as_ref()
            .map(|expr| {
                let expression = self.expression(&expr);
                self.set_loop_type(&expression.get_type());
                expression
            })
            .unwrap_or_else(|| Expr::none_const(err_tok.clone()));

        Ok(Expr::break_(expr))
    }

    fn call(&mut self, ast_callee: &AExpr, arguments: &[AExpr]) -> Res<Expr> {
        let mut args = arguments
            .iter()
            .map(|a| self.expression(a))
            .collect::<Vec<_>>();

        match ast_callee {
            // Method call while a `this` member is still uninitialized
            AExpr::Get { name, .. } if !self.uninitialized_this_fields.is_empty() => Err(self
                .err_(
                    name,
                    "Cannot call methods in constructors until all class members are initialized."
                        .to_string(),
                )),

            // Method call
            AExpr::Get {
                object,
                name,
                type_args,
            } => {
                let (object, field) = self.get_field(object, name)?;
                let func = field
                    .right()
                    .on_err(&self.path, name, "Fields cannot be called.")?;

                let obj_ty = object.get_type();
                let parent_ty_args = obj_ty.type_args().unwrap();
                args.insert(0, object);

                let ty_args = if !type_args.is_empty() {
                    type_args
                        .iter()
                        .map(|t| self.resolver.find_type(t))
                        .collect::<Res<Vec<_>>>()?
                } else {
                    self.maybe_infer_ty_args(
                        &func.borrow().parameters,
                        &func.borrow().type_parameters,
                        &args,
                        parent_ty_args.len(),
                        name,
                    )?
                };
                let ty_args = parent_ty_args
                    .iter()
                    .cloned()
                    .chain(ty_args.into_iter())
                    .collect::<Vec<_>>();
                let func = Instance::new(Rc::clone(&func), Rc::new(ty_args));

                self.check_func_args_(
                    &Type::Function(func.clone()),
                    &mut args,
                    arguments,
                    name,
                    true,
                )?;
                Ok(Expr::call(Expr::var(Variable::Function(func)), args))
            }

            // Can be either a constructor or function call
            _ => {
                let mut callee = self.expression(ast_callee);
                let mut callee_type = callee.get_type();

                if let Some(constructors) = callee_type.get_constructors() {
                    let mut ty_vars = Rc::clone(callee_type.type_args().unwrap());
                    let constructor = Rc::clone(
                        constructors
                            .iter()
                            .try_find(|constructor| {
                                let constructor = constructor.borrow();

                                // Different args count
                                if !(constructor.parameters.len() - 1 == args.len()) {
                                    return Ok(false);
                                }

                                // If there's no type args yet try inferring them
                                if ty_vars.is_empty() {
                                    args.insert(0, Expr::none_const_());
                                    ty_vars = Rc::new(self.maybe_infer_ty_args(
                                        &constructor.parameters,
                                        &constructor.type_parameters,
                                        &args,
                                        0,
                                        &constructor.name,
                                    )?);
                                    args.remove(0);
                                };

                                // Now check if the args are the correct type
                                let correct_args_types =
                                    constructor.parameters.iter().skip(1).zip(args.iter()).all(
                                        |(param, arg)| {
                                            let ty1 = arg.get_type();
                                            let ty2 = param.ty.resolve(&ty_vars);
                                            ty1 == ty2 || ty1.can_cast_to(&ty2).is_some()
                                        },
                                    );

                                Ok(correct_args_types)
                            })?
                            .on_err(
                                &self.path,
                                ast_callee.get_token(),
                                "No matching constructor found for arguments.",
                            )?,
                    );

                    { // Cast/convert all arguments to fit
                        for (param, arg) in constructor
                            .borrow()
                            .parameters
                            .iter()
                            .skip(1)
                            .zip(args.iter_mut())
                        {
                            self.resolver
                                .try_cast_in_place(arg, &param.ty.resolve(&ty_vars));
                        }
                    }

                    callee_type.set_type_args(ty_vars);
                    Ok(Expr::Allocate {
                        ty: callee_type.into_type().to_weak(),
                        constructor,
                        args,
                        tok: ast_callee.get_token().clone(),
                    })
                } else {
                    // If this is a function call, check it has
                    // its type arguments inferred should it have any
                    if let Expr::Variable(Variable::Function(func)) = &mut callee {
                        let ty_args = if !func.args().is_empty() {
                            Rc::clone(func.args())
                        } else {
                            Rc::new(self.maybe_infer_ty_args(
                                &func.ty.borrow().parameters,
                                &func.ty.borrow().type_parameters,
                                &args,
                                0,
                                &func.ty.borrow().name,
                            )?)
                        };
                        func.set_args(ty_args)
                    }

                    self.check_func_args_(
                        &callee.get_type(), // Get it again to ensure inferred type args are present
                        &mut args,
                        arguments,
                        ast_callee.get_token(),
                        false,
                    )?;
                    Ok(Expr::call(callee, args))
                }
            }
        }
    }

    /// Check a function call's arguments for correctness,
    /// possibly adding a cast if required.
    fn check_func_args<T: Iterator<Item = Type>>(
        &mut self,
        mut parameters: T,
        type_args: Option<&Rc<TypeArguments>>,
        args: &mut Vec<Expr>,
        ast_args: &[AExpr],
        allow_variadic: bool,
        err_tok: &Token,
        is_method: bool,
    ) -> Res<()> {
        let para_len = parameters.size_hint().0;
        if para_len > args.len() || (para_len < args.len() && !allow_variadic) {
            return Err(self.err_(
                err_tok,
                format!(
                    "Incorrect amount of function arguments. (Expected {}; got {})",
                    parameters.size_hint().0,
                    args.len()
                ),
            ));
        }

        // Sometimes, methods need their "this" arg to be cast (enum case calling a parent func for example)
        // Since parameters are an iterator it needs to be done now
        if is_method {
            // Remove the "this" argument to get ownership using swap_remove, last arg is now
            // swapped into index 0
            // Try casting it, if the cast fails then the method is being called with
            // a WR or DV despite the 'strong' modifier so throw an error
            let arg = self
                .resolver
                .cast_or_none(
                    args.swap_remove(0),
                    &parameters.next().unwrap().resolve(type_args.unwrap()),
                )
                .on_err(
                    &self.path,
                    err_tok,
                    "This method requires a strong reference",
                )?;
            // Put "this" arg at the end and swap them again
            let this_index = args.len();
            args.push(arg);
            args.swap(0, this_index);
            // (This is done since it does not need any copying)
        }

        for ((argument, parameter), ast) in args
            .iter_mut()
            .skip(is_method as usize)
            .zip(parameters)
            .zip(ast_args.iter())
        {
            let arg_type = argument.get_type();
            let success = self.resolver.try_cast_in_place(argument, &parameter);
            if !success {
                self.err(
                    ast.get_token(),
                    format!(
                        "Call argument is the wrong type (was {}, expected {})",
                        arg_type, parameter
                    ),
                )
            }
        }

        Ok(())
    }

    /// Same as above, but takes a function instead.
    /// `func` should be `Type::Function` or `Type::Closure`, otherwise returns an error.
    pub fn check_func_args_(
        &mut self,
        func: &Type,
        args: &mut Vec<Expr>,
        ast_args: &[AExpr],
        err_tok: &Token,
        is_method: bool,
    ) -> Res<()> {
        match func {
            Type::Function(func) => self.check_func_args(
                func.ty
                    .borrow()
                    .parameters
                    .iter()
                    .map(|p| p.ty.resolve(func.args())),
                Some(func.args()),
                args,
                ast_args,
                func.ty.borrow().ast.borrow().sig.variadic,
                err_tok,
                is_method,
            ),

            Type::Closure(closure) => self.check_func_args(
                closure.parameters.iter().cloned(),
                None,
                args,
                ast_args,
                false,
                err_tok,
                is_method,
            ),

            _ => Err(self.err_(err_tok, "This cannot be called.".to_string())),
        }
    }

    /// Try inferring a set of type arguments from a call.
    /// TODO: This should be optimized to not allocate
    /// an Rc for every function call...
    pub fn maybe_infer_ty_args(
        &mut self,
        parameters: &[Rc<LocalVariable>],
        type_params: &[TypeParameter],
        arguments: &[Expr],
        skip: usize,
        err_tok: &Token,
    ) -> Res<TypeArguments> {
        if type_params.is_empty() {
            return Ok(vec![]);
        }

        let arg_tys = arguments.iter().map(|a| a.get_type()).collect::<Vec<_>>();
        let search_res = type_params
            .iter()
            .skip(skip)
            .map(|param| {
                self.resolve_type_param(param, parameters.iter().map(|p| &p.ty), arg_tys.iter())
            })
            .collect::<Option<Vec<_>>>();

        search_res.on_err(
            &self.path,
            err_tok,
            "Cannot infer types (please specify explicitly).",
        )
    }

    fn resolve_type_param<'a, T1: Iterator<Item = &'a Type>, T2: Iterator<Item = &'a Type>>(
        &self,
        ty_param: &TypeParameter,
        call_params: T1,
        arguments: T2,
    ) -> Option<Type> {
        call_params
            .zip(arguments)
            .find_map(|(param, arg)| self.match_param(param, arg, &ty_param))
    }

    fn match_param(&self, param: &Type, arg: &Type, ty_param: &TypeParameter) -> Option<Type> {
        match (param, arg) {
            (Type::Variable(var), _) if var.name == ty_param.name.lexeme => match var.modifier {
                VariableModifier::Value => Some(arg.clone()),
                VariableModifier::Weak => Some(arg.to_value()),
                VariableModifier::Strong => Some(arg.to_value()),
            },

            (Type::RawPtr(param_inner), Type::RawPtr(arg_inner)) => {
                self.match_param(param_inner, arg_inner, ty_param)
            }

            _ => None,
        }
    }

    fn for_cond(
        &mut self,
        condition: &AExpr,
        body: &AExpr,
        else_b: &Option<Box<AExpr>>,
    ) -> Res<Expr> {
        let cond = self.expression(condition);
        if cond.get_type() != Type::Bool {
            self.err(
                condition.get_token(),
                "For condition must be a boolean.".to_string(),
            );
        }

        self.begin_scope();
        let mut cast_block = self.smart_casts(&cond);
        let (body, else_, phi_ty) = self.for_body(body, else_b);
        self.end_scope();
        cast_block.push(body);
        Ok(Expr::loop_(cond, Expr::Block(cast_block), else_, phi_ty))
    }

    /// Return type, in order:
    /// - for body
    /// - else branch
    /// - phi type
    fn for_body(
        &mut self,
        body: &AExpr,
        else_b: &Option<Box<AExpr>>,
    ) -> (Expr, Expr, Option<Type>) {
        let prev_loop_ty = std::mem::replace(&mut self.current_loop_ty, Some(Type::Any));

        let body = self.expression(body);
        let body_type = body.get_type();
        self.set_loop_type(&body_type);

        let else_val = else_b.as_ref().map_or(Expr::none_const_(), |else_branch| {
            self.expression(&else_branch)
        });
        let (phi_ty, body, else_) = self.resolver.try_unify_type(body, else_val);

        self.current_loop_ty = prev_loop_ty;
        (body, else_, phi_ty)
    }

    fn for_iter(
        &mut self,
        _elem_name: &Token,
        _iterator: &AExpr,
        _body: &AExpr,
        _else_b: &Option<Box<AExpr>>,
    ) -> Res<Expr> {
        todo!();
        /*
        self.begin_scope();

        let iter_gir = self.expression(iterator)?;
        let (iter_ty, elem_ty, to_iter_ty) = INTRINSICS
            .with(|i| i.borrow().get_for_iter_type(&iter_gir.get_type()))
            .or_err(
                &self.builder.path,
                iterator.get_token(),
                "Not an iterator (must implement Iter or ToIter).",
            )?;

        let iter = if let Some(to_iter_ty) = to_iter_ty {
            let method = Self::find_associated_method(
                &to_iter_ty,
                &Token::generic_identifier("iter".to_string()),
            )
            .unwrap()
            .into_fn();
            Expr::call(Expr::load(&method), vec![iter_gir])
        } else {
            iter_gir
        };

        let opt_proto = self
            .module
            .borrow()
            .find_prototype(&"Opt".to_string())
            .unwrap();
        let opt_ty = opt_proto.build(
            vec![elem_ty.clone()],
            &self.module,
            elem_name,
            Rc::clone(&opt_proto),
        )?;
        let some_ty = {
            let adt = opt_ty.as_adt();
            let adt = adt.borrow();
            let cases = adt.ty.cases();
            Rc::clone(cases.get(&"Some".to_string()).unwrap())
        };

        let elem_var = self.define_variable(&elem_name, false, elem_ty.clone());
        let next_fn =
            Self::find_associated_method(&iter_ty, &Token::generic_identifier("next".to_string()))
                .unwrap()
                .into_fn();

        let (body, else_, result_store) = self.for_body(body, else_b)?;
        self.end_scope();
        Ok(Expr::IterLoop {
            iter: Box::new(iter),
            next_fn,
            next_cast_ty: Type::Adt(some_ty),
            store: elem_var,
            body: Box::new(body),
            else_: Box::new(else_.unwrap_or_else(Expr::none_const)),
            result_store,
        })
        */
    }

    fn get(&mut self, object: &AExpr, name: &Token) -> Res<Expr> {
        let (object, field) = self.get_field(object, name)?;
        let field =
            field
                .left()
                .on_err(&self.path, name, "Cannot get class method (must be called)")?;

        if !self.uninitialized_this_fields.contains(&field) {
            Ok(Expr::load(object, &field))
        } else {
            Err(self.err_(name, "Cannot get uninitialized class member.".to_string()))
        }
    }

    // See `binary` for info on `allow_simple`
    fn get_static(&mut self, object: &AExpr, name: &Token, allow_simple: bool) -> Res<Expr> {
        let obj = self.expression(object);
        if let Type::Type(ty) = obj.get_type() {
            let ty = ty.as_value();
            if let ADTType::Enum { cases, .. } = &ty.ty.borrow().ty {
                if let Some(case) = cases.get(&name.lexeme) {
                    match ADT::get_singleton_inst(case, ty.args()) {
                        Some(inst) if allow_simple => Ok(inst),
                        _ => Ok(Expr::TypeGet(Type::Value(Instance::new(
                            Rc::clone(case),
                            Rc::clone(ty.args()),
                        )))),
                    }
                } else {
                    Err(self.err_(name, "Unknown enum case.".to_string()))
                }
            } else {
                Err(self.err_(
                    name,
                    "Static access is only supported on enum types.".to_string(),
                ))
            }
        } else {
            Err(self.err_(
                name,
                "Static access is not supported on values.".to_string(),
            ))
        }
    }

    fn if_(
        &mut self,
        condition: &AExpr,
        then_branch: &AExpr,
        else_branch: &Option<Box<AExpr>>,
    ) -> Res<Expr> {
        let cond = self.expression(condition);
        if cond.get_type() != Type::Bool {
            self.err(
                condition.get_token(),
                "If condition must be a boolean".to_string(),
            );
        }

        self.begin_scope(); // scope for smart casts if applicable
        let mut then_block = self.smart_casts(&cond);
        then_block.push(self.expression(then_branch));
        let then_val = Expr::Block(then_block);
        self.end_scope();

        let else_val = else_branch
            .as_ref()
            .map_or(Expr::none_const_(), |else_branch| {
                self.expression(&else_branch)
            });

        let (phi_type, then_val, else_val) = self.resolver.try_unify_type(then_val, else_val);
        Ok(Expr::if_(cond, then_val, else_val, phi_type))
    }

    /// Tries finding smart casts, where a type can be downcasted
    /// based on a user-code condition.
    /// Will insert variables for downcasts into current scope/function.
    fn smart_casts(&mut self, condition: &Expr) -> Vec<Expr> {
        let mut casts = Vec::new();
        self.find_casts(&mut casts, condition);
        casts
    }

    fn find_casts(&mut self, list: &mut Vec<Expr>, expr: &Expr) {
        if let Expr::Binary {
            left,
            operator,
            right,
        } = expr
        {
            match (operator.t_type, &**left) {
                (TType::And, _) => {
                    self.find_casts(list, &left);
                    self.find_casts(list, &right);
                }

                (TType::Is, Expr::Variable(Variable::Local(var))) => {
                    let ty = right.get_type().into_type();
                    let ty = if var.ty.is_weak_ref() {
                        ty.to_weak()
                    } else {
                        ty.to_strong()
                    };

                    let new_var = self.define_variable(&var.name, false, ty.clone());

                    list.push(Expr::store(
                        Expr::lvar(&new_var),
                        Expr::cast(Expr::lvar(var), ty, CastType::Bitcast),
                        true,
                    ));
                }

                _ => (),
            }
        }
    }

    fn index_get(&mut self, indexed: &AExpr, index: &AExpr, bracket: &Token) -> Res<Expr> {
        let obj = self.expression(indexed);
        let index = self.expression(index);
        self.binary_gir(obj, bracket, index)
    }

    fn index_set(&mut self, indexed: &AExpr, ast_index: &AExpr, ast_value: &AExpr) -> Res<Expr> {
        let mut obj = self.expression(indexed);
        let mut index = self.expression(ast_index);
        let value = self.expression(ast_value);
        let method = self
            .get_operator_overloading_method(TType::RightBracket, &mut obj, &mut index)
            .on_err(
                &self.path,
                ast_index.get_token(),
                "No implementation of operator found for types.",
            )?;

        if value.get_type() == method.ty.borrow().parameters[2].ty {
            Ok(Expr::call(
                Expr::var(Variable::Function(method)),
                vec![obj, index, value],
            ))
        } else {
            Err(self.err_(
                ast_value.get_token(),
                "Setter is of wrong type.".to_string(),
            ))
        }
    }

    fn literal(&mut self, literal: &Literal, token: &Token) -> Res<Expr> {
        match literal {
            Literal::Closure(closure) => self.closure(closure, token),
            _ => Ok(Expr::Literal(literal.clone(), token.clone())),
        }
    }

    fn closure(&mut self, closure: &Closure, token: &Token) -> Res<Expr> {
        let mut name = token.clone();
        name.lexeme = SmolStr::new(format!("closure-{}:{}", token.line, token.index));
        let ast_func = ast::Function {
            sig: FuncSignature {
                name,
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
                modifiers: vec![],
            },
            body: Some(closure.body.clone()),
        };

        let mut gen = Self::for_closure(self);
        let function = gen.create_function(
            ast_func,
            Some(FunctionParam::this_param(&Token::generic_identifier("i64"))),
            None, // TODO: always none? maybe not
        )?;
        gen.generate_function(&function);
        let closure_data = gen.end_closure(self);

        let captured = Rc::new(closure_data.captured);
        function.borrow_mut().parameters[0] = Rc::new(LocalVariable {
            name: Token::generic_identifier("CLOSURE-CAPTURED"),
            ty: Type::ClosureCaptured(Rc::clone(&captured)),
            mutable: false,
        });

        let expr = Expr::Closure { function, captured };
        let var = self.define_variable(
            &Token::generic_identifier("closure-literal"),
            false,
            expr.get_type(),
        );
        Ok(Expr::store(Expr::lvar(&var), expr, true))
    }

    fn return_(&mut self, val: &Option<Box<AExpr>>, err_tok: &Token) -> Res<Expr> {
        let value = val
            .as_ref()
            .map(|v| self.expression(&*v))
            .unwrap_or_else(|| Expr::none_const(err_tok.clone()));

        let value_type = value.get_type();
        let ret_type = self.cur_fn().borrow().ret_type.clone();
        let value = self.resolver.cast_or_none(value, &ret_type).on_err(
            &self.path,
            err_tok,
            &format!("Return expression in function has wrong type (Expected {}, was {})", ret_type, value_type),
        )?;

        Ok(Expr::ret(value))
    }

    fn set(&mut self, object: &AExpr, name: &Token, value: &AExpr) -> Res<Expr> {
        let (object, field) = self.get_field(object, name)?;
        let field = field
            .left()
            .on_err(&self.path, name, "Cannot set class method")?;
        let value = self.expression(value);
        let (value, success) = self.resolver.try_cast(value, &field.ty);

        if !success {
            self.err(name, "Class member is a different type".to_string());
        }
        if !field.mutable && !self.uninitialized_this_fields.contains(&field) {
            self.err(name, "Cannot set immutable class member".to_string());
        }

        let first_set = self.uninitialized_this_fields.remove(&field);
        Ok(Expr::store(Expr::load(object, &field), value, first_set))
    }

    fn unary(&mut self, operator: &Token, right: &AExpr) -> Res<Expr> {
        let right = self.expression(right);
        let ty = right.get_type();

        match operator.t_type {
            TType::Bang if ty != Type::Bool => self.err(
                operator,
                "'!' can only be used on boolean values".to_string(),
            ),

            TType::Minus if !(ty.is_signed_int() || ty.is_float()) => self.err(
                operator,
                "'-' can only be used on signed integers and floats".to_string(),
            ),

            TType::New => return self.alloc_heap(operator, right),

            _ => (),
        };

        Ok(Expr::unary(operator.clone(), right))
    }

    fn alloc_heap(&mut self, op: &Token, inner: Expr) -> Res<Expr> {
        if let Expr::Allocate {
            ty,
            constructor,
            args,
            tok,
        } = inner
        {
            Ok(Expr::Allocate {
                ty: ty.to_strong(),
                constructor,
                args,
                tok,
            })
        } else {
            Err(self.err_(op, "'new' can only be used with constructors".to_string()))
        }
    }

    fn var(&mut self, var: &Token) -> Res<Expr> {
        let res = self.find_var(&var);
        match res {
            Ok(var) => Ok(Expr::var(var)),
            Err(e) => self
                .resolver
                .find_type(&ast::Type::Ident(var.clone()))
                .map(Expr::TypeGet)
                .ok()
                .ok_or(e),
        }
    }

    fn var_with_generics(&mut self, name: &Token, generics: &[ast::Type]) -> Res<Expr> {
        let ty = self.resolver.find_type(&ast::Type::Generic {
            token: name.clone(),
            types: Vec::from(generics),
        })?;

        if let Type::Function(func) = ty {
            Ok(Expr::var(Variable::Function(func)))
        } else {
            Ok(Expr::TypeGet(ty))
        }
    }

    fn when(
        &mut self,
        ast_value: &AExpr,
        branches: &[(AExpr, AExpr)],
        else_branch: &Option<Box<AExpr>>,
    ) -> Res<Expr> {
        let value = self.expression(ast_value);
        let cond_type = value.get_type();

        let mut cases = Vec::with_capacity(branches.len());

        let mut iter = branches.iter();
        let first = iter.next();
        if first.is_none() {
            // There are no branches, just return else branch or nothing
            return Ok(else_branch
                .as_ref()
                .map_or_else(Expr::none_const_, |br| self.expression(br)));
        }

        let (first_cond, mut first_val) =
            self.when_branch(value.clone(), &cond_type, first.unwrap())?;
        let mut first_ty = first_val.get_type();
        for branch in iter {
            let (cond, mut branch_val) = self.when_branch(value.clone(), &cond_type, branch)?;

            if first_ty != Type::None {
                let result = self.resolver.try_unify_type(first_val, branch_val);
                first_ty = result.0.unwrap_or(Type::None);
                first_val = result.1;
                branch_val = result.2;
            } else if branch_val.get_type() != first_ty {
                first_ty = Type::None
            }

            cases.push((cond, branch_val))
        }

        // TODO: Deduplicate this...
        let mut else_br = else_branch.as_ref().map(|e| self.expression(e));
        if let Some(branch_val) = &else_br {
            if first_ty != Type::None {
                let result = self.resolver.try_unify_type(first_val, else_br.unwrap());
                first_ty = result.0.unwrap_or(Type::None);
                first_val = result.1;
                else_br = Some(result.2);
            } else if branch_val.get_type() != first_ty {
                first_ty = Type::None
            }
        }

        cases.insert(0, (first_cond, first_val));
        if else_br.is_none() && !self.can_omit_else(&cond_type, &cases) {
            first_ty = Type::None
        }
        Ok(Expr::switch(
            cases,
            else_br.unwrap_or_else(Expr::none_const_),
            first_ty.type_or_none(),
        ))
    }

    fn when_branch(
        &mut self,
        value: Expr,
        cond_type: &Type,
        branch: &(AExpr, AExpr),
    ) -> Res<(Expr, Expr)> {
        // See note on `binary` about this
        let br_cond = match &branch.0 {
            AExpr::GetStatic { object, name } => self.get_static(object, name, false)?,
            _ => self.expression(&branch.0),
        };
        let br_type = br_cond.get_type();
        if &br_type != cond_type && !br_type.is_type() {
            self.err(
                branch.0.get_token(),
                "Branches of when must be of same type as the value compared.".to_string(),
            );
        }

        // Small hack to get a token that gives the user
        // a useful error without having to add complexity
        // to binary_gir()
        let mut optok = branch.0.get_token().clone();
        optok.t_type = if br_type.is_type() {
            TType::Is
        } else {
            TType::EqualEqual
        };
        let cond = self.binary_gir(value, &optok, br_cond)?;

        self.begin_scope();
        let mut branch_list = self.smart_casts(&cond);
        branch_list.push(self.expression(&branch.1));
        let branch_val = Expr::Block(branch_list);
        self.end_scope();

        Ok((cond, branch_val))
    }

    /// If a when expression can safely give a value even when an else branch is missing.
    /// Only true when switching on enum type with every case present.
    fn can_omit_else(&self, value_ty: &Type, when_cases: &[(Expr, Expr)]) -> bool {
        let adt = if let Some(adt) = value_ty.try_adt() {
            adt
        } else {
            return false;
        };
        let adt = adt.ty.borrow();
        let cases = if let ADTType::Enum { cases } = &adt.ty {
            cases
        } else {
            return false;
        };
        let mut cases: Vec<&MutRc<ADT>> = cases.values().collect();

        for (cond, _) in when_cases.iter() {
            let (op, right) = if let Expr::Binary {
                operator, right, ..
            } = cond
            {
                (operator, right)
            } else {
                panic!("Invalid when condition")
            };

            if op.t_type != TType::Is {
                return false;
            };
            let ty = right.get_type();
            let adt = ty.as_type().as_value();
            let i = cases.iter().position(|c| Rc::ptr_eq(c, &adt.ty));
            if let Some(i) = i {
                cases.remove(i);
            }
        }
        cases.is_empty()
    }

    fn var_def(&mut self, var: &ast::Variable) -> Res<Expr> {
        let init = self.expression(&var.initializer);
        let type_ = init.get_type();
        if type_.is_assignable() {
            let var = self.define_variable(&var.name, var.mutable, type_);
            Ok(Expr::store(Expr::lvar(&var), init, true))
        } else {
            Err(self.err_(
                &var.initializer.get_token(),
                format!("Cannot assign type '{}' to a variable.", type_),
            ))
        }
    }
}
