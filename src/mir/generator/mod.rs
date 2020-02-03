/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 2/3/20 1:28 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{
    collections::{HashMap, HashSet},
    mem,
    rc::Rc,
};

use either::Either;
use indexmap::IndexMap;

use crate::{
    ast::{
        declaration::{Class as ASTClass, Constructor, Function as ASTFunc},
        expression::Expression as ASTExpr,
    },
    error::Res,
    lexer::token::{TType, Token},
    mir::{
        generator::{builder::MIRBuilder, intrinsics::INTRINSICS},
        get_iface_impls,
        nodes::{ClassMember, Expr, Function, Type, Variable},
        result::ToMIRResult,
        MModule, MutRc, IFACE_IMPLS,
    },
    Error,
};
use either::Either::{Left, Right};

pub mod builder;
pub mod gen_expr;
pub mod intrinsics;
pub mod module;
pub mod passes;

/// The MIRGenerator turns a list of declarations produced by the parser
/// into their MIR representation.
///
/// MIR is an intermediate format between the AST and LLVM IR.
///
/// The generator not only generates MIR, but also checks the code
/// for correctness (type-checking, scoping, etc.).
pub struct MIRGenerator {
    /// The module that the generator is linked to.
    pub module: MutRc<MModule>,

    /// The builder used to build the MIR.
    pub builder: MIRBuilder,

    /// The current insertion position.
    position: Option<Pointer>,

    /// An environment is a scope that variables live in.
    /// This field is used like a stack.
    /// See the begin_scope and end_scope functions for more info.
    environments: Vec<HashMap<Rc<String>, Rc<Variable>>>,

    /// The current loop, if in one.
    current_loop: Option<ForLoop>,

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
    uninitialized_this_members: HashSet<Rc<ClassMember>>,

    /// Closure-related data, if compiling a closure.
    closure_data: Option<ClosureData>,
}

impl MIRGenerator {
    /// Fill a function's body.
    /// The AST given must be from a function inside the module.
    pub fn generate_function(
        &mut self,
        func: &ASTFunc,
        override_fn: Option<&MutRc<Function>>,
    ) -> Res<()> {
        // Don't have to generate anything for external functions
        // which do not have a body
        let body = match func.body.as_ref() {
            None => return Ok(()),
            Some(body) => body,
        };

        let function = override_fn.cloned().unwrap_or_else(|| {
            self.module
                .borrow()
                .find_type(&func.sig.name.lexeme)
                .unwrap()
                .as_function()
                .clone()
        });
        self.prepare_function(&function, func.sig.name.line)?;
        let body = self.expression(body)?;

        let ret_type = function.borrow().ret_type.clone();
        match () {
            _ if ret_type == Type::None => self.insert_at_ptr(body),
            _ if ret_type == body.get_type() => self.insert_at_ptr(Expr::ret(body)),
            _ => {
                return Err(self.err(
                    &func.sig.name,
                    &format!(
                        "Function return type ({}) does not match body type ({}).",
                        ret_type,
                        body.get_type()
                    ),
                ));
            }
        };

        self.end_scope();
        Ok(())
    }

    pub fn generate_constructors(&mut self, class: &ASTClass) -> Res<()> {
        let class_rc = self
            .module
            .borrow()
            .find_type(&class.name.lexeme)
            .unwrap()
            .as_class()
            .clone();

        for (constructor, mir_fn) in class.constructors.iter().zip(
            class_rc
                .borrow()
                .constructors
                .iter()
                .map(|v| v.type_.as_function()),
        ) {
            self.prepare_function(
                mir_fn,
                constructor.parameters.get(0).map(|l| l.0.line).unwrap_or(0),
            )?;
            self.set_uninitialized_members(constructor, &class_rc.borrow().members);
            let body = self.expression(&constructor.body)?;
            self.insert_at_ptr(body);
            self.end_scope();
            self.check_no_uninitialized(&class.name)?;
        }

        self.uninitialized_this_members.clear();
        Ok(())
    }

