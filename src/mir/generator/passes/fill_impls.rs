/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/19/19 6:45 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem;
use std::rc::Rc;

use either::Either::Left;

use crate::ast::declaration::{Function, FunctionArg};
use crate::error::{Error, Res};
use crate::mir::generator::builder::MIRBuilder;
use crate::mir::generator::passes::declaring_globals::create_function;
use crate::mir::generator::passes::declaring_iface_impls::get_or_create_iface_impls;
use crate::mir::generator::passes::{ModulePass, PassType};
use crate::mir::generator::MIRGenerator;
use crate::mir::nodes::{IFaceMethod, Type, Variable};
use crate::mir::result::ToMIRResult;

/// This pass defines all methods on interface impls.
/// The boolean indicates if the pass has already been run at least once.
pub struct FillIfaceImpls(pub RefCell<bool>);

impl ModulePass for FillIfaceImpls {
    fn get_type(&self) -> PassType {
        PassType::Type
    }

    fn run_type(&self, gen: &mut MIRGenerator, ty: Type) -> Res<()> {
        // If this the first time this pass runs, run it on primitive types.
        // (Since primitive types are not in any module, they would never run if not for this.)
        if *self.0.borrow() {
            mem::replace(&mut *self.0.borrow_mut(), false);
            for ty in Type::primitives().iter() {
                self.run_type(gen, ty.clone())?;
            }
        }

        let impls = get_or_create_iface_impls(&ty);
        let mut impls = impls.borrow_mut();

        let mut methods = HashMap::with_capacity(impls.interfaces.len() * 2);
        for iface_impl in impls.interfaces.values_mut() {
            gen.builder.switch_module(&iface_impl.module);

            let ast = Rc::clone(&iface_impl.ast);
            let iface = Rc::clone(&iface_impl.iface);
            let this_arg = FunctionArg::this_arg_(&ast.implementor);

            for method in ast.methods.iter() {
                let iface = iface.borrow();
                let iface_method = iface.methods.get(&method.sig.name.lexeme).or_err(
                    &gen.builder.path,
                    &method.sig.name,
                    "Method is not defined in interface.",
                )?;

                let mir_method = create_function(
                    &gen.builder,
                    Left(&method.sig),
                    false,
                    Some(this_arg.clone()),
                )?;
                iface_impl
                    .methods
                    .insert(Rc::clone(&method.sig.name.lexeme), Rc::clone(&mir_method));
                if methods.contains_key(&method.sig.name.lexeme) {
                    methods.remove(&method.sig.name.lexeme);
                } else {
                    methods.insert(Rc::clone(&method.sig.name.lexeme), Rc::clone(&mir_method));
                }

                check_equal_signature(&gen.builder, method, mir_method, iface_method)?;
            }

            if iface.borrow().methods.len() > iface_impl.methods.len() {
                return Err(Error::new(
                    &ast.iface.get_token(),
                    "MIR",
                    "Missing methods in interface impl.".to_string(),
                    &gen.builder.path,
                ));
            }
        }

        mem::replace(&mut impls.methods, methods);
        Ok(())
    }
}

/// Ensures that the implemented interface method matches the expected signature.
fn check_equal_signature(
    builder: &MIRBuilder,
    method: &Function,
    mir_method: Rc<Variable>,
    iface_method: &IFaceMethod,
) -> Res<()> {
    let mir_method = mir_method.type_.as_function();
    let mir_method = mir_method.borrow();

    if mir_method.ret_type != iface_method.ret_type {
        let tok = method
            .sig
            .return_type
            .as_ref()
            .map(|t| t.get_token())
            .unwrap_or(&method.sig.name);
        return Err(Error::new(
            tok,
            "MIR",
            "Incorrect return type on interface method.".to_string(),
            &builder.path,
        ));
    }

    for (i, (method_param, iface_param)) in mir_method
        .parameters
        .iter()
        .zip(iface_method.parameters.iter())
        .enumerate()
    {
        if &method_param.type_ != iface_param {
            let tok = &method.sig.parameters[i].name;
            return Err(Error::new(
                tok,
                "MIR",
                format!(
                    "Incorrect parameter type on interface method (Expected {}).",
                    iface_param
                ),
                &builder.path,
            ));
        }
    }

    Ok(())
}
