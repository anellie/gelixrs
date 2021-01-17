use std::rc::Rc;

use common::MutRc;
use gir_nodes::{
    declaration::Variable,
    expression::{CastType, Intrinsic},
    Expr, Function, Instance, Literal, Type, ADT,
};
use inkwell::{
    basic_block::BasicBlock,
    types::{AnyTypeEnum, BasicTypeEnum, StructType},
    values::{BasicValueEnum, PointerValue},
    FloatPredicate, IntPredicate,
};
use std::mem;
use syntax::kind::SyntaxKind;

use super::{type_adapter::IRType, IRGenerator, LLPtr, LLValue, LoopData};

impl IRGenerator {
    pub(crate) fn expression(&mut self, expr: &Expr) -> LLValue {
        self.expression_(expr, false)
    }

    pub(crate) fn expression_(&mut self, expr: &Expr, no_load: bool) -> LLValue {
        if self.builder.get_insert_block().is_none() {
            return self.none_const.clone();
        }

        match expr {
            Expr::Block(block) => {
                self.push_local_scope();
                let ret = block
                    .iter()
                    .fold(self.none_const.clone(), |_, ex| self.expression(ex));
                self.pop_locals_lift(&ret);
                ret
            }

            Expr::Literal(literal) => self.literal(literal),

            Expr::Allocate {
                ty,
                constructor,
                args,
                ..
            } => self.allocate(ty, constructor, args),

            Expr::Variable(var) => match var {
                Variable::Local(_) if no_load => self.get_variable(var).val(),
                Variable::Local(_) => self.load_ptr(self.get_variable(var)),
                Variable::Function(func) => LLValue::cpy(
                    self.get_or_create(func)
                        .as_global_value()
                        .as_pointer_value()
                        .into(),
                    &IRType::Other,
                ),
            },

            Expr::Load { object, field } => {
                let obj = self.expression(object);
                let ptr = self.struct_gep(&obj.into_ptr(), field.index);
                if no_load {
                    LLValue::from(ptr.into(), &field.ty)
                } else {
                    self.load_ptr(&LLPtr::from(ptr, &field.ty))
                }
            }

            Expr::Store {
                location,
                value,
                first_store,
            } => {
                let store = self.expression_(location, true).into_ptr();
                let value = self.expression(value);
                self.build_store(&store, &value, *first_store);
                if *first_store && !location.is_struct_get() {
                    self.locals().push(store)
                }

                value
            }

            Expr::Binary {
                left,
                operator,
                right,
            } => {
                let left = self.expression(left);
                if *operator == SyntaxKind::Is {
                    self.binary_is(left, &right.get_type_get_type())
                } else {
                    let right = self.expression(right);
                    self.binary(left, *operator, right)
                }
            }

            Expr::Unary { operator, right } => self.unary(right, *operator),

            Expr::Call { callee, arguments } => {
                let ir_callee = self.expression(callee);
                self.build_call(ir_callee.into_pointer_value(), expr.get_type(), arguments)
            }

            Expr::If {
                condition,
                then_branch,
                else_branch,
                phi_type,
            } => self.if_(condition, then_branch, else_branch, phi_type.is_some()),

            Expr::Switch {
                branches,
                else_branch,
                phi_type,
            } => self.switch(branches, else_branch, phi_type.is_some()),

            Expr::Loop {
                condition,
                body,
                else_branch,
                phi_type,
            } => self.loop_(condition, body, else_branch, phi_type),

            Expr::Break(value) => {
                if self.loop_data.as_ref().unwrap().phi_nodes.is_some() {
                    let node = (self.expression(value), self.last_block());
                    self.loop_data
                        .as_mut()
                        .unwrap()
                        .phi_nodes
                        .as_mut()
                        .unwrap()
                        .push(node);
                }
                self.builder
                    .build_unconditional_branch(&self.loop_data.as_ref().unwrap().end_block);
                self.builder.clear_insertion_position();
                self.none_const.clone()
            }

            Expr::Return(value) => {
                let value = self.expression(value);
                self.increment_refcount(&value);
                self.decrement_all_locals();

                if matches!(value.ty, IRType::None) {
                    self.builder.build_return(None);
                } else {
                    self.builder.build_return(Some(&*value));
                }

                self.builder.clear_insertion_position();
                self.none_const.clone()
            }

            Expr::Cast { inner, to, method } => self.cast(inner, to, method),

            Expr::Intrinsic(int) => self.intrinsic(int),

            Expr::TypeGet(_) => panic!("Invalid IR instruction"),

            Expr::Closure { .. } => todo!(),
        }
    }