    fn set_uninitialized_members(
        &mut self,
        constructor: &Constructor,
        class_mems: &IndexMap<Rc<String>, Rc<ClassMember>>,
    ) {
        self.uninitialized_this_members.clear();
        for (name, mem) in class_mems.iter() {
            let initialized = constructor
                .parameters
                .iter()
                .filter(|p| p.1.is_none())
                .any(|p| &p.0.lexeme == name);
            if !initialized && !mem.has_default_value {
                self.uninitialized_this_members.insert(Rc::clone(&mem));
            }
        }
    }

    fn check_no_uninitialized(&mut self, err_tok: &Token) -> Res<()> {
        if self.uninitialized_this_members.is_empty() {
            Ok(())
        } else {
            Err(self.err(
                err_tok,
                "Cannot have uninitialized fields after constructor.",
            ))
        }
    }

    fn prepare_function(&mut self, function: &MutRc<Function>, err_line: usize) -> Res<()> {
        let mut func = function.borrow_mut();
        let entry_block = func.append_block("entry", false);

        self.set_pointer(Rc::clone(function), Rc::clone(&entry_block));
        self.begin_scope();
        for param in func.parameters.iter() {
            self.insert_variable(&param, false, err_line)?;
        }

        Ok(())
    }

    /// Defines a new variable. It is put into the variable list in the current function
    /// and placed in the topmost scope.
    fn define_variable(&mut self, token: &Token, mutable: bool, ty: Type) -> Rc<Variable> {
        let def = Variable::new(mutable, ty, &token.lexeme);
        self.add_function_variable(Rc::clone(&def));
        self.insert_variable(&def, true, token.line).unwrap_or(());
        def
    }

    /// Inserts a variable into the topmost scope.
    /// Note that the variable does NOT get added to the function!
    fn insert_variable(
        &mut self,
        var: &Rc<Variable>,
        allow_redefine: bool,
        line: usize,
    ) -> Res<()> {
        let cur_env = self.environments.last_mut().unwrap();
        let was_defined = cur_env
            .insert(Rc::clone(&var.name), Rc::clone(&var))
            .is_some();
        if was_defined && !allow_redefine {
            let mut tok = Token::generic_identifier((*var.name).clone());
            tok.line = line;
            return Err(self.err(
                &tok,
                &format!(
                    "Cannot redefine variable '{}' in the same scope.",
                    &var.name
                ),
            ));
        }

        Ok(())
    }

    /// Will create the variable in the current function.
    pub fn add_function_variable(&mut self, variable: Rc<Variable>) {
        self.cur_fn()
            .borrow_mut()
            .insert_var(Rc::clone(&variable.name), variable);
    }

    /// Searches all scopes for a variable, starting at the top.
    fn find_var(&mut self, token: &Token) -> Res<Rc<Variable>> {
        for env in self.environments.iter().rev() {
            if let Some(var) = env.get(&token.lexeme) {
                return Ok(Rc::clone(var));
            }
        }

        if let Some(closure_data) = &mut self.closure_data {
            for env in closure_data.outer_env.iter().rev() {
                if let Some(var) = env.get(&token.lexeme) {
                    closure_data.captured.push(Rc::clone(var));
                    return Ok(Rc::clone(var));
                }
            }
        }

        self.module.borrow().find_global(&token.lexeme).or_err(
            &self.builder.path,
            token,
            &format!("Variable '{}' is not defined", token.lexeme),
        )
    }

    /// Returns the variable of the current loop or creates it if it does not exist yet
    fn get_or_create_loop_var(&mut self, type_: &Type) -> Res<Rc<Variable>> {
        let var = self.cur_loop().result_var.clone().unwrap_or_else(|| {
            self.define_variable(
                &Token::generic_identifier("for-body".to_string()),
                true,
                type_.clone(),
            )
        });
        self.cur_loop().result_var = Some(Rc::clone(&var));

        if &var.type_ != type_ {
            Err(Error::new(
                &Token::generic_token(TType::Break),
                "MIR",
                "Break expressions and for body must have same type".to_string(),
                &self.builder.path,
            ))
        } else {
            Ok(var)
        }
    }

