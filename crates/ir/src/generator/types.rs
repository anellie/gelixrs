/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 2/3/20 1:58 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use common::MutRc;
use gir_ir_adapter::IRAdtInfo;
use gir_nodes::{
    declaration::{ADTType, LocalVariable},
    types::{ClosureType, TypeArguments, TypeKind, TypeVariable},
    Function, Instance, Type, ADT,
};
use inkwell::{
    types::{BasicType, BasicTypeEnum, FunctionType, PointerType, StructType},
    values::PointerValue,
    AddressSpace::Generic,
};
use std::{cell::Ref, rc::Rc};

use super::IRGenerator;
use crate::generator::type_adapter::IRType;

impl IRGenerator {
    /// Converts a GIR type to the corresponding LLVM type.
    /// This generic variant is used for:
    /// - Function parameters & return type
    /// - ADT fields/members
    /// - Cast target types
    pub(crate) fn ir_ty_generic(&mut self, gir: &Type) -> BasicTypeEnum {
        self.ir_ty_generic_full(gir).0
    }

    /// Converts a GIR type to the corresponding LLVM type.
    /// This variant is used for allocations.
    pub(crate) fn ir_ty_allocs(&mut self, gir: &Type) -> BasicTypeEnum {
        self.ir_ty_generic_full(gir).0
    }

    pub(crate) fn ir_ty_generic_full(
        &mut self,
        gir: &Type,
    ) -> (BasicTypeEnum, Option<PointerValue>) {
        let raw = self.ir_ty_raw(gir);
        (
            match self.maybe_unwrap_var(gir) {
                Type::Adt(inst) | Type::Nullable(box Type::Adt(inst))
                    if inst.ty.borrow().is_ptr() =>
                {
                    raw.0.ptr_type(Generic).into()
                }
                Type::Closure(_) | Type::ClosureCaptured(_) => raw.0.ptr_type(Generic).into(),
                _ => raw.0,
            },
            raw.1,
        )
    }

    /// Converts a `MIRType` to the corresponding LLVM type info global struct.
    pub(crate) fn ir_ty_info(&mut self, gir: &Type) -> Option<PointerValue> {
        self.ir_ty_raw(gir).1
    }

    pub(crate) fn ir_ty_raw(&mut self, gir: &Type) -> (BasicTypeEnum, Option<PointerValue>) {
        let (ty, ptr) = match gir {
            Type::Any | Type::None => (self.none_const.get_type(), None),
            Type::Bool => (self.context.bool_type().into(), None),
            Type::I8 | Type::U8 => (self.context.i8_type().into(), None),
            Type::I16 | Type::U16 => (self.context.i16_type().into(), None),
            Type::I32 | Type::U32 => (self.context.i32_type().into(), None),
            Type::I64 | Type::U64 => (self.context.i64_type().into(), None),
            Type::F32 => (self.context.f32_type().into(), None),
            Type::F64 => (self.context.f64_type().into(), None),

            Type::Function(func) => (
                self.get_or_create(func).get_type().ptr_type(Generic).into(),
                None,
            ),

            Type::Closure(closure) => {
                let ty = if let Some(ty) = closure.ir.get() {
                    ty
                } else {
                    self.build_closure_type(closure)
                };
                (ty.into(), None)
            }

            Type::ClosureCaptured(captured) => (self.build_captured_type(captured).into(), None),

            Type::Adt(inst) => {
                let adt = self.get_or_build_adt(inst);
                (adt.adt.into(), Some(adt.typeinfo))
            }

            Type::Nullable(box Type::Adt(inst))
                if inst.ty.borrow().type_kind == TypeKind::Reference =>
            {
                let adt = self.get_or_build_adt(inst);
                (adt.adt.into(), Some(adt.typeinfo))
            }

            Type::Nullable(_) => todo!("value nullables"),

            Type::RawPtr(inner) => {
                let (inner, ptr) = self.ir_ty_generic_full(inner);
                (inner.ptr_type(Generic).into(), ptr)
            }

            Type::Variable(var) => self.ir_ty_raw(&self.unwrap_var(var)),

            Type::Type(_) => panic!("invalid type"),
        };

        (ty, ptr)
    }