    fn allocate(
        &mut self,
        ty: &Type,
        constructor: &MutRc<Function>,
        constructor_args: &[Expr],
    ) -> LLValue {
        let args = constructor_args
            .iter()
            .map(|a| self.expression(a))
            .collect();
        self.allocate_raw_args(ty, constructor, args)
    }

    fn allocate_raw_args(
        &mut self,
        ty: &Type,
        constructor: &MutRc<Function>,
        constructor_args: Vec<LLValue>,
    ) -> LLValue {
        let (ir_ty, tyinfo) = self.ir_ty_raw(ty);
        let alloc = self.create_alloc(ty.clone(), ir_ty, ty.is_ref_adt());

        let adt = ty.try_adt().unwrap();
        let constructor = self.get_or_create(&Instance::new(
            Rc::clone(constructor),
            Rc::clone(adt.args()),
        ));
        let instantiator = self
            .get_or_create(&adt.get_method("new-instance"))
            .as_global_value()
            .as_pointer_value();

        let llptr = LLPtr::from(alloc, &ty);
        self.maybe_init_type_info(&adt.ty, &llptr, tyinfo);
        self.build_alloc_and_init(
            llptr,
            ty,
            instantiator,
            constructor.as_global_value().as_pointer_value(),
            constructor_args,
        )
    }

    fn maybe_init_type_info(&mut self, ty: &MutRc<ADT>, alloc: &LLPtr, info: Option<PointerValue>) {
        if !ty.borrow().ty.is_extern_class() {
            let gep = self.get_type_info_field(alloc);
            self.builder.build_store(gep, info.unwrap());
        }
    }

    fn build_alloc_and_init(
        &mut self,
        alloc: LLPtr,
        ty: &Type,
        instantiator: PointerValue,
        constructor: PointerValue,
        mut arguments: Vec<LLValue>,
    ) -> LLValue {
        self.increment_refcount(&alloc.val());
        let ptr = alloc.clone();

        self.builder
            .build_call(instantiator, &[(*ptr).into()], "inst");

        for arg in &arguments {
            self.increment_refcount(&arg);
        }
        arguments.insert(0, ptr.val());
        let ll_args = arguments.iter().map(|a| **a).collect::<Vec<_>>();
        self.builder.build_call(constructor, &ll_args, "constr");
        for arg in arguments.iter().skip(1) {
            self.decrement_refcount(&arg);
        }

        self.locals().push(ptr);
        alloc.into_val()
    }

    fn binary(&self, left_: LLValue, operator: SyntaxKind, right_: LLValue) -> LLValue {
        match (*left_, *right_) {
            (BasicValueEnum::IntValue(left), BasicValueEnum::IntValue(right)) => LLValue::cpy(
                BasicValueEnum::IntValue(match operator {
                    SyntaxKind::Plus => self.builder.build_int_add(left, right, "add"),
                    SyntaxKind::Minus => self.builder.build_int_sub(left, right, "sub"),
                    SyntaxKind::Star => self.builder.build_int_mul(left, right, "mul"),
                    SyntaxKind::Slash => self.builder.build_int_signed_div(left, right, "div"),
                    SyntaxKind::And => self.builder.build_and(left, right, "and"),
                    SyntaxKind::Or => self.builder.build_or(left, right, "or"),
                    _ => {
                        self.builder
                            .build_int_compare(get_predicate(operator), left, right, "cmp")
                    }
                }),
                &left_.ty,
            ),

            (BasicValueEnum::FloatValue(left), BasicValueEnum::FloatValue(right)) => LLValue::cpy(
                match operator {
                    SyntaxKind::Plus => self.builder.build_float_add(left, right, "add").into(),
                    SyntaxKind::Minus => self.builder.build_float_sub(left, right, "sub").into(),
                    SyntaxKind::Star => self.builder.build_float_mul(left, right, "mul").into(),
                    SyntaxKind::Slash => self.builder.build_float_div(left, right, "div").into(),
                    _ => self
                        .builder
                        .build_float_compare(get_float_predicate(operator), left, right, "cmp")
                        .into(),
                },
                &left_.ty,
            ),

            // One of the operators is `Any`, so it will branch away; return whatever
            _ => LLValue::cpy(
                self.context.bool_type().const_int(0, false).into(),
                &IRType::Other,
            ),
        }
    }

