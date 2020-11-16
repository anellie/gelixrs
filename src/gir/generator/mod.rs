use std::{
    collections::{HashMap, HashSet},
    mem,
    rc::Rc,
};

use crate::{
    ast::{module::ModulePath, Expression},
    error::{Error, Errors, Res},
    gir::{
        generator::{intrinsics::INTRINSICS, resolver::Resolver},
        get_or_create_iface_impls, gir_err, mutrc_new,
        nodes::{
            declaration::{Declaration, Field, Function, LocalVariable, Variable, ADT},
            expression::Expr,
            module::Module,
            types::{Instance, Type},
        },
        result::EmitGIRError,
        MutRc,
    },
    lexer::token::{TType, Token},
};
use either::Either;
use smol_str::SmolStr;

mod expr;
pub mod intrinsics;
pub mod module;
mod passes;
pub mod resolver;
pub mod visitors;

pub type Environment = HashMap<SmolStr, Rc<LocalVariable>>;

/// A GIR generator scoped to a single module, responsible for
/// compiling expressions and resolving types (latter delegated to resolver).
#[derive(Default)]
pub struct GIRGenerator {
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

    /// Errors produced
    errors: MutRc<HashMap<Rc<ModulePath>, Errors>>,
}

impl GIRGenerator {
    /// Tries to reserve the given name in the current module.
    /// Be warned that this will borrow mutably!
    pub fn try_reserve_name(&self, name: &Token) {
        self.module.borrow_mut().try_reserve_name(self, name)
    }

    /// Tries to reserve the given name in the current module.
    /// Uses given token for error reporting.
    /// Be warned that this will borrow mutably!
    pub fn try_reserve_name_rc(&self, name: &SmolStr, tok: &Token) {
        self.module
            .borrow_mut()
            .try_reserve_name_rc(self, name, tok)
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
            .insert(var.name.lexeme.clone(), Rc::clone(&var))
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
            .insert_var(variable.name.lexeme.clone(), variable);
    }

    /// Returns the method that corresponds to the operator given (operator overloading).
    /// Returns None if the given class does not implement the operator.
    fn get_operator_overloading_method(
        &self,
        op: TType,
        left: &mut Expr,
        right: &mut Expr,
    ) -> Option<Instance<Function>> {
        let left_ty = left.get_type();
        let interface = INTRINSICS.with(|i| i.borrow().get_op_iface(op))?;

        if let Some(adt) = left_ty.try_adt() {
            // If this is an ADT, also make sure that possible implementations
            // for other representations are checked and casted to
            self.get_op_method(&interface, &left_ty, right)
                .or_else(|| {
                    self.get_op_method(&interface, &Type::WeakRef(adt.clone()), right)
                        .map(|var| {
                            self.resolver
                                .try_cast_in_place(left, &Type::WeakRef(adt.clone()));
                            var
                        })
                })
                .or_else(|| {
                    self.get_op_method(&interface, &Type::Value(adt.clone()), right)
                        .map(|var| {
                            self.resolver
                                .try_cast_in_place(left, &Type::Value(adt.clone()));
                            var
                        })
                })
        } else {
            self.get_op_method(&interface, &left_ty, right)
        }
    }

    /// Tries finding a fitting method for an operator overload,
    /// given the overloading interface, type of the implementor and
    /// the expression of the right value (to allow casting it if needed)
    fn get_op_method(
        &self,
        interface: &MutRc<ADT>,
        ty: &Type,
        right: &mut Expr,
    ) -> Option<Instance<Function>> {
        let iface_impls = get_or_create_iface_impls(ty);
        let iface_impls = iface_impls.borrow();

        for im in iface_impls.interfaces.values() {
            if Rc::ptr_eq(&im.iface.ty, interface) {
                let method = im.methods.values().next().unwrap();
                let ty = &method.borrow().parameters[1].ty;
                self.resolver.try_cast_in_place(right, ty);
                if *ty == right.get_type() {
                    // TODO: Type arguments
                    return Some(Instance::new_(Rc::clone(method)));
                }
            }
        }
        None
    }

    /// Searches all scopes for a variable, starting at the top.
    fn find_var(&mut self, token: &Token) -> Res<Variable> {
        self.find_local_var(token)
            .map(Variable::Local)
            .or_else(|e| self.find_global_var(token).ok_or(e))
    }

