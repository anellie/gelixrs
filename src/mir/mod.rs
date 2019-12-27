/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 6:53 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use crate::{
    ast::{module::ModulePath, Import, Module},
    error::{Error, Res},
    lexer::token::Token,
    mir::nodes::{IFaceImpls, Prototype, Type, Variable},
};
use std::{fmt, fmt::Display};

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

pub fn get_iface_impls(ty: &Type) -> Option<MutRc<IFaceImpls>> {
    IFACE_IMPLS.with(|im| im.borrow().get(ty).cloned())
}

pub type MutRc<T> = Rc<RefCell<T>>;

fn mutrc_new<T>(value: T) -> MutRc<T> {
    Rc::new(RefCell::new(value))
}

/// A struct produced by a generator. It contains the full MIR representation of a file/module.
/// Note that generic types/prototypes are not included in this; only instantiated copies of them are.
#[derive(Debug, Default)]
pub struct MModule {
    /// The path of the module, for example my_app/gui/widgets
    pub path: Rc<ModulePath>,

    /// The source code of this module.
    pub src: Rc<String>,

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
    pub fn find_type(&self, name: &String) -> Option<Type> {
        self.types
            .get(name)
            .or_else(|| self.imports.types.get(name))
            .cloned()
            .or_else(|| {
                self.imports
                    .modules
                    .iter()
                    .find_map(|(_, m)| m.borrow().types.get(name).cloned())
            })
    }

    pub fn find_prototype(&self, name: &String) -> Option<Rc<Prototype>> {
        self.protos
            .get(name)
            .or_else(|| self.imports.protos.get(name))
            .cloned()
            .or_else(|| {
                self.imports
                    .modules
                    .iter()
                    .find_map(|(_, m)| m.borrow().protos.get(name).cloned())
            })
    }

    pub fn find_global(&self, name: &String) -> Option<Rc<Variable>> {
        self.globals
            .get(name)
            .or_else(|| self.imports.globals.get(name))
            .cloned()
            .or_else(|| {
                self.imports
                    .modules
                    .iter()
                    .find_map(|(_, m)| m.borrow().globals.get(name).cloned())
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

    pub fn new(ast: &Module) -> MModule {
        Self {
            path: Rc::clone(&ast.path),
            src: Rc::clone(&ast.src),
            ..Default::default()
        }
    }
}

impl Display for MModule {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        writeln!(f, "--> {}:", self.path)?;
        writeln!(f, "Used names: ")?;
        for name in self.used_names.iter() {
            writeln!(f, "{} ", name)?;
        }
        writeln!(f, "\n\n")?;
        for ty in self
            .types
            .values()
            .filter(|t| !(t.is_function() && t.as_function().borrow().name.contains('-')))
        {
            ty.display_full(f)?;
            writeln!(f)?;
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct Imports {
    pub modules: HashMap<Rc<ModulePath>, MutRc<MModule>>,
    pub globals: HashMap<Rc<String>, Rc<Variable>>,
    pub types: HashMap<Rc<String>, Type>,
    pub protos: Prototypes,
    pub ast: Vec<Import>,
}

/// A list of all prototypes.
/// Prototypes are things with generic parameters, that are
/// turned into a concrete implementation when used with
/// generic arguments.
pub type Prototypes = HashMap<Rc<String>, Rc<Prototype>>;
