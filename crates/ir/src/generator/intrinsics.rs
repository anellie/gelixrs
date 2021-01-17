/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 2/3/20 1:58 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use common::MutRc;
use gir_nodes::{types::TypeArguments, Function, Module, Type};
use inkwell::{
    basic_block::BasicBlock,
    types::BasicType,
    values::{BasicValue, FunctionValue, PointerValue},
    AddressSpace::Generic,
    IntPredicate,
};
use std::rc::Rc;

use super::{type_adapter::LLValue, IRGenerator};

impl IRGenerator {
    pub(crate) fn fill_intrinsic_functions(&mut self, module: &MutRc<Module>) {
        for func in &module.borrow().functions {
            let func = func.borrow();
            for (ir, ty_args) in func.ir.borrow().into_iter() {
                self.fill_intrinsic(&func, ty_args, *ir)
            }
        }
    }

    fn fill_intrinsic(
        &mut self,
        func: &Function,
        ty_args: Option<&Rc<TypeArguments>>,
        ir: FunctionValue,
    ) {
        let name = &func.name;
        // All functions intended to be generated in IR have this prefix
        if name.len() < 8 || &name[..8] != "gelixrs_" {
            return;
        }

        self.prepare_function(&func, ir);

        // Strip the "gelixrs_" all IR functions start with
        match &name[8..] {
            // Returns the size of the type. Implemented by
            // indexing a null pointer (0x0) at element 1
            // and returning the offset from 0.
            // Credit: http://nondot.org/sabre/LLVMNotes/SizeOf-OffsetOf-VariableSizedStructs.txt
            "get_type_size" => {
                let i = self.context.i64_type();
                let typ = self.ir_ty_generic(&ty_args.unwrap()[0]).ptr_type(Generic);
                let ptr_size = unsafe {
                    self.builder
                        .build_gep(typ.const_null(), &[i.const_int(1, false)], "size")
                };
                self.builder
                    .build_return(Some(&self.builder.build_ptr_to_int(ptr_size, i, "sizeI")));
            }

            "ptr_to_int" => {
                self.builder.build_return(Some(
                    &self
                        .builder
                        .build_ptr_to_int(
                            ir.get_first_param().unwrap().into_pointer_value(),
                            self.context.i64_type(),
                            "cast",
                        )
                        .as_basic_value_enum(),
                ));
            }

            "int_to_ptr" => {
                self.builder.build_return(Some(
                    &self
                        .builder
                        .build_int_to_ptr(
                            ir.get_first_param().unwrap().into_int_value(),
                            ir.get_type().get_return_type().unwrap().into_pointer_type(),
                            "cast",
                        )
                        .as_basic_value_enum(),
                ));
            }

            "gep" => {
                let ptr = ir.get_first_param().unwrap().into_pointer_value();
                let index = ir.get_last_param().unwrap().into_int_value();
                self.builder.build_return(Some(unsafe {
                    &self
                        .builder
                        .build_gep(ptr, &[index], "gep")
                        .as_basic_value_enum()
                }));
            }

            // Write a value to a pointer.
            "write_ptr" => {
                let value = ir.get_last_param().unwrap();
                let pointer = ir.get_first_param().unwrap();
                self.builder
                    .build_store(pointer.into_pointer_value(), value);
                self.builder.build_return(None);
            }

            "inc_ref" => {
                let ll = LLValue::from(
                    ir.get_first_param().unwrap(),
                    &func.parameters.first().unwrap().ty,
                );
                self.increment_refcount(&ll);
                self.builder.build_return(None);
            }

            "dec_ref" => {
                let ll = LLValue::from(
                    ir.get_first_param().unwrap(),
                    &func.parameters.first().unwrap().ty,
                );
                self.decrement_refcount(&ll);
                self.builder.build_return(None);
            }

            "free_type" => {
                let value = ir.get_first_param().unwrap();
                let elem = &ty_args.unwrap()[0];
                match elem {
                    Type::Adt(adt) => {
                        let method = adt.get_method(&Rc::new("free-instance".to_string()));
                        let destructor = method
                            .ty
                            .borrow()
                            .ir
                            .borrow()
                            .get_inst(method.args())
                            .unwrap();
                        self.builder.build_call(destructor, &[value], "free");
                    }

                    // Primitive, simply calling free is enough since it must be a raw pointer
                    _ => {
                        self.builder.build_free(value.into_pointer_value());
                    }
                }

                self.builder.build_return(None);
            }

            "load_value" => {
                let value = ir.get_first_param().unwrap();
                self.builder.build_return(Some(
                    &self.builder.build_load(value.into_pointer_value(), "var"),
                ));
            }

            "inc_ref_iface" => {
                let (_, impl_ptr, _, end_bb) = self.iface_ref_method(ir);
                self.write_new_refcount(impl_ptr, false);

                self.builder.build_unconditional_branch(&end_bb);
                self.builder.position_at_end(&end_bb);
                self.builder.build_return(None);
            }

            "dec_ref_iface" => {
                let (func, impl_ptr, impl_ptr_i64, end_bb) = self.iface_ref_method(ir);
                let new_rc = self.write_new_refcount(impl_ptr, false);

                let rc_is_0 = self.builder.build_int_compare(
                    IntPredicate::EQ,
                    new_rc,
                    self.context.i32_type().const_int(0, false),
                    "rc_is_0",
                );
                self.builder
                    .build_call(func, &[impl_ptr_i64.into(), rc_is_0.into()], "free_call");
                self.builder.build_unconditional_branch(&end_bb);

                self.builder.position_at_end(&end_bb);
                self.builder.build_return(None);
            }

            _ => panic!("Unknown intrinsic function: {}", name),
        }
    }

    fn iface_ref_method(
        &mut self,
        ir: FunctionValue,
    ) -> (PointerValue, PointerValue, PointerValue, BasicBlock) {
        let vtable_ptr = self.builder.build_int_to_ptr(
            ir.get_last_param().unwrap().into_int_value(),
            self.context
                .struct_type(
                    &[self
                        .context
                        .void_type()
                        .fn_type(
                            &[
                                self.context.i64_type().ptr_type(Generic).into(),
                                self.context.bool_type().into(),
                            ],
                            false,
                        )
                        .ptr_type(Generic)
                        .into()],
                    false,
                )
                .ptr_type(Generic),
            "cast",
        );

        let func = self
            .builder
            .build_load(self.struct_gep_raw(vtable_ptr, 0), "vload")
            .into_pointer_value();

        let func_as_i64 =
            self.builder
                .build_ptr_to_int(func, self.context.i64_type(), "free_to_int");
        let impl_is_primitive = self.builder.build_int_compare(
            IntPredicate::EQ,
            func_as_i64,
            self.context.i64_type().const_int(0, false),
            "impl_is_primitve",
        );

        let cont_bb = ir.append_basic_block("do");
        let end_bb = ir.append_basic_block("end");
        self.builder
            .build_conditional_branch(impl_is_primitive, &end_bb, &cont_bb);

        self.builder.position_at_end(&cont_bb);
        let impl_ptr_i64 = self.builder.build_int_to_ptr(
            ir.get_first_param().unwrap().into_int_value(),
            self.context.i64_type().ptr_type(Generic),
            "impl",
        );
        let impl_ptr = self
            .builder
            .build_bitcast(
                impl_ptr_i64,
                self.context.i32_type().ptr_type(Generic),
                "impl_to_i32ptr",
            )
            .into_pointer_value();

        (func, impl_ptr, impl_ptr_i64, end_bb)
    }
}
