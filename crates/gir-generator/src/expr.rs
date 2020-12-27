use crate::{passes::FnSig, result::EmitGIRError, GIRGenerator};
use ast::{
    Binary, Block, Break, CSTNode, Call, Expression as AExpr, ForIterCond, GenericIdent, Get,
    GetStatic, LiteralType, Return, When, WhenBranch,
};
use common::MutRc;
use error::{GErr, Res};
use gir_nodes::{
    declaration::{ADTType, LocalVariable, Variable, Visibility},
    expression::CastType,
    gir_err,
    types::{TypeArguments, TypeParameter, VariableModifier},
    Expr, Function, IFaceImpls, Instance, Literal, Type, ADT,
};
use num_traits::Num;
use smol_str::SmolStr;
use std::{convert::TryInto, iter::FromIterator, rc::Rc};
use syntax::kind::SyntaxKind;

/// This impl contains all code of the generator that directly
/// produces expressions.
/// This is split into its own file for readability reasons;
/// a 1500-line file containing everything is difficult to navigate.
impl GIRGenerator {
    pub(crate) fn expression(&mut self, expression: &AExpr) -> Expr {
        let expr = match expression {
            AExpr::Binary(binary) => self.binary(binary),

            AExpr::Block(block) => Ok(self.block(block)),

            AExpr::Break(br) => Ok(self.break_(br)),

            AExpr::Call(call) => self.call(call),

            AExpr::For(expr) if expr.iter_cond().is_some() => {
                self.for_iter(expr.iter_cond().unwrap(), expr.body(), expr.else_branch())
            }
            AExpr::For(expr) => {
                Ok(self.for_cond(expr.condition().unwrap(), expr.body(), expr.else_branch()))
            }

            AExpr::Get(get) => {
                if get.property().type_args().next().is_some() {
                    Err(gir_err(get.property().cst, GErr::E211))
                } else {
                    self.get(get, false).map(|e| e.0)
                }
            }

            AExpr::GetStatic(get) => self.get_static(get, true),

            AExpr::Grouping(inner) => Ok(self.expression(&inner.inner())),

            AExpr::If(expr) => {
                Ok(self.if_(expr.condition(), expr.then_branch(), expr.else_branch()))
            }

            AExpr::Literal(literal) => self.literal(literal),

            AExpr::LiteralClosure(closure) => self.closure(closure),

            AExpr::Prefix(expr) => self.prefix(expr.operator(), expr.right(), &expr.cst),

            AExpr::Return(ret) => self.return_(ret),

            AExpr::Variable(var) => self.var(var),

            AExpr::VarDef(var) => self.var_def(var),

            AExpr::When(when) => self.when(when),
        };

        // TODO: a proper 'poisoned' expression
        self.eat(expr).unwrap_or(Expr::Literal(Literal::Any))
    }

    fn binary(&mut self, expr: &Binary) -> Res<Expr> {
        let op = expr.operator();
        if op == SyntaxKind::Equal {
            return self.assignment(expr.left(), expr.right());
        }

        let left = self.expression(&expr.left());
        let ast_right = expr.right();

        // Account for an edge case with simple enums, where `A:A` incorrectly gets
        // turned into a regular value instead of a type get.
        let right = match ast_right {
            AExpr::GetStatic(get) if op == SyntaxKind::Is => self.get_static(&get, false)?,
            _ => self.expression(&ast_right),
        };

        self.binary_gir(&expr.cst, left, op, right)
    }

    fn assignment(&mut self, to: AExpr, value: AExpr) -> Res<Expr> {
        // Account for edge case where it is illegal to get an
        // uninitialized ADT member; it's fine here since it's being written
        // and not read
        let (lvalue, was_uninit) = if let AExpr::Get(get) = &to {
            self.get(get, true)?
        } else {
            (self.expression(&to), false)
        };
        let rvalue = self.expression(&value);
        let (rvalue, matching_types) = self.try_cast(rvalue, &lvalue.get_type());

        if !was_uninit && !lvalue.assignable() {
            Err(gir_err(to.cst(), GErr::E200(lvalue.human_name())))
        } else if !matching_types {
            Err(gir_err(value.cst(), GErr::E201))
        } else {
            Ok(Expr::store(lvalue, rvalue, was_uninit))
        }
    }