    pub(crate) fn process_args(&self, args: &Rc<TypeArguments>) -> Rc<TypeArguments> {
        Rc::new(
            args.iter()
                .map(|a| self.maybe_unwrap_var(a))
                .collect::<Vec<_>>(),
        )
    }

    pub(crate) fn maybe_unwrap_var(&self, ty: &Type) -> Type {
        match ty {
            Type::Variable(var) => self.unwrap_var(var),
            _ => ty.clone(),
        }
    }

    pub(crate) fn unwrap_var(&self, var: &TypeVariable) -> Type {
        // Edge case in intrinsc generator...
        if self.type_args.is_empty() {
            return Type::None;
        }

        let index = self.type_args.len() - 1;
        self.type_args[index][var.index].clone()
    }

    fn get_or_build_adt(&mut self, inst: &Instance<ADT>) -> IRAdtInfo {
        let inst = Instance::new(Rc::clone(&inst.ty), self.process_args(inst.args()));
        let info = inst.ty.borrow().ir.get_inst(inst.args());
        match info {
            Some(info) => info,
            None => {
                let info = IRAdtInfo {
                    adt: self.build_adt(&inst, false, ""),
                    nullable: self.build_adt(&inst, true, "nullable-"),
                    typeinfo: self.build_type_info(),
                };
                inst.ty.borrow_mut().ir.add_inst(inst.args(), info);
                info
            }
        }
    }

    fn build_adt(&mut self, inst: &Instance<ADT>, weak: bool, prefix: &str) -> StructType {
        self.push_ty_args(Rc::clone(inst.args()));
        let adt = inst.ty.borrow();
        let ret = match adt.ty {
            ADTType::Class { external } if external => self.build_struct(
                &adt.name,
                adt.fields.iter().map(|(_, m)| &m.ty),
                false,
                false,
            ),

            ADTType::Interface => self.build_iface_type(adt, weak),

            _ => self.build_struct(
                &format!("{}{}-{}", prefix, &adt.name, adt.ir.count()),
                adt.fields.iter().map(|(_, m)| &m.ty),
                !weak,
                true,
            ),
        };
        self.pop_ty_args();
        ret
    }

    /// Generates the struct for captured variables, given a list of them.
    fn build_captured_type(&mut self, captured: &[Rc<LocalVariable>]) -> StructType {
        self.build_struct(
            "SR-closure-captured",
            captured.iter().map(|var| &var.ty),
            true,
            false,
        )
    }

    /// Generates a struct out of an iterator of member types.
    fn build_struct<'a, T: Iterator<Item = &'a Type>>(
        &mut self,
        name: &str,
        body: T,
        refcount: bool,
        type_info: bool,
    ) -> StructType {
        let body: Vec<_> = body.map(|var| self.ir_ty_generic(&var)).collect();
        self.build_struct_ir(name, body.into_iter(), refcount, type_info)
    }

    fn build_struct_ir<T: Iterator<Item = BasicTypeEnum>>(
        &self,
        name: &str,
        fields: T,
        refcount: bool,
        type_info: bool,
    ) -> StructType {
        let mut body = Vec::with_capacity(fields.size_hint().0 + 2);
        if refcount {
            body.push(self.context.i32_type().into())
        }
        if type_info {
            body.push(self.type_info_type.ptr_type(Generic).into())
        }
        for item in fields {
            body.push(item);
        }

        let struc_val = self.context.opaque_struct_type(name);
        struc_val.set_body(&body, false);
        struc_val
    }

