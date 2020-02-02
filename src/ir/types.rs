/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/28/19 2:35 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::{
    ir::IRGenerator,
    mir::{
        nodes::{Class, ClosureType, Function, IFaceMethod, Interface, Type, Variable},
        MutRc,
    },
};
use inkwell::{
    types::{BasicType, BasicTypeEnum, FunctionType, PointerType, StructType},
    AddressSpace::Generic,
};
use std::{cell::Ref, rc::Rc};

impl IRGenerator {
    /// Converts a MIRType to the corresponding LLVM type.
    /// Structs are returned as PointerType<StructType>.
    pub fn ir_ty_ptr(&mut self, mir: &Type) -> BasicTypeEnum {
        if mir == &Type::None {
            return self.none_const.get_type();
        };
        let ir = self.ir_ty(mir);
        match ir {
            // If the first field is NOT i32, then this struct type is not refcounted -
            // all non-refcounted structs are passed by value, and do not need to be
            // turned into a pointer (currently, only interface fat pointers are like this)
            BasicTypeEnum::StructType(struc)
                if struc.get_field_type_at_index(0) == Some(self.context.i32_type().into()) =>
            {
                struc.ptr_type(Generic).into()
            }
            _ => ir,
        }
    }

    /// Converts a MIRType to the corresponding LLVM type.
    pub fn ir_ty(&mut self, mir: &Type) -> BasicTypeEnum {
        self.types
            .get(mir)
            .copied()
            .unwrap_or_else(|| self.build_type(mir))
    }

    /// Generates a type, if it was not found in self.types.
    fn build_type(&mut self, ty: &Type) -> BasicTypeEnum {
        let ir_ty = match ty {
            // Any is a special case - it is not in self.types
            // as it is considered equal to all other types -
            // this would break the hashmap and result in returning
            // of Any when the type searched for is not Any.
            Type::Any => return self.none_const.get_type(),

            Type::Function(func) => self.build_fn_type(func.borrow()).ptr_type(Generic).into(),

            Type::Closure(closure) => self.build_closure_type(closure),

            Type::ClosureCaptured(captured) => self.build_captured_type(captured).into(),

            Type::Class(class) => self.build_class(class).into(),

            Type::Interface(iface) => self.build_iface_type(iface.borrow()).into(),

            _ => panic!(format!("Unknown type '{}' to build", ty)),
        };

        self.types.insert(ty.clone(), ir_ty);
        if let BasicTypeEnum::StructType(struc) = ir_ty {
            self.types_bw.insert(
                struc.get_name().unwrap().to_str().unwrap().to_string(),
                ty.clone(),
            );
        }
        ir_ty
    }

    /// Generates a class struct and its body.
    fn build_class(&mut self, class: &MutRc<Class>) -> StructType {
        let class = class.borrow();
        self.build_struct(&class.name, class.members.iter().map(|(_, m)| &m.type_))
    }

    /// Generates the struct for captured variables, given a list of them.
    fn build_captured_type(&mut self, captured: &[Rc<Variable>]) -> StructType {
        self.build_struct("closure-captured", captured.iter().map(|var| &var.type_))
    }

    /// Generates a struct out of an iterator of member types.
    fn build_struct<'a, T: Iterator<Item = &'a Type>>(
        &mut self,
        name: &str,
        body: T,
    ) -> StructType {
        let body: Vec<_> = body.map(|var| self.ir_ty_ptr(&var)).collect();
        self.build_struct_ir(name, body.into_iter(), true)
    }

    fn build_struct_ir<T: Iterator<Item = BasicTypeEnum>>(
        &self,
        name: &str,
        body: T,
        refcount: bool,
    ) -> StructType {
        let first_field = if refcount {
            Some(self.context.i32_type().into())
        } else {
            None
        };

        let struc_val = self.context.opaque_struct_type(name);
        let body: Vec<_> = first_field.into_iter().chain(body).collect();
        struc_val.set_body(&body, false);
        struc_val
    }

    /// Generates the struct for a closure, containing a function pointer
    /// and a pointer to captured variables.
    fn build_closure_type(&mut self, closure: &ClosureType) -> BasicTypeEnum {
        let func_ty = self
            .fn_type_from_raw(
                Some(Type::I64).iter().chain(closure.parameters.iter()),
                &closure.ret_type,
                false,
            )
            .ptr_type(Generic)
            .into();
        let captured_ty = self.context.i64_type().into();

        self.build_struct_ir("closure", vec![func_ty, captured_ty].into_iter(), true)
            .into()
    }

    /// Generates the LLVM FunctionType of a MIR function.
    pub(super) fn build_fn_type(&mut self, func: Ref<Function>) -> FunctionType {
        let params = func.parameters.iter().map(|param| &param.type_);
        let variadic = func.ast.as_ref().map(|a| a.sig.variadic).unwrap_or(false);
        self.fn_type_from_raw(params, &func.ret_type, variadic)
    }

    /// Generates a function type from raw parts - parameters, return type.
    fn fn_type_from_raw<'a, T: Iterator<Item = &'a Type>>(
        &mut self,
        params: T,
        ret_type: &Type,
        variadic: bool,
    ) -> FunctionType {
        let params: Vec<BasicTypeEnum> = params.map(|param| self.ir_ty_ptr(param)).collect();
        if *ret_type == Type::None {
            self.context.void_type().fn_type(&params, variadic)
        } else {
            self.ir_ty_ptr(ret_type).fn_type(&params, variadic)
        }
    }

    /// Generate the type of an interface when used as a standalone type,
    /// which is a struct with 2 pointers (vtable + implementor).
    fn build_iface_type(&mut self, iface: Ref<Interface>) -> StructType {
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
                    .map(|(_, method)| self.build_iface_method_type(method)),
            )
            .collect();
        let vtable_struct = self.build_struct_ir("vtable", vtable.into_iter(), false);

        self.build_struct_ir(
            &iface.name,
            vec![
                self.context.i64_type().ptr_type(Generic).into(),
                vtable_struct.ptr_type(Generic).into(),
            ]
            .into_iter(),
            false,
        )
    }

    fn build_iface_method_type(&mut self, method: &IFaceMethod) -> BasicTypeEnum {
        let params: Vec<BasicTypeEnum> = Some(self.void_ptr().into())
            .into_iter()
            .chain(method.parameters.iter().map(|param| self.ir_ty_ptr(&param)))
            .collect();

        if method.ret_type == Type::None {
            self.context.void_type().fn_type(params.as_slice(), false)
        } else {
            let ret_type = self.ir_ty_ptr(&method.ret_type);
            ret_type.fn_type(params.as_slice(), false)
        }
        .ptr_type(Generic)
        .into()
    }

    pub fn void_ptr(&self) -> PointerType {
        self.context.i64_type().ptr_type(Generic)
    }
}