    fn get_field(
        &mut self,
        object: &ASTExpr,
        name: &Token,
    ) -> Res<(Expr, Either<Rc<ClassMember>, Callable>)> {
        let object = self.expression(object)?;
        let ty = object.get_type();

        if let Type::Class(class) = &ty {
            let class = class.borrow();
            let field = class.members.get(&name.lexeme);
            if let Some(field) = field {
                return Ok((object, Either::Left(Rc::clone(field))));
            }
        }

        self.find_associated_method(ty, name)
            .map(|m| (object, Either::Right(m)))
            .or_err(&self.builder.path, name, "Unknown field or method.")
    }

    fn generate_func_args<'a, T: Iterator<Item = &'a Type>>(
        &mut self,
        mut parameters: T,
        arguments: &[ASTExpr],
        first_arg: Option<Expr>,
        allow_variadic: bool,
        err_tok: &Token,
    ) -> Res<Vec<Expr>> {
        let para_len = parameters.size_hint().0;
        let args_len = arguments.len() + (first_arg.is_some() as usize);
        if para_len > args_len || (para_len < args_len && !allow_variadic) {
            return Err(self.err(
                err_tok,
                &format!(
                    "Incorrect amount of function arguments. (Expected {}; got {})",
                    parameters.size_hint().0,
                    arguments.len()
                ),
            ));
        }

        let mut result = Vec::with_capacity(args_len);
        if let Some(arg) = first_arg {
            let ty = parameters.next().unwrap();
            let arg = self
                .check_call_arg_type(arg, ty)
                .expect("internal error: method call");
            result.push(arg)
        }
        for (argument, parameter) in arguments.iter().zip(parameters) {
            let arg = self.expression(argument)?;
            let arg_type = arg.get_type();
            let arg = self.check_call_arg_type(arg, &parameter).or_err(
                &self.builder.path,
                argument.get_token(),
                &format!(
                    "Call argument is the wrong type (was {}, expected {})",
                    arg_type, parameter
                ),
            )?;
            result.push(arg)
        }
        // Also generate any variadic args should there be any
        for argument in arguments.iter().skip(result.len()) {
            result.push(self.expression(argument)?);
        }

        Ok(result)
    }

    /// Searches for an associated method on a type. Can be either an interface
    /// method or a class method.
    fn find_associated_method(&self, ty: Type, name: &Token) -> Option<Callable> {
        let method = if let Type::Class(class) = &ty {
            class.borrow().methods.get(&name.lexeme).cloned().map(Left)
        } else if let Type::Interface(iface) = &ty {
            iface
                .borrow()
                .methods
                .get_full(&name.lexeme)
                .map(|(i, _, _)| Right(i))
        } else {
            None
        };

        method.or_else(|| {
            IFACE_IMPLS
                .with(|impls| impls.borrow().get(&ty).cloned())?
                .borrow()
                .methods
                .get(&name.lexeme)
                .cloned()
                .map(Left)
        })
    }

    /// Returns the method that corresponds to the operator given (operator overloading).
    /// Returns None if the given class does not implement overloading.
    fn get_operator_overloading_method(
        &mut self,
        op: TType,
        left_ty: &Type,
        right_ty: &Type,
    ) -> Option<Rc<Variable>> {
        let proto = INTRINSICS.with(|i| i.borrow().get_op_iface(op));
        let iface_impls = get_iface_impls(left_ty)?;
        let iface_impls = iface_impls.borrow();
        let op_impls = iface_impls
            .interfaces
            .values()
            .filter(|im| im.iface.borrow().proto.as_ref().is_some())
            .filter(|im| Rc::ptr_eq(im.iface.borrow().proto.as_ref().unwrap(), &proto));

        for im in op_impls {
            let method = im.methods.get_index(0).unwrap().1;
            if &method.type_.as_function().borrow().parameters[1].type_ == right_ty {
                return Some(method).cloned();
            }
        }
        None
    }

