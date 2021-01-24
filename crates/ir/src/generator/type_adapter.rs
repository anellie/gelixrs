use gir_nodes::{types::ClosureType, Instance, Type, ADT};
use inkwell::values::{BasicValue, BasicValueEnum, PointerValue};
use std::{
    ops::{Deref, DerefMut},
    rc::Rc,
};

#[derive(Debug, Clone)]
pub(crate) enum IRType {
    None,
    Primitive,
    NullPrimitive,

    ValueRawPtr,
    RefRawPtr,

    ValueAdt(Instance<ADT>),
    RefAdt(Instance<ADT>),
    NullValueAdt(Instance<ADT>),
    NullRefAdt(Instance<ADT>),

    Closure(Rc<ClosureType>),
    Other,
}

pub(crate) fn is_ptr(ty: &Type) -> bool {
    ty.is_ref_adt()
        || match ty {
            Type::Function(_) | Type::Closure(_) | Type::RawPtr(_) => true,
            Type::Nullable(inner) => is_ptr(inner),
            _ => false,
        }
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
                Type::Any | Type::None => IRType::None,
                _ if ty.is_primitive() => IRType::Primitive,
                Type::Nullable(inner) if inner.is_primitive() => IRType::NullPrimitive,

                Type::RawPtr(inner) if is_ptr(inner) => IRType::RefRawPtr,
                Type::RawPtr(_) => IRType::ValueRawPtr,

                Type::Adt(r) if r.ty.borrow().is_ptr() => IRType::RefAdt(r.clone()),
                Type::Adt(r) => IRType::ValueAdt(r.clone()),
                Type::Nullable(box Type::Adt(r)) if r.ty.borrow().is_ptr() => {
                    IRType::NullRefAdt(r.clone())
                }
                Type::Nullable(box Type::Adt(r)) => IRType::NullValueAdt(r.clone()),

                Type::Closure(c) => IRType::Closure(c.clone()),
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
