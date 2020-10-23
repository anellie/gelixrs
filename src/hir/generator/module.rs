use std::rc::Rc;

use crate::{
    ast,
    ast::module::ModulePath,
    error::{Error, Errors},
    hir::{
        generator::{intrinsics::INTRINSICS, HIRGenerator},
        hir_err,
        nodes::{declaration::Declaration, module::Module},
    },
    lexer::token::Token,
    mir::MutRc,
};

/// Generator responsible for compiling the full HIR
/// list of modules as one unit.
pub struct HIRModuleGenerator {
    /// All modules in this compilation run
    pub modules: Vec<MutRc<Module>>,
    /// A single-module generator
    pub generator: HIRGenerator,
}

impl HIRModuleGenerator {
    /// Consumes AST modules given, processing them to HIR.
    /// Returns errors if any occurred.
    pub fn consume(mut self) -> Result<Vec<MutRc<Module>>, Vec<Errors>> {
        Self::reset();

        self.run_ast(Self::declare_adts);
        self.run_mod(Self::populate_intrinsics);
        self.imports(false);
        self.run_ast(Self::declare_iface_impls);
        self.run_ast(Self::declare_functions);
        self.run_mod(Self::populate_intrinsics_fn);
        self.imports(true);

        self.run_dec(HIRGenerator::declare_methods);
        self.generator.primitive_impls();
        self.run_dec(HIRGenerator::fill_impls);
        self.run_dec(HIRGenerator::insert_adt_fields);
        self.run_dec(HIRGenerator::generate);
        self.generator.generate_primitive();
        INTRINSICS
            .with(|i| i.borrow_mut().validate())
            .map_err(|e| {
                self.generator.errors.borrow_mut().insert(
                    Rc::new(ModulePath(vec![])),
                    Errors(vec![e], Rc::new("".to_string())),
                )
            })
            .ok();

        let errs = self
            .generator
            .errors
            .take()
            .into_iter()
            .map(|(k, v)| v)
            .collect::<Vec<_>>();
        if errs.is_empty() {
            Ok(self.modules)
        } else {
            Err(errs)
        }
    }

    /// Create a new error and add it to the list of errors.
    pub fn err(&self, tok: &Token, msg: String) {
        self.generator
            .error(hir_err(tok, msg, &self.generator.path))
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
    fn run_dec<T: FnMut(&mut HIRGenerator, Declaration)>(&mut self, mut runner: T) {
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
                self.generator.resolver.set_context(decl.type_parameters());
                runner(&mut self.generator, decl)
            }
        }
    }

    /// Reset global state and ready for the next compilation.
    fn reset() {}

    /// Create a new generator from AST modules.
    pub fn new(modules: Vec<ast::Module>) -> Self {
        let modules: Vec<_> = modules.into_iter().map(Module::new).collect();
        Self {
            generator: HIRGenerator::new(Rc::clone(&modules[0])),
            modules,
        }
    }
}