    fn binary_is(&mut self, left: LLValue, right: &Type) -> LLValue {
        let ty_info_ptr = self.ir_ty_info(right).unwrap();
        let left_ptr = self.get_type_info_field(&left.ptr());
        let left_ptr = self.load_ptr(&LLPtr::cpy(left_ptr, &left.ty));

        let left_int = self.builder.build_ptr_to_int(
            left_ptr.into_pointer_value(),
            self.context.i64_type(),
            "conv",
        );
        let right_int = self
            .builder
            .build_ptr_to_int(ty_info_ptr, self.context.i64_type(), "conv");
        LLValue::cpy(
            self.builder
                .build_int_compare(IntPredicate::EQ, left_int, right_int, "ident")
                .into(),
            &IRType::Other,
        )
    }

    fn build_call(&mut self, callee: PointerValue, ret_type: Type, arguments: &[Expr]) -> LLValue {
        let (ir_args, arg_tys): (Vec<_>, Vec<_>) = arguments
            .iter()
            .map(|a| {
                let arg = self.expression(a);
                self.increment_refcount(&arg);
                (*arg, arg.ty)
            })
            .unzip();

        let callee = self.callee_ir(callee);
        let ret = self
            .builder
            .build_call(callee, &ir_args, "call")
            .try_as_basic_value();
        let ret = ret.left().unwrap_or(*self.none_const);
        if ret.is_pointer_value() {
            self.locals()
                .push(LLPtr::from(ret.into_pointer_value(), &ret_type));
        }

        for (arg, ty) in ir_args.iter().zip(arg_tys.iter()) {
            self.decrement_refcount(&LLValue::cpy(*arg, ty));
        }

        LLValue::from(ret, &ret_type)
    }

    fn callee_ir(&mut self, callee: PointerValue) -> PointerValue {
        match callee.get_type().get_element_type() {
            // Function
            AnyTypeEnum::FunctionType(_) => callee,
            // Closure
            AnyTypeEnum::StructType(_) => self
                .builder
                .build_load(self.struct_gep_raw(callee, 1), "clsfnload")
                .into_pointer_value(),
            _ => panic!("Can't call this!"),
        }
    }

    fn literal(&mut self, literal: &Literal) -> LLValue {
        let ty = literal.get_type();
        LLValue::from(
            match literal {
                Literal::Any | Literal::None => *self.none_const,
                Literal::Bool(value) => self
                    .context
                    .bool_type()
                    .const_int(*value as u64, false)
                    .into(),

                Literal::I8(num) | Literal::U8(num) => {
                    self.context.i8_type().const_int(*num as u64, false).into()
                }
                Literal::I16(num) | Literal::U16(num) => {
                    self.context.i16_type().const_int(*num as u64, false).into()
                }
                Literal::I32(num) | Literal::U32(num) => {
                    self.context.i32_type().const_int(*num as u64, false).into()
                }
                Literal::I64(num) | Literal::U64(num) => {
                    self.context.i64_type().const_int(*num as u64, false).into()
                }

                Literal::F32(num) => self.context.f32_type().const_float((*num).into()).into(),
                Literal::F64(num) => self.context.f64_type().const_float(*num).into(),

                Literal::String {
                    text: string,
                    ty: string_ty,
                } => {
                    let const_str = self.builder.build_global_string_ptr(&string, "str");
                    let constructor = Rc::clone(&string_ty.as_adt().ty.borrow().constructors[1]);

                    return self.allocate_raw_args(
                        &string_ty,
                        &constructor,
                        vec![
                            LLValue::cpy(
                                self.context
                                    .i64_type()
                                    .const_int((string.len() + 1) as u64, false)
                                    .into(),
                                &IRType::Other,
                            ),
                            LLValue::cpy(
                                self.context.i64_type().const_int(0, false).into(),
                                &IRType::Other,
                            ),
                            LLValue::cpy(const_str.as_pointer_value().into(), &IRType::Other),
                        ],
                    );
                }
            },
            &ty,
        )
    }

