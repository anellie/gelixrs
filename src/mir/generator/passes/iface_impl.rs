/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/14/19 6:14 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast::declaration::{Function, FunctionArg, IFaceImpl};
use crate::ast::module::Module;
use crate::mir::{IFACE_IMPLS, MutRc, mutrc_new, ToMIRResult};
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::generator::passes::declare_func::create_function;
use crate::mir::nodes::{
    IFaceImpl as MIRImpl, IFaceImpls, IFaceMethod, Type, Variable,
};
use crate::option::Flatten;

/// This pass checks and defines all interface impl blocks.
pub fn iface_impl_pass(gen: &mut MIRGenerator, list: &mut Module) -> Res<()> {
    // Remove all impls where the implementor contains a generic
    // argument; Tack those to the prototype instead.
    for iface in list.iface_impls.drain_filter(|i| i.implementor.is_generic()) {
        let class = gen.builder
            .find_class_or_proto(&iface.implementor.get_token().unwrap().lexeme)
            .map(|o| o.right())
            .flatten_()
            .or_type_err(gen, &Some(iface.implementor.clone()), "Unknown prototype.")?;
        class.borrow_mut().impls.push(iface);
    }

    for iface in list.iface_impls.iter_mut() {
        iface_impl(gen, iface, None)?;
    }

    Ok(())
}

pub fn iface_impl(gen: &mut MIRGenerator, iface_impl: &mut IFaceImpl, override_implementor: Option<Type>) -> Res<()> {
    let implementor = if let Some(i) = override_implementor { i } else { gen.find_type(&iface_impl.implementor)? };
    let iface_impls = get_or_create_iface_impls(&implementor);

    let iface = gen.find_type(&iface_impl.iface)?;
    let iface = match iface {
        Type::Interface(iface) => iface,
        _ => return None.or_type_err(gen, &Some(iface_impl.iface.clone()), "Not an interface.")
    };

    let mut mir_impl = MIRImpl {
        implementor: implementor.clone(),
        iface: Rc::clone(&iface),
        methods: IndexMap::with_capacity(iface.borrow().methods.len()),
    };

    for method in iface_impl.methods.iter_mut() {
        let iface = iface.borrow();
        let iface_method = iface.methods.get(&method.sig.name.lexeme).or_err(
            gen,
            &method.sig.name,
            "Method is not defined in interface.",
        )?;
        let this_arg = FunctionArg::this_arg_(&iface_impl.implementor);

        let old_name = Rc::clone(&method.sig.name.lexeme);
        method.sig.name.lexeme = Rc::new(format!(
            "{}-{}",
            implementor, method.sig.name.lexeme
        ));
        method.sig.parameters.insert(0, this_arg);

        let mir_method = create_function(gen, &method.sig, false)?;
        mir_impl
            .methods
            .insert(Rc::clone(&old_name), Rc::clone(&mir_method));
        if iface_impls.borrow().methods.contains_key(&old_name) {
            iface_impls.borrow_mut().methods.remove(&old_name);
        } else {
            iface_impls
                .borrow_mut()
                .methods
                .insert(old_name, Rc::clone(&mir_method));
        }

        check_equal_signature(gen, method, mir_method, iface_method)?;
    }

    if iface.borrow().methods.len() > iface_impl.methods.len() {
        Err(gen.anon_err(
            iface_impl.iface.get_token(),
            "Missing methods in interface impl.",
        ))
    } else {
        if iface_impls.borrow_mut().interfaces.insert(mir_impl) {
            Ok(())
        } else {
            Err(gen.anon_err(iface_impl.iface.get_token(), "Interface already implemented for type."))
        }
    }
}

/// Gets the interfaces implemented by a type.
fn get_or_create_iface_impls(ty: &Type) -> MutRc<IFaceImpls> {
    match IFACE_IMPLS.with(|impls| impls.borrow().get(ty).cloned()) {
        Some(impls) => impls,
        None => {
            IFACE_IMPLS.with(|impls| {
                let iface_impls = mutrc_new(IFaceImpls {
                    implementor: ty.clone(),
                    interfaces: HashSet::with_capacity(2),
                    methods: HashMap::with_capacity(2),
                });
                impls.borrow_mut().insert(
                    ty.clone(),
                    Rc::clone(&iface_impls),
                );
                iface_impls
            })
        }
    }
}

/// Ensures that the implemented interface method matches the expected signature.
fn check_equal_signature(
    gen: &mut MIRGenerator,
    method: &Function,
    mir_method: Rc<Variable>,
    iface_method: &IFaceMethod,
) -> Res<()> {
    let mir_method = &mir_method.type_.as_function();
    let mir_method = mir_method.borrow();

    if mir_method.ret_type != iface_method.ret_type {
        let tok = method
            .sig
            .return_type
            .as_ref()
            .map(|t| t.get_token())
            .flatten_();
        return Err(gen.anon_err(tok, "Incorrect return type on interface method."));
    }

    for (i, (method_param, iface_param)) in mir_method
        .parameters
        .iter()
        .zip(iface_method.parameters.iter())
        .enumerate()
    {
        if &method_param.type_ != iface_param {
            let tok = &method.sig.parameters[i].name;
            return Err(gen.error(
                tok,
                tok,
                &format!(
                    "Incorrect parameter type on interface method (Expected {}).",
                    iface_param
                ),
            ));
        }
    }

    Ok(())
}
