/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 6:11 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::ir::IRGenerator;
use crate::mir::nodes::{Class, ClosureType, Function, IFaceMethod, Interface, Type, Variable};
use crate::mir::MutRc;
use inkwell::types::{BasicType, BasicTypeEnum, FunctionType, PointerType, StructType};
use inkwell::AddressSpace::Generic;
use std::cell::Ref;
use std::rc::Rc;

impl IRGenerator {
    /// Converts a MIRType to the corresponding LLVM type.
    /// Structs are returned as PointerType<StructType>.
    pub fn ir_ty_ptr(&mut self, mir: &Type) -> BasicTypeEnum {
        let ir = self.ir_ty(mir);
        match ir {
            BasicTypeEnum::StructType(struc) => struc.ptr_type(Generic).into(),
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
        let struc_val = self.context.opaque_struct_type(name);
        let body: Vec<BasicTypeEnum> = body.map(|var| self.ir_ty(&var)).collect();
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

        let ty = self.context.opaque_struct_type("closure");
        ty.set_body(&[func_ty, captured_ty], false);
        ty.into()
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
            self.ir_ty(ret_type).fn_type(&params, variadic)
        }
    }

    /// Generate the type of an interface when used as a standalone type,
    /// which is a struct with 2 pointers (vtable + implementor).
    fn build_iface_type(&mut self, iface: Ref<Interface>) -> StructType {
        let vtable: Vec<BasicTypeEnum> = iface
            .methods
            .iter()
            .map(|(_, method)| self.build_iface_method_type(method))
            .collect();
        let vtable_struct = self.context.struct_type(&vtable, false);

        let struc_val = self.context.opaque_struct_type(&iface.name);
        struc_val.set_body(
            &[
                self.context.i64_type().ptr_type(Generic).into(),
                vtable_struct.ptr_type(Generic).into(),
            ],
            false,
        );
        struc_val
    }

    fn build_iface_method_type(&mut self, method: &IFaceMethod) -> BasicTypeEnum {
        let params: Vec<BasicTypeEnum> = Some(self.void_ptr().into())
            .into_iter()
            .chain(method.parameters.iter().map(|param| self.ir_ty_ptr(&param)))
            .collect();

        if method.ret_type == Type::None {
            self.context.void_type().fn_type(params.as_slice(), false)
        } else {
            let ret_type = self.ir_ty(&method.ret_type);
            ret_type.fn_type(params.as_slice(), false)
        }
        .ptr_type(Generic)
        .into()
    }

    pub fn void_ptr(&self) -> PointerType {
        self.context.i64_type().ptr_type(Generic)
    }
}
