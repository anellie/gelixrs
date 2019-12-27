/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 4:54 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use inkwell::types::{AnyTypeEnum, BasicTypeEnum};
use inkwell::values::{BasicValueEnum, PointerValue};
use std::rc::Rc;
use crate::mir::nodes::Variable;
use crate::ir::{PtrEqRc, IRGenerator};

impl IRGenerator {
    /// Force any type to be turned into a void pointer.
    pub fn coerce_to_void_ptr(&self, ty: BasicValueEnum) -> BasicValueEnum {
        let target = self.void_ptr();
        match ty {
            BasicValueEnum::PointerValue(ptr) => self.builder.build_bitcast(ptr, target, "bc"),

            BasicValueEnum::IntValue(int) => {
                let num = self
                    .builder
                    .build_int_z_extend(int, self.context.i64_type(), "extend");
                self.builder
                    .build_int_to_ptr(num, target, "inttoptr")
                    .into()
            }

            BasicValueEnum::FloatValue(flt) => self.coerce_to_void_ptr(self.builder.build_bitcast(
                flt,
                self.context.i64_type(),
                "flttoint",
            )),

            _ => panic!("Cannot coerce to void ptr: {:?}", ty),
        }
    }

    /// Returns the IR pointer of the variable.
    /// Can also be used for globals, although variables are
    /// searched for first.
    pub fn get_variable(&self, var: &Rc<Variable>) -> PointerValue {
        let wrap = PtrEqRc::new(var);
        self.variables
            .get(&wrap)
            .cloned()
            .unwrap_or_else(|| self.functions[&wrap].as_global_value().as_pointer_value())
    }

    /// Creates an alloca and stores the given value in it, returning
    /// a reference to this new store.
    /// Note that this is only done for struct types; all other types are
    /// simply returned without modification.
    pub fn build_local_store(&self, value: BasicValueEnum) -> BasicValueEnum {
        // Don't bother allocating a type that does not need it
        if !value.get_type().is_struct_type() {
            return value;
        }

        let alloca = self.create_alloca(value.get_type());
        self.builder
            .build_store(alloca, self.unwrap_struct_ptr(value));
        self.load_ptr(alloca)
    }

    /// Similar to the function above; creates a local alloca.
    /// Instead of taking the value itself, the type given must be a struct,
    /// with `values` being an iterator of things to fill the struct with.
    /// Note that stored values are taken as-is, no type checking is done.
    pub fn build_local_struct<'a, T: Iterator<Item = &'a BasicValueEnum>>(
        &self,
        ty: BasicTypeEnum,
        values: T,
    ) -> BasicValueEnum {
        assert!(ty.as_struct_type().count_fields() == values.size_hint().0 as u32);
        let alloca = self.create_alloca(ty);
        for (i, value) in values.into_iter().enumerate() {
            let slot = unsafe { self.builder.build_struct_gep(alloca, i as u32, "gep") };
            self.builder.build_store(slot, *value);
        }
        alloca.into()
    }

    /// If the parameter 'ptr' is a struct pointer, the struct itself is returned.
    /// Otherwise, ptr is simply returned without modification.
    pub fn unwrap_struct_ptr(&self, val: BasicValueEnum) -> BasicValueEnum {
        if let BasicValueEnum::PointerValue(ptr) = val {
            if let AnyTypeEnum::StructType(_) = ptr.get_type().get_element_type() {
                return self.builder.build_load(ptr, "ptr-load");
            }
        }
        val
    }

    /// Loads a pointer, turning it into a value.
    /// Does not load structs or functions, since they are only ever used as pointers.
    pub fn load_ptr(&self, ptr: PointerValue) -> BasicValueEnum {
        match ptr.get_type().get_element_type() {
            AnyTypeEnum::FunctionType(_) => BasicValueEnum::PointerValue(ptr),
            AnyTypeEnum::StructType(_) => BasicValueEnum::PointerValue(ptr),
            _ => self.builder.build_load(ptr, "var"),
        }
    }

    /// Creates a new stack allocation instruction in the entry block of the function.
    /// The alloca is kept empty.
    fn create_alloca(&self, ty: BasicTypeEnum) -> PointerValue {
        let builder = self.context.create_builder();
        let entry = self
            .builder
            .get_insert_block()
            .unwrap()
            .get_parent()
            .unwrap()
            .get_first_basic_block()
            .unwrap();

        match entry.get_first_instruction() {
            Some(first_instr) => builder.position_before(&first_instr),
            None => builder.position_at_end(&entry),
        }

        builder.build_alloca(ty, "tmpalloc")
    }
}
