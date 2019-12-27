/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 4:40 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::ir::{IRGenerator, PtrEqRc};
use crate::mir::nodes::Function;
use crate::mir::{MModule, MutRc};
use inkwell::types::BasicType;
use inkwell::types::BasicTypeEnum;
use inkwell::values::FunctionValue;
use inkwell::AddressSpace::Generic;
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

            _ => panic!("Unknown intrinsic function: {}", name),
        }
    }
}
