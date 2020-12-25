use crate::GIRGenerator;
use common::{ModPath, MutRc};
use error::Errors;
use gir_nodes::{Instance, Module, Type};
use std::rc::Rc;

impl GIRGenerator {
    pub(super) fn populate_intrinsics(&mut self, module: MutRc<Module>) {
        let module = module.borrow();
        if module.path.is(&["std", "ops"]) {
            self.intrinsics.fill_ops_table(module);
        } else if module.path.is(&["std", "string"]) {
            let str_ty = module.find_decl("String").map(|d| d.into_adt()).unwrap();
            self.intrinsics.string_type = Some(Type::StrongRef(Instance::new_(str_ty)))
        } else if module.path.is(&["std", "memory"]) {
            self.intrinsics.free_iface = module.find_decl("Free").map(|d| d.into_adt());
        } else if module.path.is(&["std", "iter"]) {
            self.intrinsics.iter_proto = module.find_decl("Iter").map(|d| d.into_adt());
            self.intrinsics.to_iter_proto = module.find_decl("ToIter").map(|d| d.into_adt());
        }
    }

    pub(super) fn populate_intrinsics_fn(&mut self, module: MutRc<Module>) {
        let module = module.borrow();
        if module.path.is(&["std", "intrinsics"]) {
            self.intrinsics.libc_free = module
                .find_decl(&"free".to_string())
                .map(|d| d.into_function());

            let mut add_fn = |name: &str| {
                self.intrinsics
                    .required_compile_fns
                    .push(module.find_decl(name).map(|d| d.into_function()).unwrap());
            };
            add_fn("free");
            add_fn("malloc");
            add_fn("gelixrs_inc_ref_iface");
            add_fn("gelixrs_dec_ref_iface");
        }
    }

    pub(super) fn validate_intrinsics(&mut self) {
        self.intrinsics
            .validate()
            .map_err(|e| {
                self.errors.borrow_mut().insert(
                    Rc::new(ModPath::new()),
                    Errors {
                        errors: vec![e],
                        src: None,
                        origin: "Compiler".to_string(),
                    },
                )
            })
            .ok();
    }
}
