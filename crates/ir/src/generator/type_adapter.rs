use gir_nodes::{types::ClosureType, Instance, Type, ADT};
use inkwell::values::{BasicValue, BasicValueEnum, PointerValue};
use std::{
    ops::{Deref, DerefMut},
    rc::Rc,
};

#[derive(Debug, Clone)]
pub(crate) enum IRType {
    Adt(Instance<ADT>),
    Nullable(Instance<ADT>),
    Closure(Rc<ClosureType>),
    RawPtr,
    Primitive,
    Other,
    None,
}

#[derive(Debug, Clone)]
pub(crate) struct V<T: Copy> {
    v: T,
    pub(crate) ty: IRType,
}

impl<T: Copy> V<T> {
    pub fn from(llvm: T, ty: &Type) -> Self {
        Self {
            v: llvm,
            ty: match ty {
                Type::Closure(c) => IRType::Closure(c.clone()),
                Type::Adt(r) => IRType::Adt(r.clone()),
                Type::Nullable(box Type::Adt(n)) => IRType::Nullable(n.clone()),
                Type::RawPtr(_) => IRType::RawPtr,
                Type::Any | Type::None => IRType::None,
                _ if ty.is_primitive() => IRType::Primitive,
                _ => IRType::Other,
            },
        }
    }

    pub fn cpy(llvm: T, ty: &IRType) -> Self {
        Self {
            v: llvm,
            ty: ty.clone(),
        }
    }

    pub fn of(llvm: T) -> Self {
        Self {
            v: llvm,
            ty: IRType::Other,
        }
    }
}

impl<T: Copy> Deref for V<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.v
    }
}

impl<T: Copy> DerefMut for V<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.v
    }
}

pub(crate) type LLValue = V<BasicValueEnum>;

impl LLValue {
    pub fn ptr(&self) -> LLPtr {
        LLPtr {
            v: self.v.into_pointer_value(),
            ty: self.ty.clone(),
        }
    }

    pub fn try_ptr(&self) -> Option<LLPtr> {
        match self.v {
            BasicValueEnum::PointerValue(ptr) => Some(LLPtr {
                v: ptr,
                ty: self.ty.clone(),
            }),
            _ => None,
        }
    }

    pub fn into_ptr(self) -> LLPtr {
        LLPtr {
            v: self.v.into_pointer_value(),
            ty: self.ty,
        }
    }
}

pub(crate) type LLPtr = V<PointerValue>;

impl LLPtr {
    pub fn val(&self) -> LLValue {
        LLValue {
            v: self.v.as_basic_value_enum(),
            ty: self.ty.clone(),
        }
    }

    pub fn into_val(self) -> LLValue {
        LLValue {
            v: self.v.as_basic_value_enum(),
            ty: self.ty,
        }
    }
}
