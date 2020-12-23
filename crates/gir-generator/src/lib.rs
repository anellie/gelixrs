mod intrinsics;

use crate::intrinsics::Intrinsics;
use common::MutRc;
use gir_nodes::{types::IFaceImpls, Module, Type};
use std::collections::HashMap;

/// A struct containing all data produced by GIR compilation.
pub struct CompiledGIR {
    pub modules: Vec<MutRc<Module>>,
    pub intrinsics: Intrinsics,
    pub iface_impls: HashMap<Type, MutRc<IFaceImpls>>,
}
