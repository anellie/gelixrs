/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 2/3/20 3:28 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use gir_nodes::{declaration::Variable, types::TypeKind, Type};
use inkwell::{
    basic_block::BasicBlock,
    types::{AnyTypeEnum, BasicType, BasicTypeEnum},
    values::{BasicValue, BasicValueEnum, PointerValue},
    AddressSpace::Generic,
};
use std::mem;

use super::{type_adapter::IRType, IRGenerator};
use crate::generator::{LLPtr, LLValue};

impl IRGenerator {
    /// Force any type to be turned into a void pointer.
    /// Used for interface implementors.
    pub(crate) fn coerce_to_void_ptr(&self, ty: BasicValueEnum) -> BasicValueEnum {
        let target = self.void_ptr();
        match ty {
            BasicValueEnum::PointerValue(ptr) => self.builder.build_bitcast(ptr, target, "bc"),

            BasicValueEnum::IntValue(int) if int.get_type() == self.context.i64_type() => self
                .builder
                .build_int_to_ptr(int, target, "inttoptr")
                .into(),

            BasicValueEnum::IntValue(int) => {
                let num = self
                    .builder
                    .build_int_z_extend(int, self.context.i64_type(), "extend");
                self.coerce_to_void_ptr(num.into())
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
    pub(crate) fn get_variable(&self, var: &Variable) -> &LLPtr {
        self.variables.get(var).unwrap()
    }

    /// Write a set of values to a given struct.
    /// Starts at the first index until iterator is exhausted.
    pub(crate) fn write_struct(&mut self, location: &LLPtr, values: &[BasicValueEnum]) {
        for (i, value) in values.iter().enumerate() {
            let slot = self.struct_gep(location, i);
            self.builder.build_store(slot, *value);
        }
    }

    /// Loads a pointer, turning it into a value.
    /// Does not load structs or functions, since they are only ever used as pointers.
    pub(crate) fn load_ptr(&self, ptr: &LLPtr) -> LLValue {
        LLValue::cpy(
            match (ptr.get_type().get_element_type(), &ptr.ty) {
                (AnyTypeEnum::PointerType(ty), IRType::RefRawPtr)
                    if ty.get_element_type().is_pointer_type() =>
                {
                    self.builder.build_load(**ptr, "tptrload")
                }

                (AnyTypeEnum::PointerType(_), IRType::RefRawPtr) => (**ptr).into(),

                (AnyTypeEnum::PointerType(_), _) => self.builder.build_load(**ptr, "dptrload"),

                (AnyTypeEnum::FunctionType(_), _)
                | (_, IRType::ValueRawPtr)
                | (_, IRType::Other) => (**ptr).into(),

                (AnyTypeEnum::StructType(_), IRType::RefAdt(i))
                | (AnyTypeEnum::StructType(_), IRType::NullRefAdt(i))
                    if i.ty.borrow().is_ptr() =>
                {
                    (**ptr).into()
                }

                _ => self.builder.build_load(**ptr, "var"),
            },
            &ptr.ty,
        )
    }

    /// Perform a struct GEP with some additional safety checks.
    /// The index will be offset by one should the struct contain a refcount field,
    /// so callers do not need to account for this.
    pub(crate) fn struct_gep(&self, ptr: &LLPtr, index: usize) -> PointerValue {
        // Account for the reference count field, should it be present
        let index = index as u32 + self.get_struct_offset(ptr);
        self.struct_gep_raw(**ptr, index)
    }

    pub(crate) fn struct_gep_raw(&self, ptr: PointerValue, index: u32) -> PointerValue {
        assert!(ptr.get_type().get_element_type().is_struct_type());
        assert!(
            ptr.get_type()
                .get_element_type()
                .as_struct_type()
                .count_fields()
                > index
        );
        unsafe { self.builder.build_struct_gep(ptr, index as u32, "gep") }
    }

    pub(crate) fn get_type_info_field(&self, ptr: &LLPtr) -> PointerValue {
        self.struct_gep_raw(**ptr, Self::refcount_before_tyinfo(&ptr.ty) as u32)
    }

    pub(crate) fn get_struct_offset(&self, ptr: &LLPtr) -> u32 {
        let elem_ty = ptr.get_type().get_element_type();
        let struct_type = elem_ty.as_struct_type();
        let mut i = 0;

        // Account for the reference count field, should the struct be GCd
        i += Self::refcount_before_tyinfo(&ptr.ty) as u32;
        // Account for the type info field, should it be present
        i += (struct_type.get_field_type_at_index(i)
            == Some(self.type_info_type.ptr_type(Generic).into())) as u32;

        i
    }

    /// Creates a new stack allocation instruction in the entry block of the function.
    /// The alloca is kept empty.
    pub(crate) fn create_alloc(
        &mut self,
        gir: Type,
        ty: BasicTypeEnum,
        heap: bool,
    ) -> PointerValue {
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
            self.locals.last_mut().unwrap().push(LLPtr::from(ptr, &gir));
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

    pub(crate) fn locals(&mut self) -> &mut Vec<LLPtr> {
        self.locals.last_mut().unwrap()
    }

    pub(crate) fn push_local_scope(&mut self) {
        self.locals.push(Vec::with_capacity(5));
    }

    pub(crate) fn pop_dec_locals(&mut self) {
        let locals = self.locals.pop().unwrap();
        self.decrement_locals(&locals);
    }

    pub(crate) fn pop_locals_lift(&mut self, lift: &LLValue) {
        let mut locals = self.locals.pop().unwrap();

        if !locals.is_empty() && lift.try_ptr().is_some() {
            let index = locals.iter().position(|l| **l == **lift);
            if let Some(index) = index {
                locals.swap_remove(index);
                self.increment_refcount(&lift);
                self.locals().push(lift.try_ptr().unwrap());
            }
        }

        self.decrement_locals(&locals);
    }

    pub(crate) fn pop_locals_remove(&mut self, lift: &LLValue) {
        let locals = self.locals.pop().unwrap();
        self.increment_refcount(lift);
        self.decrement_locals(&locals);
    }

    pub(crate) fn decrement_all_locals(&mut self) {
        // Work around borrowck being a pain
        let locals = mem::replace(&mut self.locals, vec![]);

        for allocs in &locals {
            self.decrement_locals(&allocs);
        }
        self.locals = locals;
    }

    fn decrement_locals(&mut self, locals: &[LLPtr]) {
        if self.builder.get_insert_block().is_some() {
            for local in locals {
                self.free_local(local);
            }
        }
    }

    fn free_local(&mut self, ptr: &LLPtr) -> Option<()> {
        let value = self.load_ptr(ptr);

        match &ptr.ty {
            IRType::RefAdt(inst) if inst.ty.borrow().type_kind == TypeKind::Reference => {
                self.decrement_refcount(&ptr.val())
            }

            IRType::ValueAdt(adt) => {
                let method = adt.try_get_method("free-instance")?;
                let ir = self
                    .get_or_create(&method)
                    .as_global_value()
                    .as_pointer_value();
                self.builder.build_call(ir, &[*value], "free");
            }

            // Primitive, simply calling free is enough since it must be a raw pointer
            _ => (),
        };
        None
    }

    pub(crate) fn build_phi(&mut self, nodes: &[(LLValue, BasicBlock)]) -> BasicValueEnum {
        let nodes = nodes
            .iter()
            .filter(|(v, _)| !matches!(v.ty, IRType::None))
            .collect::<Vec<_>>();

        match nodes.len() {
            0 => *self.none_const,
            1 => {
                if let Some(p) = nodes[0].0.try_ptr() {
                    self.locals().push(p)
                }
                *(nodes[0].0)
            }
            _ => {
                let ty = (nodes[0].0).get_type();
                let phi_nodes = nodes
                    .iter()
                    .map(|(v, b)| (&**v as &dyn BasicValue, b))
                    .collect::<Vec<_>>();
                let phi = self.builder.build_phi(ty, "phi");
                phi.add_incoming(&phi_nodes);
                let phi_ll = LLValue::cpy(phi.as_basic_value(), &(nodes[0].0).ty);
                if let Some(p) = phi_ll.try_ptr() {
                    self.locals().push(p)
                }
                phi.as_basic_value()
            }
        }
    }
}