    /// Searches for a local variable.
    fn find_local_var(&mut self, token: &Token) -> Res<Rc<LocalVariable>> {
        for env in self.environments.iter().rev() {
            if let Some(var) = env.get(&token.lexeme) {
                return Ok(Rc::clone(var));
            }
        }

        if let Some(closure_data) = &mut self.closure_data {
            for env in closure_data.outer_env.iter().rev() {
                if let Some(var) = env.get(&token.lexeme) {
                    if !var.ty.can_escape() {
                        gir_err(
                            &token,
                            "This variable may not be captured (weak reference)".to_string(),
                            &self.path,
                        );
                    }
                    closure_data.captured.push(Rc::clone(var));
                    return Ok(Rc::clone(var));
                }
            }
        }
        Err(self.err_(token, format!("Variable '{}' is not defined", token.lexeme)))
    }

    fn find_global_var(&self, token: &Token) -> Option<Variable> {
        let decl = self.module.borrow().find_decl(&token.lexeme)?;
        match decl {
            // TODO: Reject if missing params
            Declaration::Function(func) => Some(Variable::Function(Instance::new_(func))),
            _ => None,
        }
    }

    /// Returns the variable of the current loop or creates it if it does not exist yet.
    /// This variable stores the value of the last loop iteration.
    fn set_loop_type(&mut self, type_: &Type) {
        match &self.current_loop_ty {
            Some(ty) if ty != type_ => {
                self.err(
                    // todo!!
                    &Token::generic_token(TType::Break),
                    "Break expressions and for body must have same type".to_string(),
                )
            }

            None | Some(Type::Any) => self.current_loop_ty = Some(type_.clone()),
            _ => (),
        }
    }

    /// Returns a field of the given expression/object,
    /// where a field can be either a member or a method.
    fn get_field(
        &mut self,
        object: &Expression,
        name: &Token,
    ) -> Res<(Expr, Either<Rc<Field>, MutRc<Function>>)> {
        let object = self.expression(object);
        let ty = object.get_type();

        if let Some(adt) = ty.try_adt() {
            let adt = adt.ty.borrow();
            let field = adt.fields.get(&name.lexeme);
            if let Some(field) = field {
                return Ok((object, Either::Left(Rc::clone(field))));
            }
        }

        Self::find_associated_method(&ty, name)
            .map(|m| (object, Either::Right(m)))
            .on_err(&self.path, name, "Unknown field or method.")
    }

    /// Searches for an associated method on a type. Can be either an interface
    /// method or a class method.
    fn find_associated_method(ty: &Type, name: &Token) -> Option<MutRc<Function>> {
        let method = if let Some(adt) = ty.try_adt() {
            let adt = adt.ty.borrow();
            adt.methods.get(&name.lexeme).cloned()
        } else {
            None
        };

        method.or_else(|| {
            get_or_create_iface_impls(ty)
                .borrow()
                .methods
                .get(&name.lexeme)
                .cloned()
        })
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
        self.error(gir_err(tok, msg, &self.path))
    }

    /// Create new error. Does not get added to errors!
    pub fn err_(&self, tok: &Token, msg: String) -> Error {
        gir_err(tok, msg, &self.path)
    }

    /// Add error to the list of errors.
    pub fn error(&self, error: Error) {
        self.error_(error, &self.module.borrow())
    }

    pub fn error_(&self, error: Error, module: &Module) {
        let mut errs = self.errors.borrow_mut();
        if let Some(errs) = errs.get_mut(&self.path) {
            errs.0.push(error);
        } else {
            errs.insert(
                Rc::clone(&self.path),
                Errors(vec![error], Rc::clone(&module.src)),
            );
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

    pub fn eat<T>(&self, res: Res<T>) -> Option<T> {
        match res {
            Ok(i) => Some(i),
            Err(e) => {
                self.error(e);
                None
            }
        }
    }

    /// Create a new generator working on the given module.
    pub fn new(module: MutRc<Module>) -> Self {
        let path = Rc::clone(&module.borrow().path);
        Self {
            path,
            module,
            environments: vec![HashMap::with_capacity(3)],
            errors: mutrc_new(HashMap::new()),
            ..Default::default()
        }
    }

    /// Produces a [GIRGenerator] usable for generating a closure literal,
    /// temporarily making the outer generator unusable.
    /// It takes the outer environments to allow for capturing variables,
    /// and also records some other required closure data.
    /// This data is then retried with `self.end_closure`.
    pub fn for_closure(outer: &mut GIRGenerator) -> Self {
        GIRGenerator {
            closure_data: Some(ClosureData {
                outer_env: mem::replace(&mut outer.environments, vec![]),
                captured: Vec::with_capacity(3),
            }),
            ..Self::new(Rc::clone(&outer.module))
        }
    }

    /// Ends closure compilation and restores the outer generator,
    /// returning recorded info about the compiled closure.
    pub fn end_closure(self, outer: &mut GIRGenerator) -> ClosureData {
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
