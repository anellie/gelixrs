#![feature(try_find)]
#![feature(box_syntax)]
#![feature(box_patterns)]
// Often required due to clones; also false positives from type aliases
#![allow(clippy::ptr_arg)]

use crate::intrinsics::Intrinsics;
use common::{bench, mutrc_new, ModulePath, MutRc};
use gir_nodes::{
    declaration::Visibility,
    expression::ConcreteMethodGet,
    gir_err,
    types::{ToInstance, TypeParameterBound, TypeVariable},
    Declaration, Expr, Function, IFaceImpls, Instance, Module, Type, ADT,
};
use result::EmitGIRError;
use std::{
    collections::{HashMap, HashSet},
    mem,
    rc::Rc,
};
use syntax::kind::SyntaxKind;

use ast::{CSTNode, Get};
use error::{Error, Errors, GErr, Res};
use gir_nodes::{
    declaration::{Field, LocalVariable, Variable},
    types::TypeParameters,
};
use smol_str::SmolStr;

mod expr;
mod intrinsics;
mod passes;
mod resolver;
mod result;
mod types;

/// A struct containing all data produced by GIR compilation.
pub struct CompiledGIR {
    pub modules: Vec<MutRc<Module>>,
    pub intrinsics: Intrinsics,
    pub iface_impls: HashMap<Type, MutRc<IFaceImpls>>,
}

/// A struct containing various compiler flags
/// that disable or enable certain features.
#[derive(Default, Copy, Clone)]
pub struct GIRFlags {
    /// The standard library was already compiled ahead of time.
    /// This skips intrinsic passes, which aren't required
    /// when std is already compiled.
    pub cached_std: bool,

    /// If this compilation run does not contain a binary.
    /// If true, a main function will not be required.
    pub library: bool,

    /// This compilation run does not use the standard library.
    /// This will disable quite a few features and is quite buggy.
    /// Mainly intended for debugging, WIP.
    pub no_std: bool,

    /// Do not import the prelude into every module. no_std requires this.
    pub no_prelude: bool,
}

type Environment = HashMap<SmolStr, Rc<LocalVariable>>;

/// A GIR generator, responsible for compiling GIR.
pub struct GIRGenerator {
    /// Current function inserting into
    position: Option<MutRc<Function>>,
    /// Current impl type, if inside a method
    ty_position: Option<Type>,

    /// An environment is a scope that variables live in.
    /// This field is used like a stack.
    /// See the begin_scope and end_scope functions for more info.
    environments: Vec<Environment>,

    /// Type parameters of currently compiling declaration
    type_params: Option<Rc<TypeParameters>>,

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
    uninitialized_this_fields: HashSet<Rc<Field>>,

    /// Closure-related data, if compiling a closure.
    closure_data: Option<ClosureData>,

    /// Module currently compiling in
    module: MutRc<Module>,
    /// Path of [module]
    path: ModulePath,
    /// All modules.
    modules: Vec<MutRc<Module>>,
    /// All modules that weren't compiled yet that are to be compiled.
    modules_uncompiled: Vec<MutRc<Module>>,

    /// Intrinsics info.
    intrinsics: Intrinsics,
    /// Interface implementations.
    iface_impls: HashMap<Type, MutRc<IFaceImpls>>,

    /// Errors produced
    errors: MutRc<HashMap<ModulePath, Errors>>,

    flags: GIRFlags,
}

impl GIRGenerator {
    /// Consumes AST modules given, processing them to GIR.
    /// Returns errors if any occurred.
    pub fn consume(mut self) -> Result<CompiledGIR, Vec<Errors>> {
        self.run_passes();

        for module in &self.modules_uncompiled {
            module.borrow_mut().compiled = true;
        }

        let errs = self
            .errors
            .take()
            .into_iter()
            .map(|(_, v)| v)
            .collect::<Vec<_>>();
        if errs.is_empty() {
            Ok(CompiledGIR {
                modules: self.modules,
                intrinsics: self.intrinsics,
                iface_impls: self.iface_impls,
            })
        } else {
            Err(errs)
        }
    }

