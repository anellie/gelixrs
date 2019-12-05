/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/5/19 6:24 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::fmt::{Display, Error, Formatter};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::mir::MutRc;
use crate::mir::nodes::{Class, Function, Interface};

/// All types in Gelix.
/// For all types that can have generic parameters, these parameters
/// are not part of the type. They are erased when the type
/// is first created from the prototype.
#[derive(Debug, Clone, EnumAsGetters, EnumIsA)]
pub enum Type {
    /// The Any type is considered equal to all other types.
    /// This is used as the type of 'return' or 'break' expressions,
    /// since they change control flow, preventing their value
    /// from ever being used.
    /// Simply an empty struct in IR.
    Any,
    /// Same behavior as Any, but is equal to no type.
    None,
    /// Simply a boolean type (i1; bit). Required mainly by
    /// if and for expressions.
    Bool,

    /// Signed integer types from 8 to 64 bit width.
    I8,
    I16,
    I32,
    I64,

    /// Floating-point numbers with 32 and 64 bit width.
    F32,
    F64,

    /// A i8 pointer that points to a string literal.
    /// TODO: This will be replaced with a class once
    /// arrays are implemented.
    String,

    /// A function. Note that this is not the signature of a function,
    /// but simply the function directly. Because of this, calling this
    /// a type is kinda questionable, as it is also a value...
    Function(MutRc<Function>),

    /// A class. This type is lowered to a pointer of the underlying struct
    /// in IR.
    Class(MutRc<Class>),

    /// An interface. When used as a standalone type, it gets turned into a
    /// fat pointer (pointer to implementor + pointer to vtable) in IR.
    Interface(MutRc<Interface>),

    /// A generic type. This is only used when compiling/creating prototypes.
    /// They are replaced by their concrete type when building from the prototype.
    /// The int is the offset of the generic type in the definition of the prototype:
    /// Test<T, G> / T is 0; G is 1
    Generic(usize),
}

impl Type {
    /// Is this type an integer?
    pub fn is_int(&self) -> bool {
        match self {
            Type::I8 | Type::I16 | Type::I32 | Type::I64 => true,
            _ => false,
        }
    }

    /// Is this type a floating-point number?
    pub fn is_float(&self) -> bool {
        match self {
            Type::F32 | Type::F64 => true,
            _ => false,
        }
    }
}

impl Default for Type {
    fn default() -> Self {
        Type::None
    }
}

impl PartialEq for Type {
    fn eq(&self, o: &Self) -> bool {
        if let Type::Any = o {
            return true;
        }

        // TODO: Is there really no other way than whatever the heck this is?
        match self {
            Type::Function(f) => {
                if let Type::Function(o) = o { Rc::ptr_eq(f, o) } else { false }
            }

            Type::Class(c) => {
                if let Type::Class(o) = o { Rc::ptr_eq(c, o) } else { false }
            }

            Type::Interface(i) => {
                if let Type::Interface(o) = o { Rc::ptr_eq(i, o) } else { false }
            }

            Type::Generic(g) => {
                if let Type::Generic(o) = o { g == o } else { false }
            }

            Type::Any => true,

            _ => std::mem::discriminant(self) == std::mem::discriminant(o),
        }
    }
}

impl Eq for Type {}

impl Hash for Type {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // TODO: Is there really no other way than whatever the heck this is?
        match self {
            Type::Function(v) => v.borrow().name.hash(state),
            Type::Class(v) => v.borrow().name.hash(state),
            Type::Interface(v) => v.borrow().name.hash(state),
            Type::Generic(g) => g.hash(state),
            _ => std::mem::discriminant(self).hash(state),
        }
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            Type::Function(func) => write!(f, "<func {}>", func.borrow().name),
            Type::Class(class) => write!(f, "{}", class.borrow().name),
            Type::Interface(iface) => write!(f, "{}", iface.borrow().name),
            _ => write!(f, "{:?}", self),
        }
    }
}
