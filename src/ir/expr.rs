use std::rc::Rc;

use crate::{gir::nodes::expression::Expr, ir::IRGenerator};
use inkwell::values::{BasicValueEnum, PointerValue};
use crate::ast::Literal;
use either::Either::Right;
use inkwell::types::{AnyTypeEnum, BasicTypeEnum, StructType};
use crate::gir::nodes::expression::CastType;
use crate::gir::{Type, get_or_create_iface_impls};
use crate::gir::nodes::declaration::Variable;

impl IRGenerator {
    pub fn expression(&mut self, expr: &Expr) -> BasicValueEnum {
        if self.builder.get_insert_block().is_none() {
            return self.none_const;
        }

        match expr {
            Expr::Block(block) => {
                self.push_local_scope();
                let ret = block.iter().fold(self.none_const, |_, ex| self.expression(ex));
                self.pop_locals_lift(ret);
                ret
            },

            Expr::Literal(literal, _) => self.literal(literal),

            Expr::Variable(var) => {
                match var {
                    Variable::Local(_) => self.load_ptr_mir(self.get_variable(var), &var.get_type()),
                    Variable::Function(func) => self.get_or_create(func).as_global_value().as_pointer_value().into(),
                }
            },

            Expr::Call { callee, arguments } => {
                let callee = self.expression(callee);
                self.build_call(callee.into_pointer_value(), arguments.iter())
            },

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
            },

            Expr::Cast { inner, to, method } => self.cast(inner, to, *method),

            _ => {
                dbg!(expr);
                todo!()
            }
            /*
            Expr::Allocate { .. } => {},
            Expr::Load { .. } => {},
            Expr::Store { .. } => {},
            Expr::Binary { .. } => {},
            Expr::Unary { .. } => {},
            Expr::If { .. } => {},
            Expr::Switch { .. } => {},
            Expr::Loop { .. } => {},
            Expr::Break(_) => {},
            Expr::Closure { .. } => {},
            Expr::TypeGet(_) => {},*/
        }
    }

    fn build_call<'a, T: Iterator<Item = &'a Expr>>(
        &mut self,
        ptr: PointerValue,
        arguments: T
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

            Literal::String(string) => {
                let const_str = self.builder.build_global_string_ptr(&string, "str");
                let string_builder = self
                    .module
                    .get_function("std/intrinsics::build_string_literal")
                    .unwrap();
                let st = self
                    .builder
                    .build_call(
                        string_builder,
                        &[
                            const_str.as_pointer_value().into(),
                            self.context
                                .i64_type()
                                .const_int((string.len() + 1) as u64, false)
                                .into(),
                        ],
                        "str",
                    )
                    .try_as_basic_value()
                    .left()
                    .unwrap();
                self.locals().push((st, false));
                st
            }

            /*Literal::Array(Right(literal)) => {
                let alloc = self.expression(&literal.alloc);
                let alloc_wr =
                    self.cast_sr_to_wr(alloc.into_pointer_value(), &literal.type_.to_weak());

                for value in &literal.values {
                    self.build_call(
                        self.get_variable(&literal.push_fn),
                        vec![value].into_iter(),
                        Some(alloc_wr),
                    );
                }
                alloc
            }*/

            _ => panic!("unknown literal"),
        }
    }

    fn cast(&mut self, object: &Expr, to: &Type, method: CastType) -> BasicValueEnum {
        match method {
            CastType::ToInterface => self.cast_to_interface(object, to),

            CastType::Bitcast => {
                let obj = self.expression(object);
                let cast_ty = self.ir_ty_generic(to);
                self.builder.build_bitcast(obj, cast_ty, "cast")
            }

            CastType::Number => {
                let obj = self.expression(object);
                let cast_ty = self.ir_ty_generic(to);

                match (obj.get_type(), cast_ty, to.is_signed_int()) {
                    (BasicTypeEnum::IntType(_), BasicTypeEnum::IntType(ty), _) => {
                        self.builder.build_int_cast(obj.into_int_value(), ty, "cast").into()
                    }
                    (BasicTypeEnum::FloatType(_), BasicTypeEnum::FloatType(ty), _) => {
                        self.builder.build_float_cast(obj.into_float_value(), ty, "cast").into()
                    }
                    (BasicTypeEnum::FloatType(_), BasicTypeEnum::IntType(ty), true) => self.builder.build_float_to_signed_int(
                        obj.into_float_value(),
                        ty,
                        "cast",
                    ).into(),
                    (BasicTypeEnum::FloatType(_),  BasicTypeEnum::IntType(ty), false) => self
                        .builder
                        .build_float_to_unsigned_int(obj.into_float_value(), ty, "cast").into(),

                    (BasicTypeEnum::IntType(_), BasicTypeEnum::FloatType(ty), true) => self.builder.build_signed_int_to_float(
                        obj.into_int_value(),
                        ty,
                        "cast",
                    ).into(),
                    (BasicTypeEnum::IntType(_), BasicTypeEnum::FloatType(ty), false) => self.builder.build_unsigned_int_to_float(
                        obj.into_int_value(),
                        ty,
                        "cast",
                    ).into(),

                    _ => panic!(),
                }
            }

            CastType::ToValue => {
                let ptr = self.expression(object).into_pointer_value();
                self.load_ptr_mir(ptr, to)
            }

            CastType::StrongToWeak => {
                let ptr = self.expression(object).into_pointer_value();
                self.cast_sr_to_wr(ptr, to)
            }
        }
    }

    pub fn cast_sr_to_wr(&mut self, sr: PointerValue, wr_ty: &Type) -> BasicValueEnum {
        if wr_ty.try_adt().unwrap().ty.borrow().ty.is_extern_class() {
            return sr.into();
        }

        let to = self.ir_ty_generic(wr_ty);
        let gep = unsafe { self.builder.build_struct_gep(sr, 1, "srwrgep") };
        self.builder.build_bitcast(gep, to, "wrcast")
    }

    fn cast_to_interface(&mut self, object: &Expr, to: &Type) -> BasicValueEnum {
        let obj = self.expression(object);
        let iface_ty = self.ir_ty_generic(to).into_struct_type();
        let vtable_ty = iface_ty.get_field_types()[1]
            .as_pointer_type()
            .get_element_type()
            .into_struct_type();

        let vtable = self.get_vtable(&object.get_type(), to, vtable_ty);
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
        let impls = get_or_create_iface_impls(&implementor.to_strong());
        let impls = impls.borrow();
        todo!();
        /*
        let methods_iter = self
            .get_free_function(&implementor)
            .into_iter()
            .chain(
                impls.interfaces[&iface.to_strong()]
                    .methods
                    .iter()
                    .map(|(_, method)| self.functions[&PtrEqRc::new(method)])
                    .map(|f| f.as_global_value().as_pointer_value()),
            )
            .map(|func| {
                self.builder.build_bitcast(
                    func,
                    *field_tys.next().unwrap().as_pointer_type(),
                    "funccast",
                )
            });
        let methods = methods_iter.collect::<Vec<_>>();
        let global = self.module.add_global(vtable, None, "vtable");
        global.set_initializer(&vtable.const_named_struct(&methods));
        global.as_pointer_value().into()
        */
    }

    fn get_free_function(&self, ty: &Type) -> Option<PointerValue> {
        Some(match ty {
            Type::StrongRef(adt) => {
                let method = adt.get_method(&Rc::new("free-sr".to_string()));
                let m_ty = method.ty.borrow();
                let ir = m_ty.ir.borrow();
                ir.get_inst(method.args())
                    .unwrap()
                    .as_global_value()
                    .as_pointer_value()
            }
            Type::WeakRef(adt) => {
                let method = adt.get_method(&Rc::new("free-wr".to_string()));
                let m_ty = method.ty.borrow();
                let ir = m_ty.ir.borrow();
                ir.get_inst(method.args())
                    .unwrap()
                    .as_global_value()
                    .as_pointer_value()
            }
            _ => self.void_ptr().const_zero(),
        })
    }
}
