use crate::GIRGenerator;
use common::MutRc;
use gir_nodes::{Declaration, Module, ADT};
use std::rc::Rc;

// mod declare;
// mod fields;
// mod generate;
// mod import;
// mod intrinsic_methods;
// mod intrinsics;
// mod methods;

impl GIRGenerator {
    pub fn run_passes(&self) {
        // self.run_ast(Self::declare_adts);
        // self.run_mod(Self::populate_intrinsics);
        // self.imports(false);
        // self.run_ast(Self::declare_iface_impls);
        // self.run_ast(Self::declare_functions);
        // self.run_mod(Self::populate_intrinsics_fn);
        // self.validate_intrinsics();
        // self.imports(true);

        // self.run_adt(GIRGenerator::declare_methods);
        // self.generator.fill_impls();
        // self.run_dec(GIRGenerator::insert_adt_fields);
        // self.run_adt(GIRGenerator::constructor_setters);
        // self.run_adt(GIRGenerator::declare_lifecycle_methods);
        // self.run_adt(GIRGenerator::generate_lifecycle_methods);
        // self.run_dec(GIRGenerator::generate);
        // self.generator.generate_impls();
    }

    /// Execute a given module-scope pass.
    pub fn run_mod<T: FnMut(&mut Self, MutRc<Module>)>(&mut self, mut runner: T) {
        for module in self.modules.clone() {
            self.switch_module(Rc::clone(&module));
            runner(self, module)
        }
    }

    /// Execute a given module-scope pass with AST data.
    fn run_ast<T: FnMut(&mut Self, MutRc<Module>, &mut ast::Module)>(&mut self, mut runner: T) {
        for module in self.modules.clone() {
            self.switch_module(Rc::clone(&module));
            let mut ast = module.borrow_mut().borrow_ast();
            runner(self, Rc::clone(&module), &mut ast);
            module.borrow_mut().return_ast(ast);
        }
    }

    /// Execute a given declaration-scope pass.
    fn run_dec<T: FnMut(&mut GIRGenerator, Declaration)>(&mut self, mut runner: T) {
        let declarations = self
            .modules
            .iter()
            .map(|module| {
                (
                    module
                        .borrow()
                        .declarations
                        .values()
                        .cloned()
                        .collect::<Vec<_>>(),
                    Rc::clone(module),
                )
            })
            .collect::<Vec<_>>();

        for (declarations, module) in declarations.into_iter() {
            self.switch_module(module);
            for decl in declarations {
                self.set_context(&decl.type_parameters());
                runner(self, decl)
            }
        }
    }

    fn run_adt<T: FnMut(&mut GIRGenerator, &MutRc<ADT>)>(&mut self, mut runner: T) {
        self.run_dec(|this, dec| {
            if let Declaration::Adt(adt) = &dec {
                runner(this, adt)
            }
        })
    }
}