    /// Tries to reserve the given name in the current module.
    /// Be warned that this will borrow mutably!
    /// Will eat error if any occurs.
    fn try_reserve_name(&self, node: &CSTNode, name: &SmolStr) {
        let res = self.module.borrow_mut().try_reserve_name(node, name);
        self.eat(res);
    }

    /// Defines a new variable. It is put into the variable list in the current function
    /// and placed in the topmost scope.
    fn define_variable(&mut self, ast: ast::Variable, ty: Type) -> Rc<LocalVariable> {
        let def = LocalVariable {
            name: ast.name(),
            mutable: ast.mutable(),
            ty,
        };
        self.define_variable_(def, Some(&ast.cst))
    }

    /// Defines a new variable. It is put into the variable list in the current function
    /// and placed in the topmost scope.
    /// This function additionally allows overriding mutability.
    fn define_variable_(&mut self, var: LocalVariable, cst: Option<&CSTNode>) -> Rc<LocalVariable> {
        let def = Rc::new(var);
        self.add_function_variable(Rc::clone(&def));
        self.insert_variable(&def, true, cst);
        def
    }

    /// Defines a new temporary variable and returns the variable and an expression
    /// to set it to the value to be executed immediately.
    fn temp_variable(&mut self, expr: Expr, name: SmolStr) -> (Expr, Rc<LocalVariable>) {
        let def = self.define_variable_(
            LocalVariable {
                name,
                mutable: true,
                ty: expr.get_type(),
            },
            None,
        );
        (Expr::store(Expr::lvar(&def), expr, true), def)
    }

    /// Inserts a variable into the topmost scope.
    /// Note that the variable does NOT get added to the function!
    fn insert_variable(
        &mut self,
        var: &Rc<LocalVariable>,
        allow_redefine: bool,
        err: Option<&CSTNode>,
    ) {
        let cur_env = self.environments.last_mut().unwrap();
        let was_defined = cur_env.insert(var.name.clone(), Rc::clone(&var)).is_some();
        if was_defined && !allow_redefine {
            self.err(err.unwrap().clone(), GErr::E208(var.name.clone()));
        }
    }

    /// Will insert the variable into the current function.
    fn add_function_variable(&mut self, variable: Rc<LocalVariable>) {
        self.cur_fn()
            .borrow_mut()
            .insert_var(variable.name.clone(), variable);
    }

    /// Returns the method that corresponds to the operator given (operator overloading).
    /// Returns None if the given class does not implement the operator.
    fn get_operator_overloading_method(
        &mut self,
        op: SyntaxKind,
        left: &mut Expr,
        right: &mut Expr,
    ) -> Option<Instance<Function>> {
        let left_ty = left.get_type();
        let interface = self.intrinsics.get_op_iface(op)?;
        self.get_op_method(&interface, &left_ty, right)
    }

    /// Tries finding a fitting method for an operator overload,
    /// given the overloading interface, type of the implementor and
    /// the expression of the right value (to allow casting it if needed)
    fn get_op_method(
        &mut self,
        interface: &MutRc<ADT>,
        ty: &Type,
        right: &mut Expr,
    ) -> Option<Instance<Function>> {
        let iface_impls = self.get_iface_impls(ty);
        let iface_impls = iface_impls.borrow();

        for im in iface_impls.interfaces.values() {
            if Rc::ptr_eq(&im.iface.ty, interface) {
                let method = im.methods.values().next().unwrap();
                let ty = &method.borrow().parameters[1].ty;
                self.try_cast_in_place(right, ty);
                if *ty == right.get_type() {
                    // TODO: Type arguments
                    return Some(method.to_inst());
                }
            }
        }
        None
    }

