/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 4:19 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use nodes::Variable;

use crate::ast::Module;
use crate::ast::module::ModulePath;
use crate::error::{Error, Res};
use crate::lexer::token::Token;
use crate::mir::nodes::{IFaceImpls, Prototype, Type};

pub mod generator;
pub mod nodes;
pub mod result;

thread_local! {
    /// A map containing all interface implementations.
    /// This is global state since it is shared across modules.
    /// TODO: This would be better implemented as a lazy_static,
    /// but the compiler does not currently support multithreading.
    static IFACE_IMPLS: RefCell<HashMap<Type, MutRc<IFaceImpls>>> = RefCell::new(HashMap::with_capacity(20));
}

pub type MutRc<T> = Rc<RefCell<T>>;

fn mutrc_new<T>(value: T) -> MutRc<T> {
    Rc::new(RefCell::new(value))
}

/// A struct produced by a generator. It contains the full MIR representation of a file/module.
/// Note that generic types/prototypes are not included in this; only instantiated copies of them are.
#[derive(Default)]
pub struct MModule {
    /// The path of the module, for example my_app/gui/widgets
    pub path: Rc<ModulePath>,

    /// The AST module this module was produced from.
    pub ast: Module,

    /// The 'stage' the module is on. This indicates how
    /// far along compilation is.
    pub stage: ModuleStage,

    /// All global variables: Currently only functions.
    pub globals: HashMap<Rc<String>, Rc<Variable>>,

    /// All types (classes/functions/ifaces) in this module.
    pub types: HashMap<Rc<String>, Type>,

    /// All imports from other modules.
    pub imports: Imports,

    /// All prototypes.
    pub protos: Prototypes,

    /// A list of all global names (classes/interfaces/functions) in this module.
    /// Used to ensure that no naming collision occurs.
    used_names: HashSet<Rc<String>>,
}

impl MModule {
    pub fn find_type(&self, name: &Rc<String>) -> Option<Type> {
        self.types
            .get(name)
            .or_else(|| self.imports.types.get(name))
            .cloned()
            .or_else(|| {
                self.imports
                    .modules
                    .iter()
                    .find_map(|(_, m)| m.borrow().find_type(name))
            })
    }

    pub fn find_prototype(&self, name: &Rc<String>) -> Option<MutRc<dyn Prototype>> {
        self.protos
            .get(name)
            .or_else(|| self.imports.protos.get(name))
            .cloned()
            .or_else(|| {
                self.imports
                    .modules
                    .iter()
                    .find_map(|(_, m)| m.borrow().find_prototype(name))
            })
    }

    pub fn find_global(&self, name: &Rc<String>) -> Option<Rc<Variable>> {
        self.globals
            .get(name)
            .or_else(|| self.imports.globals.get(name))
            .cloned()
            .or_else(|| {
                self.imports
                    .modules
                    .iter()
                    .find_map(|(_, m)| m.borrow().find_global(name))
            })
    }

    /// Tries to reserve the given name. If the name is already used, returns an error.
    pub fn try_reserve_name(&mut self, name: &Token) -> Res<()> {
        self.try_reserve_name_rc(&name.lexeme, name)
    }

    pub fn try_reserve_name_rc(&mut self, name: &Rc<String>, tok: &Token) -> Res<()> {
        if self.used_names.insert(Rc::clone(name)) {
            Ok(())
        } else {
            Err(Error::new(
                tok,
                "MIR",
                format!("Name {} already defined in this module", name),
                &self.path,
            ))
        }
    }

    pub fn new(ast: Module) -> MModule {
        Self {
            path: Rc::clone(&ast.path),
            ast,
            ..Default::default()
        }
    }
}

#[derive(Debug)]
pub enum ModuleStage {
    /// No compilation milestone has been reached
    None,
    /// All types in the module, including prototypes, have been declared.
    /// It is possible to accurately search for types once this is reached.
    TypesDeclared,
    /// All globals have been declared. It is possible to generate
    /// a limited set of code at this stage.
    GlobalsDeclared,
    /// All types have been filled with their members/methods.
    /// It is possible to generate code after this has been reached.
    TypesFilled,
    /// The module is currently being modified by a MIRGenerator, code
    /// is being produced.
    Generating,
}

impl Default for ModuleStage {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Default)]
pub struct Imports {
    pub modules: HashMap<Rc<ModulePath>, MutRc<MModule>>,
    pub globals: HashMap<Rc<String>, Rc<Variable>>,
    pub types: HashMap<Rc<String>, Type>,
    pub protos: Prototypes,
}

/// A list of all prototypes.
/// Prototypes are things with generic parameters, that are
/// turned into a concrete implementation when used with
/// generic arguments.
pub type Prototypes = HashMap<Rc<String>, MutRc<dyn Prototype>>;