    fn if_(&mut self, cond: &Expr, then: &Expr, else_: &Expr, phi: bool) -> LLValue {
        let cond = self.expression(cond);
        let then_bb = self.append_block("then");
        let else_bb = self.append_block("else");
        let cont_bb = self.append_block("cont");

        self.builder
            .build_conditional_branch(cond.into_int_value(), &then_bb, &else_bb);

        let mut build_block = |expr: &Expr, block: BasicBlock| {
            self.position_at_block(block);
            self.push_local_scope();
            let val = self.expression(expr);
            let bb = self.last_block();
            if phi {
                self.pop_locals_remove(&val);
            } else {
                self.pop_dec_locals()
            }
            self.builder.build_unconditional_branch(&cont_bb);
            (val, bb)
        };

        let (then_val, then_bb) = build_block(then, then_bb);
        let (else_val, else_bb) = build_block(else_, else_bb);

        self.position_at_block(cont_bb);
        if phi {
            LLValue::from(
                self.build_phi(&[(then_val, then_bb), (else_val, else_bb)]),
                &then.get_type(),
            )
        } else {
            self.none_const.clone()
        }
    }

    fn switch(&mut self, cases: &[(Expr, Expr)], else_: &Expr, phi: bool) -> LLValue {
        let cond = self.context.bool_type().const_int(1, false);
        let end_bb = self.append_block("when-end");

        let mut phi_nodes = Vec::with_capacity(cases.len());
        let mut next_bb = self.append_block("when-case-false");
        for (br_cond, branch) in cases {
            let case_bb = self.append_block("when-case");

            self.push_local_scope();
            let br_cond = self.expression(br_cond).into_int_value();
            self.pop_dec_locals();
            let cmp = self
                .builder
                .build_int_compare(IntPredicate::EQ, cond, br_cond, "when-cmp");
            self.builder
                .build_conditional_branch(cmp, &case_bb, &next_bb);

            self.position_at_block(case_bb);
            self.push_local_scope();
            let value = self.expression(branch);
            if phi {
                self.pop_locals_remove(&value)
            } else {
                self.pop_dec_locals()
            };
            phi_nodes.push((value, self.last_block()));
            self.builder.build_unconditional_branch(&end_bb);

            self.position_at_block(next_bb);
            next_bb = self.append_block("when-case-false");
        }
        // Next case is 'else', this BB is not needed
        next_bb.remove_from_function().unwrap();

        // If the last case falls though, do the else case
        self.push_local_scope();
        let else_val = self.expression(else_);
        if phi {
            self.pop_locals_remove(&else_val)
        } else {
            self.pop_dec_locals()
        };
        let else_end_bb = self.last_block();
        self.builder.build_unconditional_branch(&end_bb);
        phi_nodes.push((else_val, else_end_bb));

        self.position_at_block(end_bb);
        if phi {
            LLValue::from(self.build_phi(&phi_nodes), &else_.get_type())
        } else {
            self.none_const.clone()
        }
    }

    fn loop_(
        &mut self,
        condition: &Expr,
        body: &Expr,
        else_: &Expr,
        phi_type: &Option<Type>,
    ) -> LLValue {
        let loop_bb = self.append_block("for-loop");
        let else_bb = self.append_block("for-else");
        let cont_bb = self.append_block("for-cont");

        let prev_loop = std::mem::replace(
            &mut self.loop_data,
            Some(LoopData {
                end_block: cont_bb,
                phi_nodes: if phi_type.is_some() {
                    Some(vec![])
                } else {
                    None
                },
            }),
        );

        let result_store = phi_type.as_ref().map(|ty| {
            let alloc_ty = self.ir_ty_allocs(ty);
            LLPtr::from(
                self.builder.build_alloca(alloc_ty, "loop-result-store"),
                &ty.clone(),
            )
        });

        let cond = self.expression(condition).into_int_value();
        self.builder
            .build_conditional_branch(cond, &loop_bb, &else_bb);

        self.position_at_block(loop_bb);
        self.push_local_scope();
        let body = self.expression(body);
        let loop_end_bb = self.last_block();
        if let Some(result_store) = &result_store {
            self.build_store(result_store, &body, false);
        }
        self.pop_dec_locals();
        let cond = self.expression(condition).into_int_value();
        let phi_node = if let Some(result_store) = &result_store {
            Some(self.load_ptr(result_store))
        } else {
            None
        };
        self.builder
            .build_conditional_branch(cond, &loop_bb, &cont_bb);

        self.position_at_block(else_bb);
        self.push_local_scope();
        let else_val = self.expression(else_);
        let else_bb = self.last_block();
        self.pop_dec_locals();
        self.builder.build_unconditional_branch(&cont_bb);

        self.position_at_block(cont_bb);
        let loop_data = mem::replace(&mut self.loop_data, prev_loop).unwrap();
        if let Some(result_store) = result_store {
            let mut phi_nodes = loop_data.phi_nodes.unwrap();
            phi_nodes.push((phi_node.unwrap(), loop_end_bb));
            phi_nodes.push((else_val, else_bb));
            let phi_nodes: Vec<_> = phi_nodes.iter().map(|n| (n.0.clone(), n.1)).collect();
            LLValue::cpy(self.build_phi(&phi_nodes), &result_store.ty)
        } else {
            self.none_const.clone()
        }
    }