    /// Searches all scopes for a variable, starting at the top.
    fn find_var(&mut self, name: &SmolStr, cst: &CSTNode) -> Res<Variable> {
        self.find_local_var(name, cst)
            .map(Variable::Local)
            .or_else(|| self.find_global_var(name))
            .or_err(cst, GErr::E206(name.clone()))
    }

    /// Searches for a local variable.
    fn find_local_var(&mut self, name: &SmolStr, cst: &CSTNode) -> Option<Rc<LocalVariable>> {
        for env in self.environments.iter().rev() {
            if let Some(var) = env.get(name) {
                return Some(Rc::clone(var));
            }
        }

        if let Some(closure_data) = &mut self.closure_data {
            for env in closure_data.outer_env.iter().rev() {
                if let Some(var) = env.get(name) {
                    if !var.ty.is_assignable() {
                        gir_err(cst.clone(), GErr::E205);
                    }
                    closure_data.captured.push(Rc::clone(var));
                    return Some(Rc::clone(var));
                }
            }
        }
        None
    }

    fn find_global_var(&self, name: &SmolStr) -> Option<Variable> {
        let decl = self.module.borrow().find_decl(name)?;
        match decl {
            Declaration::Function(func) => Some(Variable::Function(Instance::new_(func))),
            _ => None,
        }
    }

    /// Returns the variable of the current loop or creates it if it does not exist yet.
    /// This variable stores the value of the last loop iteration.
    fn set_loop_type(&mut self, type_: &Type, err: &CSTNode) {
        match &self.current_loop_ty {
            Some(ty) if !ty.equal(type_, false) => self.err(
                err.clone(),
                GErr::E209 {
                    expected: ty.to_string(),
                    was: type_.to_string(),
                },
            ),

            None | Some(Type::Any) => self.current_loop_ty = Some(type_.clone()),
            _ => (),
        }
    }

    /// Returns a field of the given expression/object,
    /// where a field can be either a member or a method.
    /// Does visibility checks.
    fn get_field(&mut self, get: &Get) -> Res<(Expr, FieldOrMethod)> {
        let (expr, field) = self.get_field_(get)?;
        let visibility = match &field {
            FieldOrMethod::Field(field) => field.visibility,
            FieldOrMethod::Method(method)
            | FieldOrMethod::VirtMethod(ConcreteMethodGet {
                iface_method: method,
                ..
            }) => method.borrow().visibility,
        };

        let ty = expr.get_type();
        let allowed = match visibility {
            Visibility::Private => {
                match (&self.ty_position, &ty) {
                    // Ensure that ADTs with different type arguments can still access private fields
                    (Some(Type::Adt(inst1)), Type::Adt(inst2)) => Rc::ptr_eq(&inst1.ty, &inst2.ty),
                    (Some(ty1), ty2) => ty1 == ty2,
                    _ => false,
                }
            }
            Visibility::Module if self.module.borrow().path.index(0) == ty.module().as_ref() => {
                true
            }
            Visibility::Public => true,
            _ => false,
        };

        if allowed {
            Ok((expr, field))
        } else {
            Err(gir_err(get.cst(), GErr::E240))
        }
    }

    fn get_field_(&mut self, get: &Get) -> Res<(Expr, FieldOrMethod)> {
        let object = self.expression(&get.callee());
        let ty = object.get_type();

        if let Some(adt) = ty.try_adt() {
            let adt = adt.ty.borrow();
            let field = adt.fields.get(&get.property().name());
            if let Some(field) = field {
                return Ok((object, FieldOrMethod::Field(Rc::clone(field))));
            }
        }

        self.find_associated_method(&ty, &get.property().name())
            .map(|m| (object, m))
            .or_err(&get.cst, GErr::E210)
    }

