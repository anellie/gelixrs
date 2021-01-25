use crate::{Function, Instance, Module, Type, ADT};
use common::MutRc;
use smol_str::SmolStr;
use std::collections::HashMap;

/// An implementation of an interface.
#[derive(Debug)]
pub struct IFaceImpl {
    pub implementor: Type,
    pub iface: Instance<ADT>,
    pub methods: HashMap<SmolStr, MutRc<Function>>,
    /// Module that the impl block is in.
    pub module: MutRc<Module>,
    pub ast: ast::IfaceImpl,
}

/// A struct representing all interfaces implemented by a type.
/// A simple map of interfaces is not enough, as it does not
/// prevent naming collisions.
#[derive(Debug)]
pub struct IFaceImpls {
    pub implementor: Type,
    /// Key is the implemented interface, value the impl.
    /// Key isn't an interface directly due to needed
    /// Hash and Eq traits that only [Type] implements.
    /// Interface is always a strong reference.
    pub interfaces: HashMap<Type, IFaceImpl>,
    pub methods: HashMap<SmolStr, MutRc<Function>>,
}