    fn cast(&mut self, object: &Expr, to: &Type, method: &CastType) -> LLValue {
        match method {
            CastType::ToInterface(implementor) => self.cast_to_interface(object, implementor, to),

            CastType::Bitcast => {
                let obj = self.expression(object);
                let cast_ty = self.ir_ty_generic(to);
                LLValue::from(self.builder.build_bitcast(*obj, cast_ty, "cast"), to)
            }

            CastType::Number => {
                let obj = self.expression(object);
                let cast_ty = self.ir_ty_generic(to);

                LLValue::from(
                    match (obj.get_type(), cast_ty, to.is_signed_int()) {
                        (BasicTypeEnum::IntType(_), BasicTypeEnum::IntType(ty), _) => self
                            .builder
                            .build_int_cast(obj.into_int_value(), ty, "cast")
                            .into(),
                        (BasicTypeEnum::FloatType(_), BasicTypeEnum::FloatType(ty), _) => self
                            .builder
                            .build_float_cast(obj.into_float_value(), ty, "cast")
                            .into(),
                        (BasicTypeEnum::FloatType(_), BasicTypeEnum::IntType(ty), true) => self
                            .builder
                            .build_float_to_signed_int(obj.into_float_value(), ty, "cast")
                            .into(),
                        (BasicTypeEnum::FloatType(_), BasicTypeEnum::IntType(ty), false) => self
                            .builder
                            .build_float_to_unsigned_int(obj.into_float_value(), ty, "cast")
                            .into(),

                        (BasicTypeEnum::IntType(_), BasicTypeEnum::FloatType(ty), true) => self
                            .builder
                            .build_signed_int_to_float(obj.into_int_value(), ty, "cast")
                            .into(),
                        (BasicTypeEnum::IntType(_), BasicTypeEnum::FloatType(ty), false) => self
                            .builder
                            .build_unsigned_int_to_float(obj.into_int_value(), ty, "cast")
                            .into(),

                        _ => panic!(),
                    },
                    to,
                )
            }

            CastType::ToValue => {
                let ptr = self.expression(object).into_pointer_value();
                LLValue::from(self.builder.build_load(ptr, "vload"), to)
            }

            CastType::ToNullable => {
                let ty = self.expression(object);
                LLValue::from(*ty, to)
            }

            CastType::FromNullable => {
                let ty = self.expression(object);
                LLValue::from(*ty, to)
            }
        }
    }

    fn unary(&mut self, right: &Expr, operator: SyntaxKind) -> LLValue {
        let expr = self.expression(right);
        LLValue::cpy(
            match *expr {
                BasicValueEnum::IntValue(int) => match operator {
                    SyntaxKind::Bang => self.builder.build_not(int, "unarynot"),
                    SyntaxKind::Minus => self.builder.build_int_neg(int, "unaryneg"),
                    _ => panic!("Invalid unary operator"),
                }
                .into(),

                BasicValueEnum::FloatValue(float) => {
                    self.builder.build_float_neg(float, "unaryneg").into()
                }

                _ => panic!("Invalid unary operator"),
            },
            &expr.ty,
        )
    }

    fn cast_to_interface(&mut self, object: &Expr, implementor: &Type, to: &Type) -> LLValue {
        let obj = self.expression(object);
        let iface_ty = self.ir_ty_generic(to).into_struct_type();
        let vtable_ty = iface_ty.get_field_types()[1]
            .as_pointer_type()
            .get_element_type()
            .into_struct_type();

        let vtable = self.get_vtable(implementor, to, vtable_ty);
        let store = LLPtr::from(self.create_alloc(to.clone(), iface_ty.into(), false), &to);
        self.write_struct(&store, &[self.coerce_to_void_ptr(*obj), vtable]);
        LLValue::from(self.builder.build_load(*store, "ifaceload"), to)
    }

