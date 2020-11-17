use crate::{
    ast::module::ModulePath,
    error::Errors,
    gir::{
        generator::{intrinsics::INTRINSICS, module::GIRModuleGenerator},
        nodes::{
            module::Module,
            types::{Instance, Type},
        },
        MutRc,
    },
};
use std::rc::Rc;

impl GIRModuleGenerator {
    pub fn populate_intrinsics(&mut self, module: MutRc<Module>) {
        let module = module.borrow();
        if module.path.0 == ["std", "ops"] {
            INTRINSICS.with(|i| i.borrow_mut().fill_ops_table(module))
        } else if module.path.0 == ["std", "string"] {
            INTRINSICS.with(|i| {
                i.borrow_mut().string_type = module
                    .find_decl(&"String".to_string())
                    .map(|d| d.into_adt())
                    .map(Instance::new_)
                    .map(Type::Value);
            })
        } else if module.path.0 == ["std", "memory"] {
            INTRINSICS.with(|i| {
                i.borrow_mut().free_iface =
                    module.find_decl(&"Free".to_string()).map(|d| d.into_adt());
            })
        } else if module.path.0 == ["std", "iter"] {
            INTRINSICS.with(|i| {
                i.borrow_mut().iter_proto =
                    module.find_decl(&"Iter".to_string()).map(|d| d.into_adt());
                i.borrow_mut().to_iter_proto = module
                    .find_decl(&"ToIter".to_string())
                    .map(|d| d.into_adt());
            })
        }
    }

    pub fn populate_intrinsics_fn(&mut self, module: MutRc<Module>) {
        let module = module.borrow();
        if module.path.0 == ["std", "intrinsics"] {
            INTRINSICS.with(|i| {
                i.borrow_mut().libc_free = module
                    .find_decl(&"free".to_string())
                    .map(|d| d.into_function());
            })
        }
    }

    pub fn validate_intrinsics(&self) {
        INTRINSICS
            .with(|i| i.borrow_mut().validate())
            .map_err(|e| {
                self.generator.errors.borrow_mut().insert(
                    Rc::new(ModulePath(vec![])),
                    Errors(vec![e], Rc::new("".to_string())),
                )
            })
            .ok();
    }
}
