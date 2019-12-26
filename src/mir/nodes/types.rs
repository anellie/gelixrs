/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/26/19 7:56 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::fmt::{Debug, Display, Error, Formatter};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::mir::generator::builder::Context;
use crate::mir::nodes::{Class, Function, Interface, Variable};
use crate::mir::MutRc;

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

    /// A function. Note that this is not the signature of a function,
    /// but simply the function directly. Because of this, calling this
    /// a type is kinda questionable, as it is also a value...
    Function(MutRc<Function>),

    /// A closure. Essentially a function pointer together with
    /// another pointer to a struct containing captured data.
    /// Functions can be cast to closures using Expr::ConstructClosure.
    Closure(Rc<ClosureType>),

    /// An ADT holding all the variables captured by a closure.
    /// Only used as the receiver on closure functions, this anywhere
    /// else is undefined behavior.
    ClosureCaptured(Rc<Vec<Rc<Variable>>>),

    /// A class. This type is lowered to a pointer of the underlying struct
    /// in IR.
    Class(MutRc<Class>),

    /// An interface. When used as a standalone type, it gets turned into a
    /// fat pointer (pointer to implementor + pointer to vtable) in IR.
    Interface(MutRc<Interface>),
}

impl Type {
    /// A list of all primitive types that are not defined in any gelix code,
    /// but are instead indirectly globally defined.
    pub fn primitives() -> [Type; 8] {
        [
            Type::None,
            Type::Bool,
            Type::I8,
            Type::I16,
            Type::I32,
            Type::I64,
            Type::F32,
            Type::F64,
        ]
    }

    /// Returns the context of the type, if any.
    pub fn context(&self) -> Option<Context> {
        Some(match self {
            Type::Class(cls) => cls.borrow().context.clone(),
            Type::Interface(iface) => iface.borrow().context.clone(),
            Type::Function(func) => func.borrow().context.clone(),
            _ => return None,
        })
    }

    /// Is this type a number?
    pub fn is_number(&self) -> bool {
        self.is_int() || self.is_float()
    }

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

    pub fn display_full(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            Type::Function(func) => write!(f, "{}", func.borrow()),
            Type::Class(class) => write!(f, "{}", class.borrow()),
            Type::Interface(iface) => write!(f, "{}", iface.borrow()),
            _ => write!(f, "{:?}", self),
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
                if let Type::Function(o) = o {
                    Rc::ptr_eq(f, o)
                } else {
                    false
                }
            }

            Type::Closure(c) => {
                if let Type::Closure(o) = o {
                    c.parameters == o.parameters && c.ret_type == o.ret_type
                } else {
                    false
                }
            }

            Type::Class(c) => {
                if let Type::Class(o) = o {
                    Rc::ptr_eq(c, o)
                } else {
                    false
                }
            }

            Type::Interface(i) => {
                if let Type::Interface(o) = o {
                    Rc::ptr_eq(i, o)
                } else {
                    false
                }
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
            _ => std::mem::discriminant(self).hash(state),
        }
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            Type::Function(func) => write!(f, "{}", func.borrow().to_closure_type()),
            Type::Closure(closure) => write!(f, "{}", closure),
            Type::Class(class) => write!(f, "{}", class.borrow().name),
            Type::Interface(iface) => write!(f, "{}", iface.borrow().name),
            _ => write!(f, "{:?}", self),
        }
    }
}

#[derive(Debug)]
pub struct ClosureType {
    pub parameters: Vec<Type>,
    pub ret_type: Type,
}

impl Display for ClosureType {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "(")?;
        if !self.parameters.is_empty() {
            let mut p_iter = self.parameters.iter();
            write!(f, "{}", p_iter.next().unwrap())?;
            for ty in p_iter {
                write!(f, ", {}", ty)?;
            }
        }
        write!(f, "): {}", self.ret_type)
    }
}