    /// Returns the vtable of the interface implementor given.
    /// Will generate functions as needed to fill the vtable.
    fn get_vtable(
        &mut self,
        implementor: &Type,
        iface: &Type,
        vtable: StructType,
    ) -> BasicValueEnum {
        let field_tys = vtable.get_field_types();
        let mut field_tys = field_tys.iter();
        let impls = Rc::clone(self.gir_data.iface_impls.get(implementor).unwrap());
        let impls = impls.borrow();
        // todo order
        let methods_iter = Some(self.get_free_function(&implementor))
            .into_iter()
            .chain(
                impls.interfaces[&iface]
                    .methods
                    .iter()
                    .map(|(_, method)| self.get_or_create(&Instance::new_(Rc::clone(method)))) // todo tyargs
                    .map(|f| f.as_global_value().as_pointer_value()),
            );
        let methods = methods_iter.collect::<Vec<_>>();
        let methods = methods
            .into_iter()
            .map(|func| {
                self.builder.build_bitcast(
                    func,
                    *field_tys.next().unwrap().as_pointer_type(),
                    "funccast",
                )
            })
            .collect::<Vec<_>>();
        let global = self.module.add_global(vtable, None, "vtable");
        global.set_initializer(&vtable.const_named_struct(&methods));
        global.as_pointer_value().into()
    }

    fn get_free_function(&mut self, ty: &Type) -> PointerValue {
        match ty {
            Type::Adt(adt) => self
                .get_or_create(&adt.get_method("free-instance"))
                .as_global_value()
                .as_pointer_value(),
            _ => self.void_ptr().const_zero(),
        }
    }

    pub(crate) fn intrinsic(&mut self, int: &Intrinsic) -> LLValue {
        match int {
            Intrinsic::IncRc(val) => {
                let val = self.expression(val);
                self.increment_refcount(&val)
            }

            Intrinsic::DecRc(val) => {
                let val = self.expression(val);
                self.decrement_refcount(&val)
            }

            Intrinsic::Free(val) => {
                let val = self.expression(val);
                self.builder.build_free(val.into_pointer_value());
            }

            Intrinsic::IfaceCall {
                iface,
                index,
                arguments,
                ret_type,
            } => {
                let callee = self.expression(iface).into_struct_value();
                let vtable = self
                    .builder
                    .build_extract_value(callee, 1, "vtable")
                    .unwrap();
                let ptr = self.struct_gep_raw(vtable.into_pointer_value(), *index as u32);
                let func = self.builder.build_load(ptr, "fnload").into_pointer_value();

                let first_arg = self.builder.build_extract_value(callee, 0, "implementor");
                let args = first_arg
                    .into_iter()
                    .chain(arguments.iter().map(|e| *self.expression(e)))
                    .collect::<Vec<_>>();

                return LLValue::from(
                    self.builder
                        .build_call(func, &args, "vcall")
                        .try_as_basic_value()
                        .left()
                        .unwrap_or(*self.none_const),
                    ret_type,
                );
            }
        }
        self.none_const.clone()
    }
}

fn get_predicate(tok: SyntaxKind) -> IntPredicate {
    match tok {
        SyntaxKind::Greater => IntPredicate::SGT,
        SyntaxKind::GreaterEqual => IntPredicate::SGE,
        SyntaxKind::Less => IntPredicate::SLT,
        SyntaxKind::LessEqual => IntPredicate::SLE,
        SyntaxKind::EqualEqual => IntPredicate::EQ,
        SyntaxKind::BangEqual => IntPredicate::NE,
        _ => panic!("invalid tok"),
    }
}

fn get_float_predicate(tok: SyntaxKind) -> FloatPredicate {
    match tok {
        SyntaxKind::Greater => FloatPredicate::OGT,
        SyntaxKind::GreaterEqual => FloatPredicate::OGE,
        SyntaxKind::Less => FloatPredicate::OLT,
        SyntaxKind::LessEqual => FloatPredicate::OLE,
        SyntaxKind::EqualEqual => FloatPredicate::OEQ,
        SyntaxKind::BangEqual => FloatPredicate::ONE,
        _ => panic!("invalid tok"),
    }
}
