use std::rc::Rc;

use common::MutRc;
use gir_nodes::{
    declaration::Variable,
    expression::{CastType, Intrinsic},
    Expr, Function, Instance, Literal, Type, ADT,
};
use inkwell::{
    basic_block::BasicBlock,
    types::{BasicTypeEnum, StructType},
    values::{BasicValueEnum, IntValue, PointerValue},
    FloatPredicate, IntPredicate,
};
use std::mem;
use syntax::kind::SyntaxKind;

use super::{IRGenerator, LoopData};

impl IRGenerator {
    pub(crate) fn expression(&mut self, expr: &Expr) -> BasicValueEnum {
        self.expression_(expr, false)
    }

    pub(crate) fn expression_(&mut self, expr: &Expr, no_load: bool) -> BasicValueEnum {
        if self.builder.get_insert_block().is_none() {
            return self.none_const;
        }

        match expr {
            Expr::Block(block) => {
                self.push_local_scope();
                let ret = block
                    .iter()
                    .fold(self.none_const, |_, ex| self.expression(ex));
                self.pop_locals_lift(ret);
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
                Variable::Local(_) if no_load => self.get_variable(var).into(),
                Variable::Local(_) => self.load_ptr_gir(
                    self.get_variable(var),
                    &self.maybe_unwrap_var(&var.get_type()),
                    false,
                ),
                Variable::Function(func) => self
                    .get_or_create(func)
                    .as_global_value()
                    .as_pointer_value()
                    .into(),
            },

            Expr::Load { object, field } => {
                let obj = self.expression(object);
                let ptr = self.struct_gep(obj.into_pointer_value(), field.index);
                if no_load {
                    ptr.into()
                } else {
                    self.load_ptr_gir(ptr, &field.ty, false)
                }
            }

            Expr::Store {
                location,
                value,
                first_store,
            } => {
                let store = self.expression_(location, true);
                let value = self.expression(value);
                self.build_store(store.into_pointer_value(), value, *first_store);
                if *first_store && !location.is_struct_get() {
                    self.locals().push((store, true))
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
                    self.binary_is(left, &right.get_type_get_type()).into()
                } else {
                    let right = self.expression(right);
                    self.binary(left, *operator, right)
                }
            }

            Expr::Unary { operator, right } => self.unary(right, *operator),

            Expr::Call { callee, arguments } => {
                let callee = self.expression(callee);
                self.build_call(callee.into_pointer_value(), arguments.iter())
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
                self.none_const
            }

            Expr::Return(value) => {
                let value = self.expression(value);
                self.increment_refcount(value, false);
                self.decrement_all_locals();

                if value.get_type() == self.none_const.get_type() {
                    self.builder.build_return(None);
                } else {
                    self.builder.build_return(Some(&value));
                }

                self.builder.clear_insertion_position();
                self.none_const
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
    ) -> BasicValueEnum {
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
        constructor_args: Vec<BasicValueEnum>,
    ) -> BasicValueEnum {
        let (ir_ty, tyinfo) = self.ir_ty_raw(ty);
        let alloc = self.create_alloc(ir_ty, ty.is_strong_ref());

        let adt = ty.try_adt().unwrap();
        let constructor = self.get_or_create(&Instance::new(
            Rc::clone(constructor),
            Rc::clone(adt.args()),
        ));
        let instantiator = self
            .get_or_create(&adt.get_method("new-instance"))
            .as_global_value()
            .as_pointer_value();

        self.maybe_init_type_info(&adt.ty, alloc, tyinfo);
        self.build_alloc_and_init(
            alloc,
            ty,
            instantiator,
            constructor.as_global_value().as_pointer_value(),
            constructor_args,
        )
    }

    fn maybe_init_type_info(
        &mut self,
        ty: &MutRc<ADT>,
        alloc: PointerValue,
        info: Option<PointerValue>,
    ) {
        if ty.borrow().ty.ref_is_ptr() && !ty.borrow().ty.is_extern_class() {
            let gep = self.get_type_info_field(alloc);
            self.builder.build_store(gep, info.unwrap());
        }
    }

    fn build_alloc_and_init(
        &mut self,
        alloc: PointerValue,
        ty: &Type,
        instantiator: PointerValue,
        constructor: PointerValue,
        mut arguments: Vec<BasicValueEnum>,
    ) -> BasicValueEnum {
        let ptr = if let Type::StrongRef(ty) = ty {
            self.cast_sr_to_wr(alloc, &Type::WeakRef(ty.clone()))
                .into_pointer_value()
        } else {
            alloc
        };

        self.increment_refcount(alloc.into(), true);
        self.builder.build_call(instantiator, &[ptr.into()], "inst");

        for arg in &arguments {
            self.increment_refcount(*arg, false);
        }
        arguments.insert(0, ptr.into());
        self.builder.build_call(constructor, &arguments, "constr");
        for arg in arguments.iter().skip(1) {
            self.decrement_refcount(*arg, false);
        }

        self.locals().push((alloc.into(), true));
        alloc.into()
    }

    fn binary(
        &self,
        left: BasicValueEnum,
        operator: SyntaxKind,
        right: BasicValueEnum,
    ) -> BasicValueEnum {
        match (left, right) {
            (BasicValueEnum::IntValue(left), BasicValueEnum::IntValue(right)) => {
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
                })
            }

            (BasicValueEnum::FloatValue(left), BasicValueEnum::FloatValue(right)) => {
                BasicValueEnum::FloatValue(match operator {
                    SyntaxKind::Plus => self.builder.build_float_add(left, right, "add"),
                    SyntaxKind::Minus => self.builder.build_float_sub(left, right, "sub"),
                    SyntaxKind::Star => self.builder.build_float_mul(left, right, "mul"),
                    SyntaxKind::Slash => self.builder.build_float_div(left, right, "div"),
                    _ => {
                        return BasicValueEnum::IntValue(self.builder.build_float_compare(
                            get_float_predicate(operator),
                            left,
                            right,
                            "cmp",
                        ))
                    }
                })
            }

            // One of the operators is `Any`, so it will branch away; return whatever
            _ => self.context.bool_type().const_int(0, false).into(),
        }
    }

