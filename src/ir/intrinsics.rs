/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/31/19 8:38 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::{
    ir::{IRGenerator, PtrEqRc},
    mir::{nodes::Function, MModule, MutRc},
};
use inkwell::{types::{BasicType, BasicTypeEnum}, values::FunctionValue, AddressSpace::Generic, IntPredicate};
use std::cell::RefMut;

impl IRGenerator {
    pub(super) fn fill_intrinsic_functions(&mut self, module: &MutRc<MModule>) {
        for global in module.borrow().globals.values() {
            let ir = self.functions[&PtrEqRc::new(global)];
            let func = global.type_.as_function();
            self.fill_intrinsic(func.borrow_mut(), ir)
        }
    }

    fn fill_intrinsic(&mut self, func: RefMut<Function>, ir: FunctionValue) {
        // Trim the module name and generic parameters off the function
        // "std/mod:func_name<A, B>" -> "func_name"
        let name = &func
            .name
            .rsplit(':')
            .next()
            .unwrap()
            .split('<')
            .next()
            .unwrap();
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
                let mir_ty = func.context.type_aliases.values().next().unwrap();
                let typ = self.ir_ty(&mir_ty).ptr_type(Generic);
                let ptr_size = unsafe {
                    self.builder
                        .build_gep(typ.const_null(), &[i.const_int(1, false)], "size")
                };
                self.builder
                    .build_return(Some(&self.builder.build_ptr_to_int(ptr_size, i, "sizeI")));
            }

            // Take a pointer and return the value behind it.
            // ADTs are already pointers, so they are no-op.
            "deref_ptr" => {
                let ret_ty = ir.get_type().get_return_type().unwrap();
                let int = ir.get_first_param().unwrap().into_int_value();

                let ret_val = if let BasicTypeEnum::PointerType(ret_ty) = ret_ty {
                    self.builder.build_int_to_ptr(int, ret_ty, "ptr").into()
                } else {
                    let ptr = self
                        .builder
                        .build_int_to_ptr(int, ret_ty.ptr_type(Generic), "ptr");
                    self.builder.build_load(ptr, "load-ptr")
                };
                self.builder.build_return(Some(&ret_val));
            }

            // Dereference a pointer and set the value at its location
            // to the given value.
            "set_ptr" => {
                let value = ir.get_last_param().unwrap();
                let ty = value.get_type();
                let int = ir.get_first_param().unwrap().into_int_value();

                if let BasicTypeEnum::PointerType(ty) = ty {
                    let ptr = self.builder.build_int_to_ptr(int, ty, "ptr");
                    let value = self
                        .builder
                        .build_load(value.into_pointer_value(), "load-value");
                    self.builder.build_store(ptr, value);
                } else {
                    let ptr = self
                        .builder
                        .build_int_to_ptr(int, ty.ptr_type(Generic), "ptr");
                    self.builder.build_store(ptr, value);
                }
                self.builder.build_return(None);
            }

            // Will deallocate a struct pointer using free(), if the pointer's
            // refcount field (index 0) is 0.
            // See docs in the gelix code for more info.
            "check_dealloc" => {
                let params = ir.get_params();
                let struct_int = params[0];
                let should_drop = params[1];
                let drop_ptr = params[2];

                let ptr_ty = self.context.struct_type(&[self.context.i32_type().into()], false).ptr_type(Generic);
                let struct_ptr = self.builder.build_int_to_ptr(struct_int.into_int_value(), ptr_ty, "toptr");

                let is_dead_bb = self.context.append_basic_block(&ir, "isdead");
                let drop_bb = self.context.append_basic_block(&ir, "drop");
                let cont_bb = self.context.append_basic_block(&ir, "cont");
                let end_bb = self.context.append_basic_block(&ir, "end");

                let value = unsafe { self.builder.build_struct_gep(struct_ptr, 0, "rcgep") };
                let value = self.builder.build_load(value, "rcload");
                let value_is_0 = self.builder.build_int_compare(IntPredicate::EQ, value.into_int_value(), self.context.i32_type().const_int(0, false),
                                                                "rccond");
                self.builder.build_conditional_branch(value_is_0, &is_dead_bb, &end_bb);

                self.builder.position_at_end(&is_dead_bb);
                self.builder.build_conditional_branch(should_drop.into_int_value(), &drop_bb, &cont_bb);
                self.builder.position_at_end(&drop_bb);
                let ptr_ty = self.context.void_type().fn_type(&[self.context.i64_type().into()], false).ptr_type(Generic);
                let drop_ptr = self.builder.build_int_to_ptr(drop_ptr.into_int_value(), ptr_ty, "dropptr");
                self.builder.build_call(drop_ptr, &[struct_int], "dropcall");
                self.builder.build_unconditional_branch(&cont_bb);

                self.builder.position_at_end(&cont_bb);
                self.builder.build_free(struct_ptr);
                self.builder.build_unconditional_branch(&end_bb);

                self.builder.position_at_end(&end_bb);
                self.builder.build_return(None);
            }

            _ => panic!("Unknown intrinsic function: {}", name),
        }
    }
}
