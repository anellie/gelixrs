/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 2/3/20 1:58 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{collections::HashMap, mem, path::Path, rc::Rc};

use common::MutRc;
use gir_nodes::{
    declaration::{IRFunction, Variable},
    types::TypeArguments,
    Function, Instance, Type,
};
use inkwell::{
    basic_block::BasicBlock,
    builder::Builder,
    context::Context,
    module::Module,
    types::BasicTypeEnum,
    values::{BasicValueEnum, FunctionValue, PointerValue},
};

use gir_generator::CompiledGIR;
use gir_ir_adapter::IRAdapter;
use inkwell::types::StructType;
use std::{cell::Ref, option::Option::Some};

mod expr;
mod gc;
mod intrinsics;
mod types;
mod values;

/// A generator that creates LLVM IR out of Gelix IR (GIR).
///
/// Will panic when encountering invalid code; this should not happen however thanks to the
/// `GIRGenerator` validating the GIR it generates.
pub struct IRGenerator {
    context: Context,
    builder: Builder,
    module: Module,

    /// The currently compiled function.
    function: Option<FunctionValue>,

    /// All local stores in this function that need their refcount
    /// to be decremented when the function returns.
    /// This also includes locally declared variables.
    /// This is a vector to account for local variables that are not available
    /// during all parts of the function - locals declared inside of an if clause for example.
    ///
    /// The `bool` specifies if the value is a pointer or a value in
    /// the context of the GIR type system.
    locals: Vec<Vec<(BasicValueEnum, bool)>>,
    local_allocs: Vec<Vec<PointerValue>>,
    /// All blocks in the current function.
    blocks: Vec<BasicBlock>,
    /// The block that was last inserted to, or the one still inserting to.
    last_block: Option<BasicBlock>,
    /// All local variables in the current function.
    variables: HashMap<Variable, PointerValue>,

    /// A map of types based on their names in IR to allow for backwards lookup.
    types_bw: HashMap<String, Type>,
    /// A constant that is used for expressions that don't produce a value but are required to,
    /// like return or break expressions.
    none_const: BasicValueEnum,
    /// The type for type info that is baked into every value
    type_info_type: StructType,
    /// Type arguments to substitute if encountering Type::Variable
    type_args: Vec<Option<Rc<TypeArguments>>>,

    /// A list of functions that still require being generated.
    /// The compiler only generates `main` and a few intrinsic functions first,
    /// adding all referenced functions here.
    /// The actual compilation of them then occurs by removing from this vector until it is empty.
    functions_left: Vec<(MutRc<Function>, Rc<TypeArguments>)>,

    /// Needed state about the current loop, if compiling one.
    loop_data: Option<LoopData>,

    /// GIR compilation data.
    gir_data: CompiledGIR,
}

impl IRGenerator {
    /// Generates IR. Will process all GIR modules given.
    pub fn generate(mut self) -> Module {
        // Get required-to-compile fns from INTRINSICS
        let required_fns = mem::replace(&mut self.gir_data.intrinsics.required_compile_fns, vec![]);
        // Declare them and collect into new vec
        let fns_ir = required_fns
            .iter()
            .map(|f| self.declare_function(f))
            .collect::<Vec<_>>();
        // Compile their bodies
        fns_ir
            .into_iter()
            .zip(required_fns.into_iter())
            .for_each(|(ir, f)| self.function(&f, ir.unwrap()));

        // Compile all functions out from required, See docs on functions_left
        while let Some((func, args)) = self.functions_left.pop() {
            self.push_ty_args(Some(&args));
            let ir = func.borrow().ir.borrow().get_inst(&args).unwrap();
            self.function(&func, ir);
            self.pop_ty_args();
        }

        let intrinsics_module = self
            .gir_data
            .modules
            .iter()
            .find(|m| {
                let module = m.borrow();
                module.path.is(&["std", "intrinsics"])
            })
            .cloned();
        self.fill_intrinsic_functions(&intrinsics_module.unwrap());

        self.module
            .verify()
            .map_err(|e| {
                self.module.print_to_file(Path::new("invalid_code.ll")).unwrap_or(());
                if !cfg!(test) {
                    println!("The compiler generated invalid code, which can be found in 'invalid_code.ll'.");
                    println!("This is a severe internal bug, and should be reported (please include the code when doing so).");
                    println!("The error message reported by LLVM:\n");
                    println!("{}\n", e.to_string().replace("\\n", "\n"));
                    println!("The compiler is forced to terminate.");
                    std::process::exit(1);
                } else {
                    panic!("Invalid IR")
                }
            })
            .unwrap();
        self.module
    }

    pub fn get_or_create(&mut self, func: &Instance<Function>) -> FunctionValue {
        let args = func.args();
        let args = self.process_args(args);

        let func_ref = func.ty.borrow();
        let mut ir = func_ref.ir.borrow_mut();
        let inst = ir.get_inst(&args);
        if let Some(ir) = inst {
            ir
        } else {
            self.create_function(func, &mut *ir, args)
        }
    }

    fn create_function(
        &mut self,
        func: &Instance<Function>,
        ir: &mut IRFunction,
        args: Rc<TypeArguments>,
    ) -> FunctionValue {
        self.push_ty_args(Some(&args));
        let func_ir = self.declare_function_inst(&func.ty.borrow(), &format!("[{}]", ir.count()));
        ir.add_inst(&args, func_ir);
        self.functions_left.push((Rc::clone(&func.ty), args));
        self.pop_ty_args();
        func_ir
    }

    /// Declares a function.
    fn declare_function(&mut self, func: &MutRc<Function>) -> Option<FunctionValue> {
        let func = func.borrow();
        let mut ir = func.ir.borrow_mut();
        match &mut *ir {
            IRAdapter::NoTypeArgs(opt) => {
                let inst = self.declare_function_inst(&func, "");
                *opt = Some(inst);
                Some(inst)
            }
            IRAdapter::TypeArgs(_) => None,
        }
    }