    /// Generates the struct for a closure, containing a function pointer
    /// and a pointer to captured variables.
    fn build_closure_type(&mut self, closure: &ClosureType) -> StructType {
        let refcount = self.context.i32_type().into();
        let func_ty = self
            .fn_type_from_raw(
                Some(Type::I64).iter().chain(closure.parameters.iter()),
                &closure.ret_type,
                false,
            )
            .ptr_type(Generic)
            .into();
        let captured_ty = self.context.i64_type().into();

        let struc_ty = self.context.opaque_struct_type("SR-closure");
        let free_ty = self
            .context
            .void_type()
            .fn_type(&[struc_ty.ptr_type(Generic).into()], false)
            .ptr_type(Generic)
            .into();
        struc_ty.set_body(&[refcount, func_ty, free_ty, captured_ty], false);

        closure.ir.set(Some(struc_ty));
        struc_ty
    }

    /// Generates a function type from raw parts - parameters, return type.
    pub(crate) fn fn_type_from_raw<'a, T: Iterator<Item = &'a Type>>(
        &mut self,
        params: T,
        ret_type: &Type,
        variadic: bool,
    ) -> FunctionType {
        let params: Vec<BasicTypeEnum> = params.map(|param| self.ir_ty_generic(param)).collect();
        if *ret_type == Type::None {
            self.context.void_type().fn_type(&params, variadic)
        } else {
            self.ir_ty_generic(ret_type).fn_type(&params, variadic)
        }
    }

    const IFACE_EXCLUDE_METHODS: [&'static str; 3] = ["new-instance", "free-wr", "free-sr"];

    /// Generate the type of an interface when used as a standalone type,
    /// which is a struct with 2 pointers (vtable + implementor).
    fn build_iface_type(&mut self, iface: Ref<ADT>, weak: bool) -> StructType {
        let free_method_sig = Some(
            self.context
                .void_type()
                .fn_type(
                    &[self.void_ptr().into(), self.context.bool_type().into()],
                    false,
                )
                .ptr_type(Generic)
                .into(),
        );
        let vtable: Vec<BasicTypeEnum> = free_method_sig
            .into_iter()
            .chain(
                iface
                    .methods
                    .iter()
                    .filter(|(name, _)| !Self::IFACE_EXCLUDE_METHODS.contains(&&***name))
                    .map(|(_, method)| self.build_iface_method_type(method)),
            )
            .collect();
        let vtable_struct = self.build_struct_ir("vtable", vtable.into_iter(), false, false);

        self.build_struct_ir(
            &format!("iface-{}{}", if weak { "WR-" } else { "" }, &iface.name),
            vec![
                self.context.i64_type().ptr_type(Generic).into(),
                vtable_struct.ptr_type(Generic).into(),
            ]
            .into_iter(),
            false,
            false,
        )
    }

    fn build_iface_method_type(&mut self, method: &MutRc<Function>) -> BasicTypeEnum {
        let params: Vec<BasicTypeEnum> = Some(self.void_ptr().into())
            .into_iter()
            .chain(
                method
                    .borrow()
                    .parameters
                    .iter()
                    .skip(1)
                    .map(|param| self.ir_ty_generic(&param.ty)),
            )
            .collect();

        if method.borrow().ret_type == Type::None {
            self.context.void_type().fn_type(params.as_slice(), false)
        } else {
            let ret_type = self.ir_ty_generic(&method.borrow().ret_type);
            ret_type.fn_type(params.as_slice(), false)
        }
        .ptr_type(Generic)
        .into()
    }

    fn build_type_info(&self) -> PointerValue {
        let global = self
            .module
            .add_global(self.type_info_type, None, "typeinfo");
        global.set_initializer(
            &self.type_info_type.const_named_struct(&[self
                .context
                .i64_type()
                .const_int(0, false)
                .into()]),
        );
        global.as_pointer_value()
    }

    pub(crate) fn refcount_before_tyinfo(ty: &IRType) -> bool {
        if let IRType::Adt(adt) = ty {
            adt.ty.borrow().refcounted()
        } else {
            false
        }
    }

    pub(crate) fn void_ptr(&self) -> PointerType {
        self.context.i64_type().ptr_type(Generic)
    }
}
