/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/30/19 8:18 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{
    cell::RefMut,
    collections::HashMap,
    hash::{Hash, Hasher},
    path::Path,
    rc::Rc,
};

use inkwell::{
    basic_block::BasicBlock,
    builder::Builder,
    context::Context,
    module::Module,
    types::BasicTypeEnum,
    values::{BasicValueEnum, FunctionValue, PointerValue},
};

use crate::mir::{
    nodes::{Expr, Function, Type, Variable},
    MModule, MutRc,
};

mod gc;
mod gen_expr;
mod intrinsics;
mod types;
mod values;

/// A generator that creates LLVM IR out of Gelix mid-level IR (MIR).
///
/// Will panic when encountering invalid code; this should not happen however thanks to the
/// MIRGenerator validating the MIR it generates.
pub struct IRGenerator {
    context: Context,
    builder: Builder,
    module: Module,

    /// All variables, the currently compiled function.
    /// Note that not all variables are valid - they are kept after going out of scope.
    /// This is not an issue since the MIR generator checked against this already.
    variables: HashMap<PtrEqRc<Variable>, PointerValue>,
    /// All blocks in the current function.
    blocks: HashMap<Rc<String>, BasicBlock>,

    /// All functions.
    functions: HashMap<PtrEqRc<Variable>, FunctionValue>,
    /// All types (classes/interfaces/structs) that are available.
    types: HashMap<Type, BasicTypeEnum>,

    /// A constant that is used for expressions that don't produce a value but are required to,
    /// like return or break expressions.
    none_const: BasicValueEnum,
}

impl IRGenerator {
    /// Generates IR. Will process all MIR modules given.
    pub fn generate(mut self, mir: Vec<MutRc<MModule>>) -> Module {
        for module in &mir {
            let module = module.borrow_mut();
            for function in module.globals.values() {
                self.declare_function(function);
            }
        }

        let intrinsics_module = mir.iter().find(|m| {
            let module = m.borrow();
            **module.path.0[0] == *"std" && **module.path.0[1] == *"intrinsics"
        });
        self.fill_intrinsic_functions(intrinsics_module.unwrap());

        for module in mir {
            let module = module.borrow_mut();
            for function in module.globals.values().cloned() {
                self.function(function);
            }
        }

        self.module
            .verify()
            .map_err(|e| {
                self.module.print_to_file(Path::new("invalid_code.ll")).unwrap_or(());
                println!("The compiler generated invalid code, which can be found in the 'invalid_code.ll'.");
                println!("This is a bug, and should be reported (please include the code when doing so).");
                println!("The error message reported by LLVM:\n");
                println!("{}\n", e.to_string().replace("\\n", "\n"));
                std::process::exit(1);
            })
            .unwrap();
        self.module
    }

    /// Declares a function. All functions must be declared before generating
    /// code; as a reference to an undeclared function is otherwise possible
    /// (and leads to a panic)
    fn declare_function(&mut self, func: &Rc<Variable>) {
        let func_ty = func.type_.as_function();
        let fn_ty = self.build_fn_type(func_ty.borrow());
        let func_val = self
            .module
            .add_function(&func_ty.borrow().name, fn_ty, None);
        self.functions.insert(PtrEqRc::new(&func), func_val);
    }

    /// Generates a function, should it have a body.
    fn function(&mut self, func_var: Rc<Variable>) {
        let func_ty = func_var.type_.as_function();
        let func = func_ty.borrow_mut();
        if !func.blocks.is_empty() {
            let func_val = self.functions[&PtrEqRc::new(&func_var)];
            self.function_body(func, func_val);
        }
    }

    /// Generates a function's body.
    fn function_body(&mut self, mut func: RefMut<Function>, func_val: FunctionValue) {
        self.blocks.clear();

        let entry_b = func.blocks.remove_entry(&"entry".to_string()).unwrap();
        self.prepare_function(&func, func_val);

        for (name, var) in func.variables.iter() {
            let alloc_ty = self.ir_ty_ptr(&var.type_);
            let alloca = self.builder.build_alloca(alloc_ty, &name);
            self.variables.insert(PtrEqRc::new(var), alloca);
        }

        // Fill in all blocks first before generating any actual code;
        // otherwise referencing a block yet to be built would result in a panic
        for (name, _block) in func.blocks.iter() {
            let bb = self.context.append_basic_block(&func_val, name);
            self.blocks.insert(Rc::clone(name), bb);
        }

        self.fill_basic_block(&entry_b.1);

        // Fill all blocks
        for (name, block) in func.blocks.iter() {
            let bb = self.blocks[name];
            self.position_at_block(bb);
            self.fill_basic_block(block)
        }
    }

