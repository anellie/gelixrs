use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crate::{
    ast::module::ModulePath,
    error::Error,
    hir::nodes::types::{IFaceImpls, Type},
    lexer::token::Token,
    mir::{mutrc_new, MutRc},
};

#[macro_use]
pub mod result;
pub mod generator;
pub mod nodes;

thread_local! {
    /// A map containing all interface implementations.
    /// This is global state since it is shared across modules.
    static IFACE_IMPLS: RefCell<HashMap<Type, MutRc<IFaceImpls>>> = RefCell::new(HashMap::with_capacity(20));
}

// todo remove?
pub fn get_iface_impls(ty: &Type) -> Option<MutRc<IFaceImpls>> {
    IFACE_IMPLS.with(|im| im.borrow().get(ty).cloned())
}

/// Gets the interfaces implemented by a type.
pub fn get_or_create_iface_impls(ty: &Type) -> MutRc<IFaceImpls> {
    match get_iface_impls(ty) {
        Some(impls) => impls,
        None => IFACE_IMPLS.with(|impls| {
            let iface_impls = mutrc_new(IFaceImpls {
                implementor: ty.clone(),
                interfaces: HashMap::with_capacity(2),
                methods: HashMap::with_capacity(2),
            });
            impls
                .borrow_mut()
                .insert(ty.clone(), Rc::clone(&iface_impls));
            iface_impls
        }),
    }
}

/// Produces a new error for the HIR.
pub fn hir_err(tok: &Token, msg: String, path: &Rc<ModulePath>) -> Error {
    Error::new(tok, "HIR", msg, path)
}