    fn binary_gir(
        &mut self,
        cst: &CSTNode,
        mut left: Expr,
        operator: SyntaxKind,
        mut right: Expr,
    ) -> Res<Expr> {
        let left_ty = left.get_type();
        let right_ty = right.get_type();

        if (left_ty == right_ty && left_ty.is_number())
            || (left_ty.is_int() && right_ty.is_int())
            || left_ty.is_float() && right_ty.is_float()
            || (operator == SyntaxKind::Is && right_ty.is_type())
        {
            if operator == SyntaxKind::And || operator == SyntaxKind::Or {
                Ok(Self::binary_logic(left, operator, right))
            } else {
                let (_, left, right) = self.try_unify_type(left, right);
                Ok(Expr::binary(operator, left, right))
            }
        } else {
            let method_var = self
                .get_operator_overloading_method(operator, &mut left, &mut right)
                .or_err(cst, GErr::E202)?;

            let mut expr = Expr::call(Expr::var(Variable::Function(method_var)), vec![left, right]);
            if operator == SyntaxKind::BangEqual {
                // Invert the result if this is `!=`, as the method is from `Eq`
                expr = Expr::unary(SyntaxKind::Bang, expr);
            }
            Ok(expr)
        }
    }

    /// Logic operators need special treatment for shortcircuiting behavior
    fn binary_logic(left: Expr, operator: SyntaxKind, right: Expr) -> Expr {
        if operator == SyntaxKind::And {
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

    fn block(&mut self, block: &Block) -> Expr {
        self.begin_scope();
        let exprs: Vec<_> = block.expressions().map(|e| self.expression(&e)).collect();
        self.end_scope();

        if exprs.is_empty() {
            Expr::none_const()
        } else {
            Expr::Block(exprs)
        }
    }

    fn break_(&mut self, expr: &Break) -> Expr {
        if self.current_loop_ty.is_none() {
            self.err(expr.cst(), GErr::E207);
        }

        let expr = expr
            .value()
            .map(|expr| {
                let expression = self.expression(&expr);
                self.set_loop_type(&expression.get_type(), &expr.cst());
                expression
            })
            .unwrap_or_else(Expr::none_const);

        Expr::break_(expr)
    }

    fn call(&mut self, call: &Call) -> Res<Expr> {
        let mut args = call.args().map(|a| self.expression(&a)).collect::<Vec<_>>();

        match call.callee() {
            // Method call while a `this` member is still uninitialized
            AExpr::Get(get) if !self.uninitialized_this_fields.is_empty() => {
                Err(gir_err(get.cst, GErr::E203))
            }

            // Method call
            AExpr::Get(get) => {
                let (object, field) = self.get_field(&get)?;
                let func = field.right().or_err(&get.cst, GErr::E204)?;

                let obj_ty = object.get_type();
                let parent_ty_args = obj_ty.type_args().unwrap();
                args.insert(0, object);

                let ty_args = get
                    .property()
                    .type_args()
                    .map(|t| self.find_type(&t))
                    .collect::<Res<Vec<_>>>()?;

                let ty_args = if ty_args.is_empty() {
                    self.maybe_infer_ty_args(
                        &func.borrow().parameters,
                        &func.borrow().type_parameters,
                        &args,
                        parent_ty_args.len(),
                        &get.cst,
                    )?
                } else {
                    ty_args
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
                    call.args(),
                    &get.cst,
                    true,
                )?;
                Ok(Expr::call(Expr::var(Variable::Function(func)), args))
            }

            // Can be either a constructor or function call
            _ => {
                let mut callee = self.expression(&call.callee());
                let mut callee_type = callee.get_type();

                if let Some(constructors) = callee_type.get_constructors() {
                    let mut ty_vars = Rc::clone(callee_type.type_args().unwrap());
                    let constructor = Rc::clone(
                        constructors
                            .iter()
                            .try_find(|constructor| {
                                let constructor = constructor.borrow();

                                // Different args count
                                if constructor.parameters.len() - 1 != args.len() {
                                    return Ok(false);
                                }

                                // If there's no type args yet try inferring them
                                if ty_vars.is_empty() {
                                    args.insert(0, Expr::none_const());
                                    let res = self.maybe_infer_ty_args(
                                        &constructor.parameters,
                                        &constructor.type_parameters,
                                        &args,
                                        0,
                                        &call.cst,
                                    );
                                    if let Ok(args) = res {
                                        ty_vars = Rc::new(args);
                                    }
                                    args.remove(0);
                                };

                                // Now check if the args are the correct type
                                let correct_args_types =
                                    constructor.parameters.iter().skip(1).zip(args.iter()).all(
                                        |(param, arg)| {
                                            let ty1 = arg.get_type();
                                            let ty2 = param.ty.resolve(&ty_vars);
                                            ty1 == ty2 || self.can_cast_type(&ty1, &ty2).is_some()
                                        },
                                    );

                                Ok(correct_args_types)
                            })?
                            .or_err(&call.cst, GErr::E219)?,
                    );

                    {
                        // Cast/convert all arguments to fit
                        for (param, arg) in constructor
                            .borrow()
                            .parameters
                            .iter()
                            .skip(1)
                            .zip(args.iter_mut())
                        {
                            self.try_cast_in_place(arg, &param.ty.resolve(&ty_vars));
                        }
                    }

                    callee_type.set_type_args(ty_vars);
                    Ok(Expr::Allocate {
                        ty: callee_type.into_type().to_weak(),
                        constructor,
                        args,
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
                                &call.cst,
                            )?)
                        };
                        func.set_args(ty_args)
                    }

                    self.check_func_args_(
                        &callee.get_type(), // Get it again to ensure inferred type args are present
                        &mut args,
                        call.args(),
                        &call.cst,
                        false,
                    )?;
                    Ok(Expr::call(callee, args))
                }
            }
        }
    }