    /// Declares a single function instance
    fn declare_function_inst(&mut self, func: &Ref<Function>, suffix: &str) -> FunctionValue {
        let params = func.parameters.iter().map(|param| &param.ty);
        let fn_ty = self.fn_type_from_raw(params, &func.ret_type, func.variadic);

        // If extern fn OR main: don't prepend module name
        let name = if func.ast.as_ref().map(|a| a.body()).flatten().is_none() || func.name == "main" {
            format!("{}", func.name)
        } else {
            format!("{}::{}{}", func.module.borrow().path, func.name, suffix)
        };

        self.module.add_function(&name, fn_ty, None)
    }

    /// Generates a function, should it have a body.
    /// Does not handle type arguments.
    fn function(&mut self, func_var: &MutRc<Function>, ir: FunctionValue) {
        let func = func_var.borrow();
        if !func.exprs.is_empty() {
            self.function_body(&func, ir);
        }
    }

    /// Generates a function's body.
    fn function_body(&mut self, func: &Ref<Function>, func_val: FunctionValue) {
        self.function = Some(func_val);
        self.blocks.clear();

        self.prepare_function(&func, func_val);

        for (name, var) in &func.variables {
            let alloc_ty = self.ir_ty_allocs(&var.ty);
            let alloca = self.builder.build_alloca(alloc_ty, &name);
            self.variables
                .insert(Variable::Local(Rc::clone(var)), alloca);
        }

        for expr in &func.exprs {
            self.expression(expr);
        }

        // Build a return if the end of the function is an implicit return
        if self.builder.get_insert_block().is_some() {
            self.decrement_all_locals();
            self.builder.build_return(None);
        }

        self.variables.clear();
    }

    /// Given a function, will do steps needed to allow code generation, starting
    /// with resetting function-specific state.
    /// It inserts the entry BB in IR, and build all parameter alloca.
    /// self.builder will be positioned after the entry alloca after calling this method.
    /// Returns the entry basic block.
    fn prepare_function(&mut self, func: &Ref<Function>, func_val: FunctionValue) {
        self.blocks.clear();
        self.variables.clear();
        self.locals.clear();
        self.local_allocs.clear();
        self.push_local_scope();

        let entry_bb = self.context.append_basic_block(&func_val, "entry");
        self.blocks.push(entry_bb);

        self.position_at_block(entry_bb);
        self.build_parameter_alloca(&func, func_val);
    }

    fn build_parameter_alloca(&mut self, func: &Ref<Function>, func_val: FunctionValue) {
        for (arg, arg_val) in func.parameters.iter().zip(func_val.get_param_iter()) {
            if let Type::ClosureCaptured(captured) = &arg.ty {
                // If this is the first arg on a closure containing all captured variables,
                // 'unpack' them and create separate entries for each in self.variables
                // so they can be used like regular variables.
                let arg_val = *arg_val.as_pointer_value();
                for (i, var) in captured.iter().enumerate() {
                    let field = self.struct_gep(arg_val, i);
                    self.variables
                        .insert(Variable::Local(Rc::clone(var)), field);
                }
            } else if let BasicValueEnum::PointerValue(ptr) = arg_val {
                // If the type of the function parameter is a pointer (aka a struct or function),
                // creating an alloca isn't needed; the pointer can be used directly.
                self.variables.insert(Variable::Local(Rc::clone(arg)), ptr);
            } else {
                let alloc = self.builder.build_alloca(arg_val.get_type(), &arg.name);
                self.builder.build_store(alloc, arg_val);
                self.variables
                    .insert(Variable::Local(Rc::clone(arg)), alloc);
            }
        }
    }

    fn position_at_block(&mut self, block: BasicBlock) {
        self.last_block = Some(block);
        self.builder.position_at_end(&block)
    }

    pub fn append_block(&mut self, name: &'static str) -> BasicBlock {
        let bb = self
            .context
            .append_basic_block(&self.function.unwrap(), name);
        self.blocks.push(bb);
        bb
    }

    pub fn last_block(&self) -> BasicBlock {
        self.last_block.unwrap()
    }

    pub fn push_ty_args(&mut self, args: Option<&Rc<TypeArguments>>) {
        self.type_args.push(args.map(|a| Rc::clone(a)));
    }

    pub fn pop_ty_args(&mut self) {
        self.type_args.pop();
    }

    pub fn new(gir_data: CompiledGIR) -> IRGenerator {
        let context = Context::create();
        let module = context.create_module("main");
        let builder = context.create_builder();

        let none_const = context
            .struct_type(&[BasicTypeEnum::IntType(context.bool_type())], true)
            .const_named_struct(&[BasicValueEnum::IntValue(
                context.bool_type().const_int(0, false),
            )]);

        let type_info_type = context.struct_type(&[context.i64_type().into()], false);

        IRGenerator {
            context,
            module,
            builder,
            function: None,

            locals: Vec::with_capacity(10),
            local_allocs: Vec::with_capacity(10),
            blocks: Vec::with_capacity(10),
            last_block: None,
            variables: HashMap::with_capacity(30),

            types_bw: HashMap::with_capacity(50),
            type_info_type,
            none_const: none_const.into(),
            type_args: Vec::with_capacity(3),
            functions_left: Vec::with_capacity(20),

            loop_data: None,
            gir_data,
        }
    }
}

pub struct LoopData {
    /// The block to jump to using break expressions;
    /// the block at the end of the loop.
    pub end_block: BasicBlock,
    pub phi_nodes: Option<Vec<(BasicValueEnum, BasicBlock)>>,
}
