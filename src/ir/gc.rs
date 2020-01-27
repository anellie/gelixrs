/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 1/27/20 6:22 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::ir::IRGenerator;
use inkwell::{
    values::{BasicValueEnum, PointerValue},
};
use inkwell::types::{AnyTypeEnum, BasicType};
use inkwell::AddressSpace::Generic;

impl IRGenerator {
    pub fn increment_refcount(&self, value: BasicValueEnum) {
        if value.get_type() == self.none_const.get_type().ptr_type(Generic).into() { return }
        if let Some(ptr) = self.get_struct_ptr(value) {
            self.mod_refcount(ptr, false, false)
        }
    }

    pub fn decrement_refcount(&self, value: BasicValueEnum) {
        if value.get_type() == self.none_const.get_type().ptr_type(Generic).into() { return }
        if let Some(ptr) = self.get_struct_ptr(value) {
            self.mod_refcount(ptr, true, true)
        }
    }

    fn get_struct_ptr(&self, value: BasicValueEnum) -> Option<PointerValue> {
        match value {
            BasicValueEnum::PointerValue(ptr) => match ptr.get_type().get_element_type() {
                AnyTypeEnum::PointerType(_) => Some(self.load_ptr(ptr).into_pointer_value()),
                AnyTypeEnum::StructType(_) => Some(ptr),
                _ => None,
            },

            _ => None,
        }
    }

    fn mod_refcount(&self, ptr: PointerValue, decrement: bool, maybe_dealloc: bool) {
        let gep = unsafe { self.builder.build_struct_gep(ptr, 0, "rcgep") };
        let rc = self.builder.build_load(gep, "rcload").into_int_value();
        let added = self.context.i32_type().const_int(1, false).into();
        let new_rc = if decrement {
            self.builder.build_int_sub(rc, added, "rcdec")
        } else {
            self.builder.build_int_add(rc, added, "rcinc")
        };
        self.builder.build_store(gep, new_rc);

        if maybe_dealloc {
            let func = self.module.get_function("gelixrs_check_dealloc");
            // FIXME: `Drop` impls need to be called
            self.builder.build_call(
                func.unwrap(),
                &[
                    self.builder
                        .build_ptr_to_int(ptr, self.context.i64_type(), "pti").into(),
                    self.context.bool_type().const_int(0, false).into(),
                    self.context.i64_type().const_int(0, false).into(),
                ],
                "dealloc-check",
            );
        }
    }
}
