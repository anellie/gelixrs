use std::rc::Rc;

use crate::{
    ast,
    error::{Error, Errors},
    gir::{
        generator::{
            intrinsics::{Intrinsics, INTRINSICS},
            GIRGenerator,
        },
        gir_err,
        nodes::{declaration::Declaration, module::Module},
        CompiledGIR, MutRc, IFACE_IMPLS,
    },
    lexer::token::Token,
};
use std::collections::HashMap;

/// Generator responsible for compiling the full GIR
/// list of modules as one unit.
pub struct GIRModuleGenerator {
    /// All modules in this compilation run
    pub modules: Vec<MutRc<Module>>,
    /// A single-module generator
    pub generator: GIRGenerator,
}

impl GIRModuleGenerator {
    /// Consumes AST modules given, processing them to GIR.
    /// Returns errors if any occurred.
    pub fn consume(mut self) -> Result<CompiledGIR, Vec<Errors>> {
        self.run_ast(Self::declare_adts);
        self.run_mod(Self::populate_intrinsics);
        self.imports(false);
        self.run_ast(Self::declare_iface_impls);
        self.run_ast(Self::declare_functions);
        self.run_mod(Self::populate_intrinsics_fn);
        self.validate_intrinsics();
        self.imports(true);

        self.run_dec(GIRGenerator::declare_methods);
        self.generator.fill_impls();
        self.run_dec(GIRGenerator::insert_adt_fields);
        self.run_dec(GIRGenerator::constructor_setters);
        self.run_dec(GIRGenerator::generate);
        self.generator.generate_primitive();
        self.run_dec(GIRGenerator::intrinsic_methods);

        let errs = self
            .generator
            .errors
            .take()
            .into_iter()
            .map(|(_, v)| v)
            .collect::<Vec<_>>();
        if errs.is_empty() {
            Ok(CompiledGIR {
                modules: self.modules,
                intrinsics: INTRINSICS.with(|i| i.replace(Intrinsics::default())),
                iface_impls: IFACE_IMPLS.with(|i| i.replace(HashMap::default())),
            })
        } else {
            Err(errs)
        }
    }

    /// Create a new error and add it to the list of errors.
    pub fn err(&self, tok: &Token, msg: String) {
        self.generator
            .error(gir_err(tok, msg, &self.generator.path))
    }

    /// Add the given error to the list of errors.
    pub fn err_(&self, err: Error) {
        self.generator.error(err)
    }

    /// Execute a given module-scope pass.
    pub fn run_mod<T: FnMut(&mut Self, MutRc<Module>)>(&mut self, mut runner: T) {
        for module in self.modules.clone() {
            self.generator.switch_module(Rc::clone(&module));
            runner(self, module)
        }
    }

    /// Execute a given module-scope pass with AST data.
    fn run_ast<T: FnMut(&mut Self, MutRc<Module>, &mut ast::Module)>(&mut self, mut runner: T) {
        for module in self.modules.clone() {
            self.generator.switch_module(Rc::clone(&module));
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
            self.generator.switch_module(module);
            for decl in declarations {
                self.generator.resolver.set_context(&decl.type_parameters());
                runner(&mut self.generator, decl)
            }
        }
    }

    /// Create a new generator from AST modules.
    pub fn new(modules: Vec<ast::Module>) -> Self {
        let modules: Vec<_> = modules.into_iter().map(Module::new).collect();
        Self {
            generator: GIRGenerator::new(Rc::clone(&modules[0])),
            modules,
        }
    }
}
