/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 1/27/20 6:22 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::ir::IRGenerator;
use inkwell::{
    types::{AnyTypeEnum, BasicType, StructType},
    values::{BasicValueEnum, IntValue, PointerValue, StructValue},
    AddressSpace::Generic,
    IntPredicate,
};

impl IRGenerator {
    pub fn build_store(&self, ptr: PointerValue, value: BasicValueEnum, is_null: bool) {
        if !is_null {
            self.decrement_refcount(ptr.into());
        }
        self.increment_refcount(value);
        self.builder.build_store(ptr, value);
    }

    pub fn increment_refcount(&self, value: BasicValueEnum) {
        if let Some(val) = self.get_rc_value(value) {
            self.mod_refcount(val, false)
        }
    }

    pub fn decrement_refcount(&self, value: BasicValueEnum) {
        if let Some(val) = self.get_rc_value(value) {
            self.mod_refcount(val, true)
        }
    }

    fn get_rc_value(&self, value: BasicValueEnum) -> Option<BasicValueEnum> {
        if value.get_type() == self.none_const.get_type().ptr_type(Generic).into() {
            return None;
        }

        match value {
            BasicValueEnum::PointerValue(ptr) => match ptr.get_type().get_element_type() {
                AnyTypeEnum::PointerType(_) => self.get_rc_value(self.load_ptr(ptr)),
                AnyTypeEnum::StructType(struc) if Self::needs_gc(struc) => Some(self.load_ptr(ptr)),
                _ => None,
            },

            BasicValueEnum::StructValue(struc) if Self::needs_gc(struc.get_type()) => Some(value),

            _ => None,
        }
    }

    fn needs_gc(struc: StructType) -> bool {
        !struc
            .get_name()
            .map(|name| name.to_str().unwrap().starts_with("vtable"))
            .unwrap_or(true)
    }

    fn mod_refcount(&self, value: BasicValueEnum, decrement: bool) {
        match value {
            BasicValueEnum::StructValue(struc) => self.mod_refcount_iface(struc, decrement),
            BasicValueEnum::PointerValue(ptr) => self.mod_refcount_class(ptr, decrement),
            _ => panic!("Cannot mod refcount on this"),
        }
    }

    fn mod_refcount_class(&self, ptr: PointerValue, decrement: bool) {
        let ty = ptr.get_type().get_element_type().into_struct_type();
        let ty = self
            .types_bw
            .get(ty.get_name().unwrap().to_str().unwrap())
            .unwrap();
        let func = self.get_variable(&ty.as_class().borrow().destructor);

        let refcount = unsafe { self.builder.build_struct_gep(ptr, 0, "rcgep") };
        let refcount = self.write_new_refcount(refcount, decrement);
        if decrement {
            self.build_maybe_free(refcount, &mut |this, pred| {
                this.builder
                    .build_call(func, &[ptr.into(), pred.into()], "free");
            })
        }
    }

    fn mod_refcount_iface(&self, struc: StructValue, decrement: bool) {
        let impl_ptr = self.builder.build_extract_value(struc, 0, "rcext").unwrap();
        let impl_ptr = self.builder.build_bitcast(
            impl_ptr,
            self.context.i32_type().ptr_type(Generic),
            "rccst",
        );
        let refcount = self.write_new_refcount(impl_ptr.into_pointer_value(), decrement);

        if decrement {
            self.build_maybe_free(refcount, &mut |this, pred| {
                let vtable_ptr = this
                    .builder
                    .build_extract_value(struc, 1, "vtable")
                    .unwrap()
                    .into_pointer_value();

                let func = this
                    .load_ptr(this.struct_gep(vtable_ptr, 0))
                    .into_pointer_value();

                let implementor = this
                    .builder
                    .build_extract_value(struc, 0, "impl")
                    .unwrap()
                    .into_pointer_value();

                this.builder
                    .build_call(func, &[implementor.into(), pred.into()], "free");
            })
        }
    }

    fn write_new_refcount(&self, refcount: PointerValue, decrement: bool) -> IntValue {
        let rc = self.builder.build_load(refcount, "rcload").into_int_value();
        let added = self.context.i32_type().const_int(1, false).into();
        let new_rc = if decrement {
            self.builder.build_int_sub(rc, added, "rcdec")
        } else {
            self.builder.build_int_add(rc, added, "rcinc")
        };
        self.builder.build_store(refcount, new_rc);
        new_rc
    }

    /// Will insert a free check at the current insert position.
    /// free_closure should generate the code that runs when the value
    /// is to be freed.
    fn build_maybe_free(
        &self,
        refcount: IntValue,
        free_closure: &mut dyn FnMut(&IRGenerator, IntValue),
    ) {
        let value_is_0 = self.builder.build_int_compare(
            IntPredicate::EQ,
            refcount,
            self.context.i32_type().const_int(0, false),
            "rccond",
        );
        free_closure(self, value_is_0);
    }
}