    /// Searches for an associated method on a type. Can be either an interface
    /// method or a class method.
    fn find_associated_method(&mut self, ty: &Type, name: &SmolStr) -> Option<FieldOrMethod> {
        let method = match ty {
            Type::Adt(adt) => {
                let adt = adt.ty.borrow();
                adt.methods.get(name).cloned().map(FieldOrMethod::Method)
            }

            Type::Variable(TypeVariable {
                index,
                bound: TypeParameterBound::Interface(interface),
                ..
            }) => {
                let iface = interface.as_adt().ty.borrow();
                iface.methods.get(name).cloned().map(|iface_method| {
                    FieldOrMethod::VirtMethod(ConcreteMethodGet {
                        index: *index,
                        interface: (**interface).clone(),
                        iface_method,
                    })
                })
            }

            _ => None,
        };

        method.or_else(|| {
            self.get_iface_impls(ty)
                .borrow()
                .methods
                .get(name)
                .cloned()
                .map(FieldOrMethod::Method)
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
    fn insert_at_ptr(&mut self, expr: Expr) {
        let func = self.cur_fn();
        let mut func = func.borrow_mut();
        func.exprs.push(expr)
    }

    /// Sets the insertion pointer.
    /// Insertion is always at the end of a function.
    /// Also sets resolver type parameters.
    fn set_pointer(&mut self, func: &MutRc<Function>) {
        self.set_context(&func.borrow().type_parameters);
        self.position = Some(Rc::clone(func))
    }

    /// Returns the function of the insertion pointer.
    fn cur_fn(&self) -> MutRc<Function> {
        self.position.as_ref().unwrap().clone()
    }

    /// Create new error and add it to the list of errors.
    fn err(&self, cst: CSTNode, err: GErr) {
        self.error(gir_err(cst, err))
    }

    /// Add error to the list of errors.
    fn error(&self, error: Error) {
        self.error_(error, &self.module.borrow())
    }

    fn error_(&self, error: Error, module: &Module) {
        let mut errs = self.errors.borrow_mut();
        if let Some(errs) = errs.get_mut(&self.path) {
            errs.errors.push(error);
        } else {
            errs.insert(
                Rc::clone(&self.path),
                Errors {
                    errors: vec![error],
                    src: Some(Rc::clone(&module.src)),
                    origin: format!("{}", module.path),
                },
            );
        }
    }

    /// Switch to compiling a different module, resetting module state.
    fn switch_module(&mut self, new: MutRc<Module>) {
        self.module = new;
        self.path = Rc::clone(&self.module.borrow().path);
        self.type_params = None;
        self.environments.clear();
        self.current_loop_ty = None;
        self.position = None;
        self.uninitialized_this_fields.clear();
    }

    fn eat<T>(&self, res: Res<T>) -> Option<T> {
        match res {
            Ok(i) => Some(i),
            Err(e) => {
                self.error(e);
                None
            }
        }
    }

    /// Create a new generator from AST modules.
    pub fn new(modules: Vec<ast::Module>, flags: GIRFlags) -> Self {
        bench!("gir precompile", {
            let modules: Vec<_> = modules.into_iter().map(Module::new).collect();
            Self::from_modules(None, modules, flags)
        })
    }

    pub fn with_cached_std(
        modules: Vec<ast::Module>,
        std: &CompiledGIR,
        mut flags: GIRFlags,
    ) -> Self {
        bench!("gir precompile", {
            flags.cached_std = true;
            let mods_uncompiled = modules.into_iter().map(Module::new).collect();
            let mut gen = Self::from_modules(Some(std.modules.clone()), mods_uncompiled, flags);
            gen.intrinsics = std.intrinsics.clone();
            gen.iface_impls = std.iface_impls.clone();
            gen
        })
    }

    /// Produces a [GIRGenerator] usable for generating a closure literal,
    /// temporarily making the outer generator unusable.
    /// It takes the outer environments to allow for capturing variables,
    /// and also records some other required closure data.
    /// This data is then retried with `self.end_closure`.
    fn for_closure(outer: &mut GIRGenerator) -> Self {
        let modules = mem::replace(&mut outer.modules, vec![]);
        let modules_uncompiled = mem::replace(&mut outer.modules_uncompiled, vec![]);
        GIRGenerator {
            closure_data: Some(ClosureData {
                outer_env: mem::replace(&mut outer.environments, vec![]),
                captured: Vec::with_capacity(3),
            }),
            ..Self::from_modules_(modules, modules_uncompiled, outer.flags)
        }
    }

    /// Ends closure compilation and restores the outer generator,
    /// returning recorded info about the compiled closure.
    fn end_closure(self, outer: &mut GIRGenerator) -> ClosureData {
        let mut closure_data = self.closure_data.unwrap();
        outer.environments = mem::replace(&mut closure_data.outer_env, vec![]);
        outer.modules = self.modules;
        outer.modules_uncompiled = self.modules_uncompiled;
        closure_data
    }

    /// Create a new generator from GIR modules.
    fn from_modules(
        precompiled: Option<Vec<MutRc<Module>>>,
        uncompiled: Vec<MutRc<Module>>,
        flags: GIRFlags,
    ) -> Self {
        let path = uncompiled[0].borrow().path.clone();

        if let Some(modules) = &precompiled {
            Self::reset_modules(modules);
        }
        let modules = precompiled
            .into_iter()
            .flatten()
            .chain(uncompiled.iter().cloned())
            .collect();

        Self {
            position: None,
            ty_position: None,
            path,
            module: Rc::clone(&uncompiled[0]),
            modules,
            modules_uncompiled: uncompiled,
            intrinsics: Intrinsics::default(),
            iface_impls: HashMap::with_capacity(100),
            environments: vec![HashMap::with_capacity(3)],
            type_params: None,
            current_loop_ty: None,
            uninitialized_this_fields: HashSet::with_capacity(5),
            closure_data: None,
            errors: mutrc_new(HashMap::new()),
            flags,
        }
    }

    /// Create a new generator from GIR modules.
    fn from_modules_(
        modules: Vec<MutRc<Module>>,
        modules_uncompiled: Vec<MutRc<Module>>,
        flags: GIRFlags,
    ) -> Self {
        let path = modules[0].borrow().path.clone();
        Self {
            position: None,
            ty_position: None,
            path,
            module: Rc::clone(&modules[0]),
            modules,
            modules_uncompiled,
            intrinsics: Intrinsics::default(),
            iface_impls: HashMap::with_capacity(100),
            environments: vec![HashMap::with_capacity(3)],
            type_params: None,
            current_loop_ty: None,
            uninitialized_this_fields: HashSet::with_capacity(5),
            closure_data: None,
            errors: mutrc_new(HashMap::new()),
            flags,
        }
    }

    fn reset_modules(modules: &[MutRc<Module>]) {
        for module in modules.iter() {
            for decl in module.borrow().declarations.values() {
                match decl {
                    Declaration::Function(func) => func.borrow().ir.borrow_mut().clear(),
                    Declaration::Adt(adt) => adt.borrow_mut().ir.clear(),
                }
            }

            for func in &module.borrow().functions {
                func.borrow().ir.borrow_mut().clear();
            }
        }
    }
}

/// Data required for closure compilation.
struct ClosureData {
    /// All environments in the function that the closure literal
    /// is being compiled in.
    pub outer_env: Vec<Environment>,
    /// All variables inside the outer environments that are used
    /// inside the closure and therefore 'captured'
    pub captured: Vec<Rc<LocalVariable>>,
}

#[derive(Debug)]
pub enum FieldOrMethod {
    Field(Rc<Field>),
    Method(MutRc<Function>),
    VirtMethod(ConcreteMethodGet),
}

impl FieldOrMethod {
    pub fn try_field(self) -> Option<Rc<Field>> {
        if let Self::Field(f) = self {
            Some(f)
        } else {
            None
        }
    }
}
