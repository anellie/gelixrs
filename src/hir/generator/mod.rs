use std::{
    collections::{HashMap, HashSet},
    mem,
    rc::Rc,
};

use crate::{
    ast::module::ModulePath,
    error::{Error, Errors, Res},
    hir::{
        generator::resolver::Resolver,
        hir_err,
        nodes::{
            declaration::{Field, Function, LocalVariable},
            expression::Expr,
            module::Module,
            types::Type,
        },
    },
    lexer::token::Token,
    mir::{mutrc_new, MutRc},
};

mod expr;
pub mod intrinsics;
mod module;
mod passes;
pub mod resolver;

pub type Environment = HashMap<Rc<String>, Rc<LocalVariable>>;

/// A HIR generator scoped to a single module, responsible for
/// compiling expressions and resolving types (latter delegated to resolver).
#[derive(Default)]
pub struct HIRGenerator {
    /// Type resolver
    pub resolver: Resolver,

    /// Current function inserting into
    position: Option<MutRc<Function>>,

    /// An environment is a scope that variables live in.
    /// This field is used like a stack.
    /// See the begin_scope and end_scope functions for more info.
    environments: Vec<Environment>,

    /// The current loop's type, if in one.
    current_loop_ty: Option<Type>,

    /// All class members that are not initialized yet.
    /// This is only used when generating constructors to check
    /// that all constructors don't access uninitialized fields,
    /// and initialize all fields when finished.
    ///
    /// Because of this, calling is_empty() on this set
    /// can be used to determine if 'this' is fully
    /// initialized yet and if methods can be used.
    ///
    /// TODO: The code checking for illegal uninitialized access
    /// does not validate that the object the access occurs on is 'this'.
    /// Because of this, accesses of members on other objects of the same type
    /// (that ARE initialized) will be considered illegal.
    uninitialized_this_fields: HashSet<Rc<Field>>,

    /// Closure-related data, if compiling a closure.
    closure_data: Option<ClosureData>,

    /// Module currently compiling in
    pub module: MutRc<Module>,
    /// Path of [module]
    pub path: Rc<ModulePath>,

    /// Errors produced in the current module
    errors: MutRc<Option<Errors>>,
}

impl HIRGenerator {
    /// Tries to reserve the given name in the current module.
    pub fn try_reserve_name(&self, name: &Token) {
        self.try_reserve_name_rc(&name.lexeme, name)
    }

    /// Tries to reserve the given name in the current module.
    /// Used given token for error reporting.
    pub fn try_reserve_name_rc(&self, name: &Rc<String>, tok: &Token) {
        if !self.module.borrow_mut().used_names.insert(Rc::clone(name)) {
            self.err(tok, format!("Name {} already defined in this module", name))
        }
    }

    /// Defines a new variable. It is put into the variable list in the current function
    /// and placed in the topmost scope.
    fn define_variable(&mut self, name: &Token, mutable: bool, ty: Type) -> Rc<LocalVariable> {
        let def = Rc::new(LocalVariable {
            mutable,
            ty,
            name: name.clone(),
        });
        self.add_function_variable(Rc::clone(&def));
        self.insert_variable(&def, true);
        def
    }

    /// Inserts a variable into the topmost scope.
    /// Note that the variable does NOT get added to the function!
    fn insert_variable(&mut self, var: &Rc<LocalVariable>, allow_redefine: bool) {
        let cur_env = self.environments.last_mut().unwrap();
        let was_defined = cur_env
            .insert(Rc::clone(&var.name.lexeme), Rc::clone(&var))
            .is_some();
        if was_defined && !allow_redefine {
            self.err(
                &var.name,
                format!(
                    "Cannot redefine variable '{}' in the same scope.",
                    &var.name.lexeme
                ),
            );
        }
    }

