/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 2/3/20 3:28 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::{
    gir::{nodes::declaration::Variable, Type},
    ir::IRGenerator,
};
use inkwell::{
    basic_block::BasicBlock,
    types::{AnyTypeEnum, BasicType, BasicTypeEnum},
    values::{BasicValue, BasicValueEnum, PointerValue},
    AddressSpace::Generic,
};
use std::mem;

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

            BasicValueEnum::FloatValue(flt) => self.coerce_to_void_ptr(
                self.builder.build_bitcast(
                    self.builder
                        .build_float_ext(flt, self.context.f64_type(), "fltextend"),
                    self.context.i64_type(),
                    "flttoint",
                ),
            ),

            _ => panic!("Cannot coerce to void ptr: {:?}", ty),
        }
    }

    /// Returns the IR pointer of the variable.
    pub fn get_variable(&self, var: &Variable) -> PointerValue {
        *self.variables.get(var).unwrap()
    }

    /// Write a set of values to a given struct.
    pub fn write_struct<'a, T: Iterator<Item = &'a BasicValueEnum>>(
        &self,
        location: PointerValue,
        values: T,
    ) {
        for (i, value) in values.enumerate() {
            let slot = self.struct_gep(location, i);
            self.build_store(slot, *value, true);
        }
    }

    /// Loads a pointer, turning it into a value.
    /// Does not load structs or functions, since they are only ever used as pointers.
    pub fn load_ptr(&self, ptr: PointerValue) -> BasicValueEnum {
        match ptr.get_type().get_element_type() {
            AnyTypeEnum::FunctionType(_) => BasicValueEnum::PointerValue(ptr),
            AnyTypeEnum::StructType(str)
                if str
                    .get_name()
                    .map_or(true, |n| !n.to_str().unwrap().starts_with("iface")) =>
            {
                ptr.into()
            }
            _ => self.builder.build_load(ptr, "var"),
        }
    }

    /// Similar to the function above, but with GIR type info.
    /// This is sometimes required to prevent unintentionally loading
    /// a value, for example when dealing with primitive pointers.
    pub fn load_ptr_gir(&self, ptr: PointerValue, mir_ty: &Type) -> BasicValueEnum {
        match (ptr.get_type().get_element_type(), mir_ty) {
            (AnyTypeEnum::IntType(_), Type::RawPtr(_)) => ptr.into(),
            (AnyTypeEnum::PointerType(inner), Type::RawPtr(_))
                if inner.get_element_type().is_struct_type() =>
            {
                ptr.into()
            }
            (_, Type::Value(_)) => self.builder.build_load(ptr, "dvload"),
            _ => self.load_ptr(ptr),
        }
    }

    /// Perform a struct GEP with some additional safety checks.
    /// The index will be offset by one should the struct contain a refcount field,
    /// so callers do not need to account for this.
    pub fn struct_gep(&self, ptr: PointerValue, _index: usize) -> PointerValue {
        assert!(ptr.get_type().get_element_type().is_struct_type());

        // Account for the reference count field, should it be present
        let index = _index as u32 + self.get_struct_offset(ptr);

        assert!(
            ptr.get_type()
                .get_element_type()
                .as_struct_type()
                .count_fields()
                > index
        );

        unsafe { self.builder.build_struct_gep(ptr, index as u32, "gep") }
    }

    pub fn get_type_info_field(&self, ptr: PointerValue) -> PointerValue {
        unsafe {
            self.builder.build_struct_gep(
                ptr,
                Self::needs_gc(*ptr.get_type().get_element_type().as_struct_type()) as u32,
                "gep",
            )
        }
    }

    pub fn get_struct_offset(&self, ptr: PointerValue) -> u32 {
        let elem_ty = ptr.get_type().get_element_type();
        let struct_type = elem_ty.as_struct_type();
        let mut i = 0;

        // Account for the reference count field, should the struct be GCd
        i += Self::needs_gc(*struct_type) as u32;
        // Account for the type info field, should it be present
        i += (struct_type.get_field_type_at_index(i)
            == Some(self.type_info_type.ptr_type(Generic).into())) as u32;

        i
    }

    /// Creates a new stack allocation instruction in the entry block of the function.
    /// The alloca is kept empty.
    pub fn create_alloc(&mut self, ty: BasicTypeEnum, heap: bool) -> PointerValue {
        let builder = self.context.create_builder();

        let (builder, ptr) = if heap {
            let malloc = self
                .module
                .get_function("malloc")
                .unwrap()
                .as_global_value()
                .as_pointer_value();
            let malloc_ty = ty
                .ptr_type(Generic)
                .fn_type(&[self.context.i32_type().into()], false);
            let malloc = self
                .builder
                .build_bitcast(malloc, malloc_ty.ptr_type(Generic), "malloccast")
                .into_pointer_value();

            let i = self.context.i32_type();
            let ty_size = unsafe {
                self.builder.build_gep(
                    ty.ptr_type(Generic).const_null(),
                    &[i.const_int(1, false)],
                    "size",
                )
            };
            let ty_size = self.builder.build_ptr_to_int(ty_size, i, "sizeint").into();

            (
                &self.builder,
                self.builder
                    .build_call(malloc, &[ty_size], "malloc")
                    .try_as_basic_value()
                    .left()
                    .unwrap()
                    .into_pointer_value(),
            )
        } else {
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

            let ptr = builder.build_alloca(ty, "alloc");
            self.local_allocs.last_mut().unwrap().push(ptr);
            (&builder, ptr)
        };

        if ty.is_struct_type()
            && ty.as_struct_type().get_field_type_at_index(0)
                == Some(self.context.i32_type().into())
        {
            // Initialize the refcount to 0
            let rc = unsafe { builder.build_struct_gep(ptr, 0, "rcinit") };
            builder.build_store(rc, self.context.i32_type().const_int(0, false));
        }
        ptr
    }

    pub fn locals(&mut self) -> &mut Vec<(BasicValueEnum, bool)> {
        self.locals.last_mut().unwrap()
    }

    pub fn push_local_scope(&mut self) {
        self.locals.push(Vec::with_capacity(5));
        self.local_allocs.push(Vec::with_capacity(2))
    }

    pub fn pop_dec_locals(&mut self) {
        let locals = self.locals.pop().unwrap();
        self.decrement_locals(&locals);
        let local_allocs = self.local_allocs.pop().unwrap();
        self.kill_local_allocs(&local_allocs);
    }

    pub fn pop_locals_lift(&mut self, lift: BasicValueEnum) {
        let locals = self.locals.pop().unwrap();

        if !locals.is_empty() {
            self.increment_refcount(lift, false);
            self.locals().push((lift, false));
        }

        self.decrement_locals(&locals);

        let local_allocs = self.local_allocs.pop().unwrap();
        self.kill_local_allocs(&local_allocs);
    }

    pub fn pop_locals_remove(&mut self, lift: BasicValueEnum) {
        let locals = self.locals.pop().unwrap();
        self.increment_refcount(lift, false);
        self.decrement_locals(&locals);

        let local_allocs = self.local_allocs.pop().unwrap();
        self.kill_local_allocs(&local_allocs);
    }

    pub fn decrement_all_locals(&mut self) {
        for locals in &self.locals {
            self.decrement_locals(locals);
        }

        // Work around borrowck being a pain
        let locals = mem::replace(&mut self.local_allocs, vec![]);
        for allocs in &locals {
            self.kill_local_allocs(&allocs);
        }
        self.local_allocs = locals;
    }

    fn decrement_locals(&self, locals: &[(BasicValueEnum, bool)]) {
        if self.builder.get_insert_block().is_some() {
            for (local, is_ptr) in locals {
                self.decrement_refcount(*local, *is_ptr);
            }
        }
    }

    fn kill_local_allocs(&mut self, local_allocs: &[PointerValue]) {
        if self.builder.get_insert_block().is_some() {
            for local in local_allocs {
                if let Some(dest) = self.get_destructor(*local) {
                    self.builder.build_call(dest, &[(*local).into()], "free");
                }
            }
        }
    }

    fn get_destructor(&mut self, ptr: PointerValue) -> Option<PointerValue> {
        let inst = self
            .types_bw
            .get(
                ptr.get_type()
                    .get_element_type()
                    .as_struct_type()
                    .get_name()
                    .unwrap()
                    .to_str()
                    .unwrap(),
            )?
            .clone();
        let method = match inst {
            Type::WeakRef(ref adt) => adt.get_method("free-wr"),
            Type::StrongRef(ref adt) => adt.get_method("free-sr"),
            // Primitive, simply calling free is enough since it must be a raw pointer
            _ => return None,
        };
        Some(
            self.get_or_create(&method)
                .as_global_value()
                .as_pointer_value(),
        )
    }

    pub fn nullptr(&self) -> PointerValue {
        self.context.i64_type().ptr_type(Generic).const_null()
    }

    pub fn build_phi(&mut self, nodes: &[(BasicValueEnum, BasicBlock)]) -> BasicValueEnum {
        let nodes = nodes
            .iter()
            .filter(|(v, _)| v.get_type() != self.none_const.get_type())
            .collect::<Vec<_>>();

        match nodes.len() {
            0 => self.none_const,
            1 => {
                self.locals().push((nodes[0].0, false));
                nodes[0].0
            }
            _ => {
                let ty = nodes[0].0.get_type();
                let nodes = nodes
                    .iter()
                    .map(|(v, b)| (v as &dyn BasicValue, b))
                    .collect::<Vec<_>>();
                let phi = self.builder.build_phi(ty, "phi");
                phi.add_incoming(&nodes);
                self.locals().push((phi.as_basic_value(), false));
                phi.as_basic_value()
            }
        }
    }
}
