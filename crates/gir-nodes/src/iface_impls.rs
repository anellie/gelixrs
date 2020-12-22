use crate::Type;
use common::{MutRc, mutrc_new};
use crate::types::IFaceImpls;
use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    /// A map containing all interface implementations.
    /// This is global state since it is shared across modules.
    /// TODO: Why the MutRc?
    pub static IFACE_IMPLS: RefCell<HashMap<Type, MutRc<IFaceImpls>>> = RefCell::new(HashMap::with_capacity(20));
}

/// Gets the interfaces implemented by a type.
pub fn get_iface_impls(ty: &Type) -> MutRc<IFaceImpls> {
    let impls = IFACE_IMPLS.with(|im| im.borrow().get(ty).cloned());
    match impls {
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
