/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 2/3/20 3:26 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use gir_nodes::{declaration::ADTType, Instance, ADT};
use inkwell::{
    values::{BasicValueEnum, IntValue, PointerValue, StructValue},
    IntPredicate,
};
use std::rc::Rc;

use super::{type_adapter::IRType, IRGenerator};
use crate::generator::type_adapter::{LLPtr, LLValue};

impl IRGenerator {
    /// Build a store; will do required refcount modification.
    pub(crate) fn build_store(&mut self, ptr: &LLPtr, value: &LLValue, is_null: bool) {
        let ptr_val = ptr.val();
        if !is_null {
            self.decrement_refcount(&ptr_val);
        }
        self.builder.build_store(**ptr, **value);
        self.increment_refcount(&ptr_val);
    }

    /// Increment the refcount of a value.
    /// is_ptr specifies if the value is a pointer or a value in
    /// the context of the MIR type system.
    pub(crate) fn increment_refcount(&mut self, _value: &LLValue) {
        // self.mod_refcount(value, false)
    }

    /// Decrement the refcount of a value, and check if it needs to be freed
    /// is_ptr specifies if the value is a pointer or a value in
    /// the context of the MIR type system.
    pub(crate) fn decrement_refcount(&mut self, _value: &LLValue) {
        // self.mod_refcount(value, true)
    }

    #[allow(dead_code)]
    fn mod_refcount(&mut self, value: &LLValue, decrement: bool) {
        match (**value, &value.ty) {
            (BasicValueEnum::StructValue(struc), IRType::ValueAdt(inst))
                if matches!(inst.ty.borrow().ty, ADTType::Interface) =>
            {
                self.mod_refcount_iface(struc, decrement)
            }

            (BasicValueEnum::PointerValue(ptr), IRType::ValueAdt(inst))
                if matches!(inst.ty.borrow().ty, ADTType::Interface) =>
            {
                self.mod_refcount_iface(
                    self.builder
                        .build_load(ptr, "ifaceload")
                        .into_struct_value(),
                    decrement,
                )
            }

            (BasicValueEnum::PointerValue(ptr), IRType::RefAdt(inst)) => {
                self.mod_refcount_adt(ptr, &inst, decrement)
            }

            (BasicValueEnum::PointerValue(_), IRType::Closure(_)) => {
                self.mod_refcount_closure(value.ptr(), decrement)
            }

            _ => (),
        }
    }

    fn mod_refcount_adt(&mut self, mut ptr: PointerValue, adt: &Instance<ADT>, decrement: bool) {
        if adt.ty.borrow().ty.is_extern_class() {
            return;
        }
        if let Some(destructor) = &adt.ty.borrow().methods.get("free-sr") {
            if ptr.get_type().get_element_type().is_pointer_type() {
                ptr = self.builder.build_load(ptr, "gcload").into_pointer_value();
            }
            let ptr = ptr;

            let inst = Instance::new(Rc::clone(destructor), Rc::clone(adt.args()));
            let func = self.get_or_create(&inst);

            let refcount = self.struct_gep_raw(ptr, 0);
            let refcount = self.write_new_refcount(refcount, decrement);
            if decrement {
                self.build_maybe_free(refcount, &mut |this, pred| {
                    this.builder
                        .build_call(func, &[ptr.into(), pred.into()], "free");
                })
            }
        }
    }

    fn mod_refcount_iface(&self, struc: StructValue, decrement: bool) {
        let func = if decrement {
            self.module.get_function("gelixrs_dec_ref_iface")
        } else {
            self.module.get_function("gelixrs_inc_ref_iface")
        }
        .unwrap();

        let int_ty = self.context.i64_type();
        let extract = |i: u32| {
            self.builder.build_ptr_to_int(
                self.builder
                    .build_extract_value(struc, i, "extr")
                    .unwrap()
                    .into_pointer_value(),
                int_ty,
                "cast",
            )
        };
        let first = extract(0);
        let second = extract(1);
        self.builder
            .build_call(func, &[first.into(), second.into()], "rc");
    }

    fn mod_refcount_closure(&self, mut ptr: LLPtr, decrement: bool) {
        if ptr.get_type().get_element_type().is_pointer_type() {
            *ptr = self.builder.build_load(*ptr, "gcload").into_pointer_value();
        }

        let refcount = self.struct_gep_raw(*ptr, 0);
        self.write_new_refcount(refcount, decrement);
        if decrement {
            let free_fn = self.struct_gep(&ptr, 1);
            let free_fn = self.load_ptr(&LLPtr::of(free_fn));
            self.builder
                .build_call(free_fn.into_pointer_value(), &[(*ptr).into()], "rccheck");
        }
    }

    pub(super) fn write_new_refcount(&self, refcount: PointerValue, decrement: bool) -> IntValue {
        let rc = self.builder.build_load(refcount, "rcload").into_int_value();
        let added = self.context.i32_type().const_int(1, false);
        let new_rc = if decrement {
            self.builder.build_int_sub(rc, added, "rcdec")
        } else {
            self.builder.build_int_add(rc, added, "rcinc")
        };
        self.builder.build_store(refcount, new_rc);
        new_rc
    }

    /// Will insert a free check at the current insert position.
    /// `free_closure` should generate the code that runs when the value
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