    /// Checks if the arg parameter is of the given type ty.
    /// Will do casts if needed to make the types match;
    /// returns the new expression that should be used in case a cast happened.
    fn check_call_arg_type(&self, arg: Expr, ty: &Type) -> Option<Expr> {
        let arg_type = arg.get_type();
        if &arg_type == ty {
            Some(arg)
        } else if get_iface_impls(&arg_type)?
            .borrow()
            .interfaces
            .get(ty)
            .is_some()
        {
            Some(Expr::cast(arg, ty))
        } else {
            None
        }
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

    /// Will append a block to the given function, always creating a new one.
    pub fn append_block(&mut self, name: &str) -> Rc<String> {
        self.cur_fn().borrow_mut().append_block(name, true)
    }

    pub fn insert_at_ptr(&mut self, expr: Expr) {
        let func = self.cur_fn();
        let mut func = func.borrow_mut();
        func.blocks
            .get_mut(&self.position.as_ref().unwrap().block)
            .unwrap()
            .push(expr)
    }

    pub fn set_pointer(&mut self, function: MutRc<Function>, block: Rc<String>) {
        self.position = Some(Pointer { function, block })
    }

    pub fn set_block(&mut self, block: &Rc<String>) {
        if let Some(pos) = self.position.as_mut() {
            pos.block = Rc::clone(block)
        }
    }

    pub fn cur_fn(&self) -> MutRc<Function> {
        self.position.as_ref().unwrap().function.clone()
    }

    pub fn cur_block_name(&self) -> Rc<String> {
        Rc::clone(&self.position.as_ref().unwrap().block)
    }

    /// Switch the module this generator is operating on.
    /// Doing this will cause everything related to the currently generating code
    /// to be reset.
    pub fn switch_module(&mut self, module: &MutRc<MModule>) {
        self.module = Rc::clone(module);
        self.builder.switch_module(&module);
        self.position = None;
        self.environments.clear();
        self.environments.clear();
        self.current_loop = None;
        self.uninitialized_this_members.clear();
    }

    fn cur_loop(&mut self) -> &mut ForLoop {
        self.current_loop.as_mut().unwrap()
    }

    fn err(&self, tok: &Token, msg: &str) -> Error {
        Error::new(tok, "MIR", msg.to_string(), &self.builder.path)
    }

    pub fn new(builder: MIRBuilder) -> Self {
        MIRGenerator {
            module: Rc::clone(&builder.module),
            builder,
            position: None,
            environments: Vec::with_capacity(5),
            current_loop: None,
            uninitialized_this_members: HashSet::with_capacity(10),
            closure_data: None,
        }
    }

    pub fn for_closure(outer: &mut MIRGenerator) -> Self {
        MIRGenerator {
            module: Rc::clone(&outer.module),
            builder: MIRBuilder::with_context(&outer.module, outer.builder.context.clone()),
            position: None,
            environments: vec![HashMap::with_capacity(3)],
            current_loop: None,
            uninitialized_this_members: HashSet::new(),
            closure_data: Some(ClosureData {
                outer_env: mem::replace(&mut outer.environments, vec![]),
                captured: Vec::with_capacity(3),
            }),
        }
    }

    pub fn end_closure(self, outer: &mut MIRGenerator) -> ClosureData {
        let mut closure_data = self.closure_data.unwrap();
        outer.environments = mem::replace(&mut closure_data.outer_env, vec![]);
        closure_data
    }
}

/// A pointer is the location the generator is inserting into.
pub struct Pointer {
    /// The function inserting into
    pub function: MutRc<Function>,
    /// The name of the block appending to
    block: Rc<String>,
}

/// All data of a loop.
struct ForLoop {
    /// The alloca of the for loop result. Can be None for loops that return None type.
    result_var: Option<Rc<Variable>>,
    /// The block to jump to when the current loop finishes.
    cont_block: Rc<String>,
    /// The phi nodes of the loop (loops are expressions).
    phi_nodes: Vec<(Expr, Rc<String>)>,
    /// All variables that were newly created inside the loop.
    /// This is used to decrement their refcount at the end of the loop.
    variables: Vec<Rc<Variable>>
}

impl ForLoop {
    fn new(cont_block: &Rc<String>) -> ForLoop {
        ForLoop {
            result_var: None,
            cont_block: Rc::clone(cont_block),
            phi_nodes: vec![],
            variables: vec![]
        }
    }
}

pub type Callable = Either<Rc<Variable>, IFaceFuncIndex>;
pub type IFaceFuncIndex = usize;

pub struct ClosureData {
    pub outer_env: Vec<HashMap<Rc<String>, Rc<Variable>>>,
    pub captured: Vec<Rc<Variable>>,
}