    /// Given a function, will do steps needed to allow code generation, starting
    /// with resetting function-specific state.
    /// It inserts the entry BB in IR, and build all parameter alloca.
    /// self.builder will be positioned after the entry alloca after calling this method.
    /// Returns the entry basic block.
    fn prepare_function(&mut self, func: &RefMut<Function>, func_val: FunctionValue) {
        self.blocks.clear();
        self.variables.clear();
        let entry_bb = self.context.append_basic_block(&func_val, "entry");
        self.builder.position_at_end(&entry_bb);
        self.build_parameter_alloca(&func, func_val);
    }

    fn build_parameter_alloca(&mut self, func: &RefMut<Function>, func_val: FunctionValue) {
        for (arg, arg_val) in func.parameters.iter().zip(func_val.get_param_iter()) {
            if let Type::ClosureCaptured(captured) = &arg.type_ {
                // If this is the first arg on a closure containing all captured variables,
                // 'unpack' them and create separate entries for each in self.variables
                // so they can be used like regular variables.
                let arg_val = *arg_val.as_pointer_value();
                for (i, var) in captured.iter().enumerate() {
                    let field = self.struct_gep(arg_val, i);
                    self.variables.insert(PtrEqRc::new(var), field);
                }
            } else if let BasicValueEnum::PointerValue(ptr) = arg_val {
                // If the type of the function parameter is a pointer (aka a struct or function),
                // creating an alloca isn't needed; the pointer can be used directly.
                self.variables.insert(PtrEqRc::new(arg), ptr);
            } else {
                let alloc = self.builder.build_alloca(arg_val.get_type(), &arg.name);
                self.builder.build_store(alloc, arg_val);
                self.variables.insert(PtrEqRc::new(arg), alloc);
            }
        }
    }

    fn position_at_block(&mut self, block: BasicBlock) {
        match block.get_first_instruction() {
            Some(inst) => self.builder.position_before(&inst),
            None => self.builder.position_at_end(&block),
        }
    }

    fn fill_basic_block(&mut self, mir_bb: &[Expr]) {
        for expression in mir_bb.iter() {
            self.expression(expression);
        }

        // Blocks that return from a None-type function do not have a MIR terminator,
        // so we create a void return (The insert block would be unset had there been a return)
        if self.builder.get_insert_block().is_some() {
            self.builder.build_return(None);
        }
    }

    pub fn new() -> IRGenerator {
        let context = Context::create();
        let module = context.create_module("main");
        let builder = context.create_builder();

        let none_const = context
            .struct_type(&[BasicTypeEnum::IntType(context.bool_type())], true)
            .const_named_struct(&[BasicValueEnum::IntValue(
                context.bool_type().const_int(0, false),
            )]);

        let mut types = HashMap::with_capacity(20);
        types.insert(Type::None, none_const.get_type().into());
        types.insert(Type::Bool, context.bool_type().into());

        types.insert(Type::I8, context.i8_type().into());
        types.insert(Type::I16, context.i16_type().into());
        types.insert(Type::I32, context.i32_type().into());
        types.insert(Type::I64, context.i64_type().into());

        types.insert(Type::F32, context.f32_type().into());
        types.insert(Type::F64, context.f64_type().into());

        IRGenerator {
            context,
            module,
            builder,

            variables: HashMap::with_capacity(10),
            blocks: HashMap::with_capacity(10),

            types,
            functions: HashMap::with_capacity(10),

            none_const: none_const.into(),
        }
    }
}

/// A Rc that can be compared by checking for pointer equality.
/// Used as a HashMap key to allow unique keys with the same data.
pub struct PtrEqRc<T: Hash>(Rc<T>);

impl<T: Hash> PtrEqRc<T> {
    pub fn new(rc: &Rc<T>) -> PtrEqRc<T> {
        PtrEqRc(Rc::clone(rc))
    }
}

impl<T: Hash> PartialEq for PtrEqRc<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl<T: Hash> Eq for PtrEqRc<T> {}

impl<T: Hash> Hash for PtrEqRc<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}
