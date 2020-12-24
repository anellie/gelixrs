use crate::{gir_err, Declaration, Function};
use ast::CSTNode;
use common::{mutrc_new, ModulePath, MutRc};
use error::{GErr, Res};
use smol_str::SmolStr;
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

/// A module as represented in GIR.
/// Simplified to a list of declarations.
#[derive(Debug)]
pub struct Module {
    /// All declarations (classes/functions/ifaces) in this module.
    pub declarations: HashMap<SmolStr, Declaration>,
    /// All functions declared.
    /// Defined here additionally allow easily compiling all in IR.
    pub functions: Vec<MutRc<Function>>,

    /// All imports from other modules.
    pub imports: Imports,
    /// All exports from other modules.
    pub exports: Imports,

    /// A list of all global names (classes/interfaces/functions) in this module.
    /// Used to ensure that no naming collision occurs.
    pub used_names: HashSet<SmolStr>,

    pub path: ModulePath,
    pub src: Rc<String>,

    pub ast: Option<ast::Module>,
}

impl Module {
    /// Find a declaration based on name, also looking at imports/exports.
    pub fn find_decl(&self, name: &str) -> Option<Declaration> {
        self.find_import(name).or_else(|| self.imports.get(name))
    }

    /// Find a declaration on name, only checking local or exported declarations.
    pub fn find_import(&self, name: &str) -> Option<Declaration> {
        self.declarations
            .get(name)
            .cloned()
            .or_else(|| self.exports.get(name))
    }

    /// "Borrow" ownership of the AST for temporary use. Return with [return_ast]
    pub fn borrow_ast(&mut self) -> ast::Module {
        self.ast.take().unwrap()
    }

    /// Return the AST after [borrow_ast].
    pub fn return_ast(&mut self, ast: ast::Module) {
        self.ast = Some(ast)
    }

    /// Tries to reserve the given name.
    pub fn try_reserve_name(&mut self, node: &CSTNode, name: &SmolStr) -> Res<()> {
        if !self.used_names.insert(name.clone()) {
            Err(gir_err(node.clone(), GErr::E100(name.clone())))
        } else {
            Ok(())
        }
    }

    /// Create new module from AST, consuming it.
    pub fn new(ast: ast::Module) -> MutRc<Self> {
        mutrc_new(Self {
            declarations: HashMap::with_capacity(10),
            functions: Vec::with_capacity(10),
            imports: Imports::default(),
            exports: Imports::default(),
            used_names: HashSet::with_capacity(10),
            path: Rc::clone(&ast.path),
            src: Rc::clone(&ast.src),
            ast: Some(ast),
        })
    }
}

/// A list of imports inside a module.
#[derive(Default, Debug)]
pub struct Imports {
    pub decls: HashMap<SmolStr, Declaration>,
    pub modules: Vec<MutRc<Module>>,
    pub unresolved: Vec<UnresolvedImport>,
}

impl Imports {
    fn get(&self, name: &str) -> Option<Declaration> {
        self.decls.get(name).cloned().or_else(|| {
            self.modules
                .iter()
                .find_map(|m| m.borrow().find_import(name))
        })
    }
}

/// An import that has not been resolved yet. Used
/// by GIR generator to keep track.
#[derive(Debug)]
pub struct UnresolvedImport {
    pub ast: ast::Import,
    pub module: MutRc<Module>,
    pub symbol: SmolStr,
}