    /// Check a function call's arguments for correctness,
    /// possibly adding a cast if required.
    #[allow(clippy::too_many_arguments)] // Not ideal, but no real way of fixing this
    fn check_func_args(
        &mut self,
        mut parameters: impl Iterator<Item = Type>,
        type_args: Option<&Rc<TypeArguments>>,
        args: &mut Vec<Expr>,
        ast_args: impl Iterator<Item = AExpr>,
        allow_variadic: bool,
        err_cst: &CSTNode,
        is_method: bool,
    ) -> Res<()> {
        let para_len = parameters.size_hint().0;
        if para_len > args.len() || (para_len < args.len() && !allow_variadic) {
            return Err(gir_err(
                err_cst.clone(),
                GErr::E216 {
                    expected: parameters.size_hint().0,
                    was: args.len(),
                },
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
                .cast_or_none(
                    args.swap_remove(0),
                    &parameters.next().unwrap().resolve(type_args.unwrap()),
                )
                .or_err(err_cst, GErr::E217)?;
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
            .zip(ast_args)
        {
            let arg_type = argument.get_type();
            let success = self.try_cast_in_place(argument, &parameter);
            if !success {
                self.error(gir_err(
                    ast.cst(),
                    GErr::E218 {
                        expected: parameter.to_string(),
                        was: arg_type.to_string(),
                    },
                ))
            }
        }

        Ok(())
    }

    /// Same as above, but takes a function instead.
    /// `func` should be `Type::Function` or `Type::Closure`, otherwise returns an error.
    fn check_func_args_(
        &mut self,
        func: &Type,
        args: &mut Vec<Expr>,
        ast_args: impl Iterator<Item = AExpr>,
        err_cst: &CSTNode,
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
                func.ty.borrow().variadic,
                err_cst,
                is_method,
            ),

            Type::Closure(closure) => self.check_func_args(
                closure.parameters.iter().cloned(),
                None,
                args,
                ast_args,
                false,
                err_cst,
                is_method,
            ),

            _ => Err(gir_err(err_cst.clone(), GErr::E215(func.to_string()))),
        }
    }

    /// Try inferring a set of type arguments from a call.
    fn maybe_infer_ty_args(
        &mut self,
        parameters: &[Rc<LocalVariable>],
        type_params: &[TypeParameter],
        arguments: &[Expr],
        skip: usize,
        err_cst: &CSTNode,
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

        let args = search_res.or_err(err_cst, GErr::E214)?;
        self.validate_type_args(&args, type_params, err_cst);
        Ok(args)
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
            (Type::Variable(var), _) if var.name == ty_param.name => match var.modifier {
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

    fn for_cond(&mut self, condition: AExpr, body: AExpr, else_b: Option<AExpr>) -> Expr {
        let cond = self.expression(&condition);
        if cond.get_type() != Type::Bool {
            self.err(condition.cst(), GErr::E220);
        }

        self.begin_scope();
        let mut cast_block = self.smart_casts(&cond);
        let (body, else_, phi_ty) = self.for_body(body, else_b);
        self.end_scope();
        cast_block.push(body);
        Expr::loop_(cond, Expr::Block(cast_block), else_, phi_ty)
    }

    /// Return type, in order:
    /// - for body
    /// - else branch
    /// - phi type
    fn for_body(&mut self, body_ast: AExpr, else_b: Option<AExpr>) -> (Expr, Expr, Option<Type>) {
        let prev_loop_ty = std::mem::replace(&mut self.current_loop_ty, Some(Type::Any));

        let body = self.expression(&body_ast);
        let body_type = body.get_type();
        self.set_loop_type(&body_type, &body_ast.cst()); // todo maybe elim copy

        let else_val = else_b.as_ref().map_or(Expr::none_const(), |else_branch| {
            self.expression(&else_branch)
        });
        let (phi_ty, body, else_) = self.try_unify_type(body, else_val);

        self.current_loop_ty = prev_loop_ty;
        (body, else_, phi_ty)
    }

    /* Have a visualization of the desugaring this performs:
    val iter = Range(0, 5)
    for (i in iter) {
        i = 3
    }

    Into:

    {
        var i = iter.next()
        for (i is Opt[i64]:Some) {
            val i = cast[i64](i)
            val loop_res = { // USER CODE
                i = 3
            }
            loop_val = iter.next()
            loop_res
        }
    }
    */
    fn for_iter(&mut self, cond: ForIterCond, body: AExpr, else_b: Option<AExpr>) -> Res<Expr> {
        self.begin_scope();

        let iter_gir = self.expression(&cond.iterator());
        let impls = self.get_iface_impls(&iter_gir.get_type());
        let (iter_value, next_fn, elem_ty) =
            self.get_iterator_value(iter_gir, &*impls.borrow(), &cond.cst)?;

        let opt_adt = self.module.borrow().find_decl("Opt").unwrap().into_adt();
        let some_adt = {
            let adt = opt_adt.borrow();
            let cases = adt.ty.cases();
            Rc::clone(cases.get("Some").unwrap())
        };
        let some_ty = Type::StrongRef(Instance::new(Rc::clone(&some_adt), Rc::clone(&elem_ty)));

        let next_call = Expr::call(Expr::var(Variable::Function(next_fn)), vec![iter_value]);
        let (inital_store_expr, loop_var) = self.temp_variable(next_call.clone(), cond.name());
        let next_call_store = Expr::store(Expr::lvar(&loop_var), next_call, false);

        let cond = Expr::binary(
            SyntaxKind::Is,
            Expr::lvar(&loop_var),
            Expr::type_get(some_ty.clone()),
        );

        self.begin_scope();
        let mut clone = (*loop_var).clone();
        clone.ty = elem_ty[0].clone();
        let loop_inner_var = self.define_variable_(clone, None);
        let loop_cast_store = Expr::store(
            Expr::lvar(&loop_inner_var),
            Expr::cast(Expr::lvar(&loop_var), elem_ty[0].clone(), CastType::Bitcast),
            true,
        );

        let (body, else_, phi_ty) = self.for_body(body, else_b);
        let body_block = vec![loop_cast_store, body];
        self.end_scope();

        let (body_store, body_var) =
            self.temp_variable(Expr::Block(body_block), "body-value".into());
        let loop_block = vec![body_store, next_call_store, Expr::lvar(&body_var)];

        self.end_scope();
        let loop_expr = Expr::loop_(cond, Expr::Block(loop_block), else_, phi_ty);
        let block = vec![inital_store_expr, loop_expr];
        Ok(Expr::Block(block))
    }

    fn get_iterator_value(
        &mut self,
        value: Expr,
        impls: &IFaceImpls,
        cst: &CSTNode,
    ) -> Res<(Expr, Instance<Function>, Rc<TypeArguments>)> {
        let find_iface = |name: &str| {
            impls.interfaces.iter().find(|(iface, _)| {
                let iface = iface.as_strong_ref().ty.borrow();
                iface.name == name && iface.module.borrow().path.is(&["std", "iter"])
            })
        };
        let iter_impl = find_iface("Iter").or_else(|| find_iface("ToIter"));

        if let Some((iface, impl_)) = iter_impl {
            let elem_ty = Rc::clone(iface.type_args().unwrap());
            let func = impl_.methods.values().next().unwrap();
            let next_fn = Instance::new(Rc::clone(func), Rc::clone(&elem_ty));
            Ok((value, next_fn, elem_ty))
        } else {
            Err(gir_err(cst.clone(), GErr::E237))
        }
    }

    // Handle a get expression. allow_uninit controls if uninitialized ADT
    // members can be accessed; they will be removed if they are.
    // Return value also specifies if the value was uninitialized if allow_uninit is true.
    // This is special behavior is used for assignment.
    fn get(&mut self, get: &Get, allow_uninit: bool) -> Res<(Expr, bool)> {
        let (object, field) = self.get_field(get)?;
        let field = field.left().or_err(&get.property().cst, GErr::E221)?;

        if allow_uninit || !self.uninitialized_this_fields.contains(&field) {
            Ok((
                Expr::load(object, &field),
                !allow_uninit || self.uninitialized_this_fields.remove(&field),
            ))
        } else {
            Err(gir_err(get.property().cst, GErr::E222))
        }
    }

    // See `binary` for info on `allow_simple`
    fn get_static(&mut self, get: &GetStatic, allow_simple: bool) -> Res<Expr> {
        let obj = self.expression(&get.callee());
        let name = get.property();

        if let Type::Type(ty) = obj.get_type() {
            let ty = ty.try_adt().unwrap();
            if let ADTType::Enum { cases, .. } = &ty.ty.borrow().ty {
                if let Some(case) = cases.get(&name) {
                    match ADT::get_singleton_inst(case, ty.args()) {
                        Some(inst) if allow_simple => Ok(inst),
                        _ => Ok(Expr::TypeGet(Type::Value(Instance::new(
                            Rc::clone(case),
                            Rc::clone(ty.args()),
                        )))),
                    }
                } else {
                    Err(gir_err(get.callee().cst(), GErr::E223))
                }
            } else {
                Err(gir_err(get.callee().cst(), GErr::E224))
            }
        } else {
            Err(gir_err(get.callee().cst(), GErr::E225))
        }
    }

    fn if_(&mut self, condition: AExpr, then_branch: AExpr, else_branch: Option<AExpr>) -> Expr {
        let cond = self.expression(&condition);
        if cond.get_type() != Type::Bool {
            self.err(condition.cst(), GErr::E220);
        }

        self.begin_scope(); // scope for smart casts if applicable
        let mut then_block = self.smart_casts(&cond);
        then_block.push(self.expression(&then_branch));
        let then_val = Expr::Block(then_block);
        self.end_scope();

        let else_val = else_branch
            .as_ref()
            .map_or(Expr::none_const(), |else_branch| {
                self.expression(&else_branch)
            });

        let (phi_type, then_val, else_val) = self.try_unify_type(then_val, else_val);
        Expr::if_(cond, then_val, else_val, phi_type)
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
            match (operator, &**left) {
                (SyntaxKind::And, _) => {
                    self.find_casts(list, &left);
                    self.find_casts(list, &right);
                }

                (SyntaxKind::Is, Expr::Variable(Variable::Local(var))) => {
                    let ty = right.get_type().into_type();
                    let ty = if var.ty.is_weak_ref() {
                        ty.to_weak()
                    } else {
                        ty.to_strong()
                    };

                    let mut clone = (**var).clone();
                    clone.ty = ty.clone();
                    let new_var = self.define_variable_(clone, None);

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

    fn literal(&mut self, literal: &ast::Literal) -> Res<Expr> {
        let (text, ty) = literal.get();
        Ok(match ty {
            LiteralType::True => Expr::Literal(Literal::Bool(true)),
            LiteralType::False => Expr::Literal(Literal::Bool(false)),
            LiteralType::Int => Expr::Literal(self.numeric_literal(text, &literal.cst, false)?),
            LiteralType::Float => Expr::Literal(self.numeric_literal(text, &literal.cst, true)?),
            LiteralType::String if self.flags.no_std => {
                return Err(gir_err(literal.cst(), GErr::E238))
            }
            LiteralType::String => Expr::Literal(Literal::String {
                text: self.string_literal(text, literal)?,
                ty: self.intrinsics.string_type.clone().unwrap(),
            }),
        })
    }

    fn numeric_literal(&mut self, text: SmolStr, cst: &CSTNode, float: bool) -> Res<Literal> {
        let mut split = text.split(|c| c == 'u' || c == 'i' || c == 'f');
        let value = split.next().unwrap().trim();
        let types = split.next().map(|t| {
            (
                text.chars()
                    .find(|c| *c == 'u' || *c == 'i' || *c == 'f')
                    .unwrap(),
                t,
            )
        });

        Ok(match types {
            Some(('i', "8")) => Literal::I8(self.parse_numeric_literal(value, cst)?),
            Some(('i', "16")) => Literal::I16(self.parse_numeric_literal(value, cst)?),
            Some(('i', "32")) => Literal::I32(self.parse_numeric_literal(value, cst)?),
            #[cfg(target_pointer_width = "64")]
            Some(('i', "s")) => Literal::I64(self.parse_numeric_literal(value, cst)?),
            #[cfg(not(target_pointer_width = "64"))]
            Some(('i', "s")) => Literal::I32(self.parse_numeric_literal(value, cst)?),

            Some(('u', "8")) => Literal::U8(self.parse_numeric_literal(value, cst)?),
            Some(('u', "16")) => Literal::U16(self.parse_numeric_literal(value, cst)?),
            Some(('u', "32")) => Literal::U32(self.parse_numeric_literal(value, cst)?),
            Some(('u', "64")) => Literal::U64(self.parse_numeric_literal(value, cst)?),
            #[cfg(target_pointer_width = "64")]
            Some(('u', "s")) => Literal::U64(self.parse_numeric_literal(value, cst)?),
            #[cfg(not(target_pointer_width = "64"))]
            Some(('u', "s")) => Literal::U32(self.parse_numeric_literal(value, cst)?),

            Some(('f', "32")) => Literal::F32(self.parse_numeric_literal(value, cst)?),

            _ if float => Literal::F64(self.parse_numeric_literal(value, cst)?),
            _ => Literal::I64(self.parse_numeric_literal(value, cst)?),
        })
    }

    fn parse_numeric_literal<T: Num>(&self, text: &str, cst: &CSTNode) -> Res<T> {
        T::from_str_radix(text.trim(), 10)
            .ok()
            .or_err(cst, GErr::E233)
    }

    /// Replace all escape sequences inside a string literal with their proper char
    /// and return either an error or the finished string literal
    fn string_literal(&mut self, text: SmolStr, cst: &ast::Literal) -> Res<SmolStr> {
        let mut chars = text.chars().skip(1).collect::<Vec<_>>();
        chars.pop(); // Final '"'

        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '\\' {
                chars.remove(i);
                if chars.len() == i {
                    return Err(gir_err(cst.cst(), GErr::E231));
                }

                chars[i] = match chars[i] {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '\\' => '\\',
                    '0' => '\0',
                    '"' => '"',

                    'u' => {
                        let mut hex_chars = Vec::with_capacity(6);
                        while chars[i + 1].is_ascii_hexdigit() {
                            hex_chars.push(chars.remove(i + 1));
                        }
                        u32::from_str_radix(&String::from_iter(hex_chars), 16)
                            .unwrap()
                            .try_into()
                            .unwrap()
                    }

                    _ => return Err(gir_err(cst.cst(), GErr::E232)),
                }
            }
            i += 1;
        }
        Ok(chars.iter().collect::<String>().into())
    }

    fn closure(&mut self, func: &ast::Function) -> Res<Expr> {
        let signature = func.sig();
        let ret_type = signature
            .ret_type()
            .map(|ty| self.find_type(&ty))
            .transpose()?;
        let params = signature
            .parameters()
            .map(|ast| Ok((ast.name(), self.find_type(&ast._type())?)))
            .collect::<Vec<_>>();

        let mut gen = Self::for_closure(self);
        let function = gen.create_function(FnSig {
            name: SmolStr::new_inline(&format!(
                "closure-{}",
                u32::from(signature.cst.text_range().start())
            )),
            visibility: Visibility::Private,
            params: box params.into_iter(),
            type_parameters: Rc::new(vec![]),
            ret_type,
            ast: Some(func.clone()),
        })?;
        gen.generate_function(&function);
        let closure_data = gen.end_closure(self);

        let captured = Rc::new(closure_data.captured);
        function.borrow_mut().parameters.insert(
            0,
            Rc::new(LocalVariable {
                name: SmolStr::new_inline("CLOSURE-CAPTURED"),
                ty: Type::ClosureCaptured(Rc::clone(&captured)),
                mutable: false,
            }),
        );

        let expr = Expr::Closure { function, captured };
        let (store, _) = self.temp_variable(expr, "closure-store".into());
        Ok(store)
    }

    fn prefix(&mut self, operator: SyntaxKind, ast_right: AExpr, cst: &CSTNode) -> Res<Expr> {
        let right = self.expression(&ast_right);
        let ty = right.get_type();

        match operator {
            SyntaxKind::Bang if ty != Type::Bool => self.err(cst.clone(), GErr::E227),

            SyntaxKind::Minus if !(ty.is_signed_int() || ty.is_float()) => {
                self.err(cst.clone(), GErr::E228)
            }

            SyntaxKind::New => return self.alloc_heap(right, cst),

            _ => (),
        };

        Ok(Expr::unary(operator, right))
    }

    fn alloc_heap(&mut self, inner: Expr, err_cst: &CSTNode) -> Res<Expr> {
        if let Expr::Allocate {
            ty,
            constructor,
            args,
        } = inner
        {
            Ok(Expr::Allocate {
                ty: ty.to_strong(),
                constructor,
                args,
            })
        } else {
            Err(gir_err(err_cst.clone(), GErr::E226))
        }
    }

    fn return_(&mut self, ret: &Return) -> Res<Expr> {
        let value = ret
            .value()
            .map(|v| self.expression(&v))
            .unwrap_or_else(Expr::none_const);

        let value_type = value.get_type();
        let ret_type = self.cur_fn().borrow().ret_type.clone();
        let value = self
            .cast_or_none(value, &ret_type)
            .or_error(&ret.cst, || GErr::E212 {
                expected: ret_type.to_string(),
                was: value_type.to_string(),
            })?;

        Ok(Expr::ret(value))
    }

    fn var(&mut self, var: &GenericIdent) -> Res<Expr> {
        let has_ty_args = var.type_args().next().is_some();
        let variable = self.find_var(&var.name(), &var.cst);

        match (has_ty_args, variable) {
            (true, Ok(Variable::Local(_))) => Err(gir_err(var.cst(), GErr::E213)),

            (true, Ok(Variable::Function(mut func))) => {
                let args = var
                    .type_args()
                    .map(|p| self.find_type(&p))
                    .collect::<Res<Vec<_>>>()?;
                func.set_args(Rc::new(args));
                Ok(Expr::var(Variable::Function(func)))
            }

            (false, Ok(var)) => Ok(Expr::var(var)),

            (_, Err(e)) => self
                .symbol_with_type_args(&var.name(), var.type_args(), &var.cst)
                .map(Expr::type_get)
                .map_err(|_| e),
        }
    }

    fn when(&mut self, when: &When) -> Res<Expr> {
        let value = self.expression(&when.condition());
        let cond_type = value.get_type();

        let mut cases = Vec::with_capacity(8);

        let mut iter = when.branches();
        let first = iter.next();
        if first.is_none() {
            // There are no branches, just return else branch or nothing
            return Ok(when
                .else_branch()
                .map_or_else(Expr::none_const, |br| self.expression(&br)));
        }

        let (first_cond, mut first_val) =
            self.when_branch(value.clone(), &cond_type, first.unwrap())?;
        let mut first_ty = first_val.get_type();
        for branch in iter {
            let (cond, mut branch_val) = self.when_branch(value.clone(), &cond_type, branch)?;

            if first_ty != Type::None {
                let result = self.try_unify_type(first_val, branch_val);
                first_ty = result.0.unwrap_or(Type::None);
                first_val = result.1;
                branch_val = result.2;
            } else if branch_val.get_type() != first_ty {
                first_ty = Type::None
            }

            cases.push((cond, branch_val))
        }

        // TODO: Deduplicate this...
        let mut else_br = when.else_branch().map(|e| self.expression(&e));
        if let Some(branch_val) = &else_br {
            if first_ty != Type::None {
                let result = self.try_unify_type(first_val, else_br.unwrap());
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
            else_br.unwrap_or_else(Expr::none_const),
            first_ty.type_or_none(),
        ))
    }

    fn when_branch(
        &mut self,
        value: Expr,
        cond_type: &Type,
        branch: WhenBranch,
    ) -> Res<(Expr, Expr)> {
        let cond = branch.condition();
        // See note on `binary` about this
        let br_cond = match &cond {
            AExpr::GetStatic(get) => self.get_static(&get, false)?,
            _ => self.expression(&cond),
        };

        let br_type = br_cond.get_type();
        if &br_type != cond_type && !br_type.is_type() {
            self.err(branch.cst(), GErr::E229);
        }

        let op = if br_type.is_type() {
            SyntaxKind::Is
        } else {
            SyntaxKind::EqualEqual
        };
        let cond = self.binary_gir(&branch.condition().cst(), value, op, br_cond)?;

        self.begin_scope();
        let mut branch_list = self.smart_casts(&cond);
        branch_list.push(self.expression(&branch.branch()));
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

            if *op != SyntaxKind::Is {
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
        let init = self.expression(&var.initializer());
        let type_ = init.get_type();
        if type_.is_assignable() {
            let var = self.define_variable(var.clone(), type_);
            Ok(Expr::store(Expr::lvar(&var), init, true))
        } else {
            Err(gir_err(
                var.initializer().cst(),
                GErr::E230(type_.to_string()),
            ))
        }
    }
}