    fn binary_is(&mut self, left: BasicValueEnum, right: &Type) -> IntValue {
        let ty_info_ptr = self.ir_ty_info(right).unwrap();
        let left_ptr = self.get_type_info_field(left.into_pointer_value());
        let left_ptr = self.load_ptr(left_ptr).into_pointer_value();

        let left_int = self
            .builder
            .build_ptr_to_int(left_ptr, self.context.i64_type(), "conv");
        let right_int = self
            .builder
            .build_ptr_to_int(ty_info_ptr, self.context.i64_type(), "conv");
        self.builder
            .build_int_compare(IntPredicate::EQ, left_int, right_int, "ident")
    }

    fn build_call<'a, T: Iterator<Item = &'a Expr>>(
        &mut self,
        ptr: PointerValue,
        arguments: T,
    ) -> BasicValueEnum {
        let arguments: Vec<_> = arguments.map(|a| self.expression(a)).collect();

        for arg in &arguments {
            self.increment_refcount(*arg, false);
        }

        let ret = self
            .builder
            .build_call(ptr, &arguments, "call")
            .try_as_basic_value();
        let ret = ret.left().unwrap_or(self.none_const);
        self.locals().push((ret, false));

        for arg in &arguments {
            self.decrement_refcount(*arg, false);
        }

        ret
    }

    fn literal(&mut self, literal: &Literal) -> BasicValueEnum {
        match literal {
            Literal::Any | Literal::None => self.none_const,
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

            Literal::String { text: string, ty: string_ty } => {
                let const_str = self.builder.build_global_string_ptr(&string, "str");
                let constructor = Rc::clone(&string_ty.as_strong_ref().ty.borrow().constructors[1]);

                self.allocate_raw_args(
                    &string_ty,
                    &constructor,
                    vec![
                        self.context
                            .i64_type()
                            .const_int((string.len() + 1) as u64, false)
                            .into(),
                        self.context.i64_type().const_int(0, false).into(),
                        const_str.as_pointer_value().into(),
                    ],
                )
            }
        }
    }