    /// Will insert the variable into the current function.
    pub fn add_function_variable(&mut self, variable: Rc<LocalVariable>) {
        self.cur_fn()
            .borrow_mut()
            .insert_var(Rc::clone(&variable.name.lexeme), variable);
    }

    /// Creates a new scope. A new scope is created for every function and block,
    /// in addition to the bottom global scope.
    ///
    /// # Example
    /// (global scope #1)
    /// func main() {       <- new scope (#2) for the class main
    ///     var a = 5       <- a now in scope #2
    ///     {               <- new scope (#3)
    ///         var b = 1   <- b now in scope #3
    ///     }               <- scope #3 gets removed, along with b
    /// }                   <- scope #2 gets removed, along with a
    fn begin_scope(&mut self) {
        self.environments.push(HashMap::new());
    }

    /// Removes the topmost scope.
    fn end_scope(&mut self) {
        self.environments.pop();
    }

    /// Inserts the given expression at the current insertion pointer.
    pub fn insert_at_ptr(&mut self, expr: Expr) {
        let func = self.cur_fn();
        let mut func = func.borrow_mut();
        func.exprs.push(expr)
    }

    /// Sets the insertion pointer.
    /// Insertion is always at the end of a function.
    pub fn set_pointer(&mut self, function: MutRc<Function>) {
        self.position = Some(function)
    }

    /// Returns the function of the insertion pointer.
    pub fn cur_fn(&self) -> MutRc<Function> {
        self.position.as_ref().unwrap().clone()
    }

    /// Create new error and add it to the list of errors.
    pub fn err(&self, tok: &Token, msg: String) {
        self.error(hir_err(tok, msg, &self.path))
    }

    /// Add error to the list of errors.
    pub fn error(&self, error: Error) {
        let mut errs = self.errors.borrow_mut();
        if let Some(errs) = errs.as_mut() {
            errs.0.push(error);
        } else {
            errs.replace(Errors(vec![error], Rc::clone(&self.module.borrow().src)));
        }
    }

    /// Switch to compiling a different module, resetting module state.
    pub fn switch_module(&mut self, new: MutRc<Module>) {
        self.module = new;
        self.path = Rc::clone(&self.module.borrow().path);
        self.resolver.switch_module(Rc::clone(&self.module));
        self.environments.clear();
        self.current_loop_ty = None;
        self.position = None;
        self.uninitialized_this_fields.clear();
    }

    /// Create a new generator working on the given module.
    pub fn new(module: MutRc<Module>) -> Self {
        let path = Rc::clone(&module.borrow().path);
        Self {
            path,
            module,
            environments: vec![HashMap::with_capacity(3)],
            errors: mutrc_new(None),
            ..Default::default()
        }
    }

    /// Produces a [HIRGenerator] usable for generating a closure literal,
    /// temporarily making the outer generator unusable.
    /// It takes the outer environments to allow for capturing variables,
    /// and also records some other required closure data.
    /// This data is then retried with `self.end_closure`.
    pub fn for_closure(outer: &mut HIRGenerator) -> Self {
        HIRGenerator {
            closure_data: Some(ClosureData {
                outer_env: mem::replace(&mut outer.environments, vec![]),
                captured: Vec::with_capacity(3),
            }),
            ..Self::new(Rc::clone(&outer.module))
        }
    }

    /// Ends closure compilation and restores the outer generator,
    /// returning recorded info about the compiled closure.
    pub fn end_closure(self, outer: &mut HIRGenerator) -> ClosureData {
        let mut closure_data = self.closure_data.unwrap();
        outer.environments = mem::replace(&mut closure_data.outer_env, vec![]);
        closure_data
    }
}

/// Data required for closure compilation.
pub struct ClosureData {
    /// All environments in the function that the closure literal
    /// is being compiled in.
    pub outer_env: Vec<Environment>,
    /// All variables inside the outer environments that are used
    /// inside the closure and therefore 'captured'
    pub captured: Vec<Rc<LocalVariable>>,
}
