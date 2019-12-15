/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/4/19 9:55 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use nodes::Variable;

use crate::lexer::token::Token;
use crate::mir::nodes::{IFaceImpls, Type, Prototype};
use crate::error::{Res, Error};
use crate::ast::module::ModulePath;

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
    pub fn find_type(&self, name: &Rc<String>) -> Option<&Type> {
        self.types.get(name)
            .or_else(|| self.imports.types.get(name))
            .or_else(|| self.imports.modules.iter().find_map(|(_, m)| m.borrow().find_type(name)))
    }

    pub fn find_prototype(&self, name: &Rc<String>) -> Option<MutRc<dyn Prototype>> {
        self.protos.get(name)
            .or_else(|| self.imports.protos.get(name))
            .cloned()
            .or_else(|| self.imports.modules.iter().find_map(|(_, m)| m.borrow().find_prototype(name)))
    }

    pub fn find_global(&self, name: &Rc<String>) -> Option<Rc<Variable>> {
        self.globals.get(name)
            .or_else(|| self.imports.globals.get(name))
            .cloned()
            .or_else(|| self.imports.modules.iter().find_map(|(_, m)| m.borrow().find_global(name)))
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
                &self.path
            ))
        }
    }

    pub fn new(path: Rc<ModulePath>) -> MModule {
        Self {
            path,
            ..Default::default()
        }
    }
}

#[derive(Default)]
pub struct Imports {
    pub modules: HashMap<Rc<ModulePath>, MutRc<MModule>>,
    pub globals: HashMap<Rc<String>, Rc<Variable>>,
    pub types: HashMap<Rc<String>, Type>,
    pub protos: Prototypes
}

/// A list of all prototypes.
/// Prototypes are things with generic parameters, that are
/// turned into a concrete implementation when used with
/// generic arguments.
/// Note that the prototypes in the builder also include imported
/// prototypes, since they do not end up in the final module either way.
pub type Prototypes = HashMap<Rc<String>, MutRc<dyn Prototype>>;