    fn if_(&mut self, cond: &Expr, then: &Expr, else_: &Expr, phi: bool) -> BasicValueEnum {
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
                self.pop_locals_remove(val);
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
            self.build_phi(&[(then_val, then_bb), (else_val, else_bb)])
        } else {
            self.none_const
        }
    }

    fn switch(&mut self, cases: &[(Expr, Expr)], else_: &Expr, phi: bool) -> BasicValueEnum {
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
                self.pop_locals_remove(value)
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
            self.pop_locals_remove(else_val)
        } else {
            self.pop_dec_locals()
        };
        let else_end_bb = self.last_block();
        self.builder.build_unconditional_branch(&end_bb);
        phi_nodes.push((else_val, else_end_bb));

        self.position_at_block(end_bb);
        if phi {
            self.build_phi(&phi_nodes)
        } else {
            self.none_const
        }
    }

    fn loop_(
        &mut self,
        condition: &Expr,
        body: &Expr,
        else_: &Expr,
        phi_type: &Option<Type>,
    ) -> BasicValueEnum {
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
            self.builder.build_alloca(alloc_ty, "loop-result-store")
        });

        let cond = self.expression(condition).into_int_value();
        self.builder
            .build_conditional_branch(cond, &loop_bb, &else_bb);

        self.position_at_block(loop_bb);
        self.push_local_scope();
        let body = self.expression(body);
        let loop_end_bb = self.last_block();
        if let Some(result_store) = result_store {
            self.build_store(result_store, body, false);
        }
        self.pop_dec_locals();
        let cond = self.expression(condition).into_int_value();
        let phi_node = if let Some(result_store) = result_store {
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
        if result_store.is_some() {
            let mut phi_nodes = loop_data.phi_nodes.unwrap();
            phi_nodes.push((phi_node.unwrap(), loop_end_bb));
            phi_nodes.push((else_val, else_bb));
            self.build_phi(&phi_nodes)
        } else {
            self.none_const
        }
    }

    fn cast(&mut self, object: &Expr, to: &Type, method: &CastType) -> BasicValueEnum {
        match method {
            CastType::ToInterface(implementor) => self.cast_to_interface(object, implementor, to),

            CastType::Bitcast => {
                let obj = self.expression(object);
                let cast_ty = self.ir_ty_generic(to);
                self.builder.build_bitcast(obj, cast_ty, "cast")
            }

            CastType::Number => {
                let obj = self.expression(object);
                let cast_ty = self.ir_ty_generic(to);

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
                }
            }

            CastType::ToValue => {
                let ptr = self.expression(object).into_pointer_value();
                self.load_ptr_gir(ptr, to, true)
            }

            CastType::StrongToWeak => {
                let ptr = self.expression(object).into_pointer_value();
                self.cast_sr_to_wr(ptr, to)
            }
        }
    }

    fn unary(&mut self, right: &Expr, operator: SyntaxKind) -> BasicValueEnum {
        let expr = self.expression(right);
        match expr {
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
        }
    }

    pub(crate) fn cast_sr_to_wr(&mut self, sr: PointerValue, wr_ty: &Type) -> BasicValueEnum {
        if wr_ty.try_adt().unwrap().ty.borrow().ty.is_extern_class() {
            return sr.into();
        }

        let to = self.ir_ty_generic(wr_ty);
        let gep = unsafe { self.builder.build_struct_gep(sr, 1, "srwrgep") };
        self.builder.build_bitcast(gep, to, "wrcast")
    }

    fn cast_to_interface(
        &mut self,
        object: &Expr,
        implementor: &Type,
        to: &Type,
    ) -> BasicValueEnum {
        let obj = self.expression(object);
        let iface_ty = self.ir_ty_generic(to).into_struct_type();
        let vtable_ty = iface_ty.get_field_types()[1]
            .as_pointer_type()
            .get_element_type()
            .into_struct_type();

        let vtable = self.get_vtable(implementor, to, vtable_ty);
        let store = self.create_alloc(iface_ty.into(), false);
        self.write_struct(store, [self.coerce_to_void_ptr(obj), vtable].iter());
        self.builder.build_load(store, "ifaceload")
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
                impls.interfaces[&iface.to_strong()]
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
            Type::StrongRef(adt) => self
                .get_or_create(&adt.get_method("free-sr"))
                .as_global_value()
                .as_pointer_value(),
            Type::WeakRef(adt) => self
                .get_or_create(&adt.get_method("free-wr"))
                .as_global_value()
                .as_pointer_value(),
            _ => self.void_ptr().const_zero(),
        }
    }

    pub(crate) fn intrinsic(&mut self, int: &Intrinsic) -> BasicValueEnum {
        match int {
            Intrinsic::IncRc(val) => {
                let val = self.expression(val);
                self.increment_refcount(val, false)
            }

            Intrinsic::DecRc(val) => {
                let val = self.expression(val);
                self.decrement_refcount(val, false)
            }

            Intrinsic::Free(val) => {
                let val = self.expression(val);
                self.builder.build_free(val.into_pointer_value());
            }

            Intrinsic::IfaceCall {
                iface,
                index,
                arguments,
            } => {
                let callee = self.expression(iface).into_struct_value();
                let vtable = self
                    .builder
                    .build_extract_value(callee, 1, "vtable")
                    .unwrap();
                let func = unsafe {
                    let ptr = self.builder.build_struct_gep(
                        vtable.into_pointer_value(),
                        *index as u32,
                        "ifacefn",
                    );
                    self.builder.build_load(ptr, "fnload").into_pointer_value()
                };

                let first_arg = self.builder.build_extract_value(callee, 0, "implementor");
                let args = first_arg
                    .into_iter()
                    .chain(arguments.iter().map(|e| self.expression(e)))
                    .collect::<Vec<_>>();

                return self
                    .builder
                    .build_call(func, &args, "vcall")
                    .try_as_basic_value()
                    .left()
                    .unwrap_or(self.none_const);
            }
        }
        self.none_const
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
