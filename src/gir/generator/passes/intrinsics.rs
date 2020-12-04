use crate::{
    ast::module::ModulePath,
    error::Errors,
    gir::{
        generator::{
            intrinsics::{Intrinsics, INTRINSICS},
            module::GIRModuleGenerator,
        },
        nodes::{
            module::Module,
            types::{Instance, Type},
        },
        MutRc,
    },
};
use std::{cell::RefCell, rc::Rc};

impl GIRModuleGenerator {
    pub fn populate_intrinsics(&mut self, module: MutRc<Module>) {
        let module = module.borrow();
        if module.path.0 == ["std", "ops"] {
            INTRINSICS.with(|i| i.borrow_mut().fill_ops_table(module))
        } else if module.path.0 == ["std", "string"] {
            INTRINSICS.with(|i| {
                let str_ty = module.find_decl("String").map(|d| d.into_adt()).unwrap();
                i.borrow_mut().string_type = Some(Type::StrongRef(Instance::new_(str_ty)))
            })
        } else if module.path.0 == ["std", "memory"] {
            INTRINSICS.with(|i| {
                i.borrow_mut().free_iface = module.find_decl("Free").map(|d| d.into_adt());
            })
        } else if module.path.0 == ["std", "iter"] {
            INTRINSICS.with(|i| {
                i.borrow_mut().iter_proto = module.find_decl("Iter").map(|d| d.into_adt());
                i.borrow_mut().to_iter_proto = module.find_decl("ToIter").map(|d| d.into_adt());
            })
        }
    }

    pub fn populate_intrinsics_fn(&mut self, module: MutRc<Module>) {
        let module = module.borrow();

        let add_fn = |i: &RefCell<Intrinsics>, name: &str| {
            i.borrow_mut()
                .required_compile_fns
                .push(module.find_decl(name).map(|d| d.into_function()).unwrap());
        };

        if module.path.0 == ["std", "intrinsics"] {
            INTRINSICS.with(|i| {
                i.borrow_mut().libc_free = module
                    .find_decl(&"free".to_string())
                    .map(|d| d.into_function());

                add_fn(i, "free");
                add_fn(i, "malloc");
                add_fn(i, "gelixrs_inc_ref_iface");
                add_fn(i, "gelixrs_dec_ref_iface");
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
