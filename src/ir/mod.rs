/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/21/19 4:30 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{
    cell::{Ref, RefMut},
    collections::HashMap,
    hash::{Hash, Hasher},
    rc::Rc,
};

use inkwell::{
    AddressSpace,
    basic_block::BasicBlock,
    builder::Builder,
    context::Context,
    IntPredicate,
    module::Module,
    passes::PassManager,
    types::{AnyTypeEnum, BasicType, BasicTypeEnum, FunctionType, StructType}, values::{BasicValue, BasicValueEnum, FunctionValue, IntValue, PointerValue},
};

use crate::module_path_to_string;

use super::{
    ast::literal::Literal,
    lexer::token::Type,
    mir::{
        MIRModule,
        nodes::{MIRBlock, MIRExpression, MIRFlow, MIRFunction, MIRStruct, MIRType, MIRVariable},
    },
};

/// A generator that creates LLVM IR out of Gelix mid-level IR (MIR).
///
/// Will panic when encountering invalid code; this should not happen however thanks to the
/// MIRGenerator validating the MIR it generates.
pub struct IRGenerator {
    // LLVM-related. Refer to their docs for more info.
    context: Context,
    builder: Builder,
    module: Module,
    mpm: PassManager<Module>,

    /// All variables, the currently compiled function.
    /// Note that not all variables are valid - they are kept after going out of scope.
    /// This is not an issue since the MIR generator checked against this already.
    variables: HashMap<PtrEqRc<MIRVariable>, PointerValue>,
    /// All blocks in the current function.
    blocks: HashMap<Rc<String>, BasicBlock>,

    /// All types (classes/structs) that are available.
    types: HashMap<Rc<String>, StructType>,

    /// A constant that is used for expressions that don't produce a value but are required to,
    /// like return or break expressions.
    none_const: BasicValueEnum,
    /// Simply a string called "entry". Used to find the entry block of functions.
    entry_const: String,
}

impl IRGenerator {
    /// Generates IR. Will process all MIR modules given.
    pub fn generate(mut self, mir: Vec<MIRModule>) -> Module {
        // Create structs for all classes first
        for module in mir.iter() {
            for struc in module.types.iter() {
                let struc = struc.1.borrow();
                let val = self.context.opaque_struct_type(&struc.name);
                self.types.insert(Rc::clone(&struc.name), val);
            }
        }

        let mut finished_modules = Vec::with_capacity(mir.len());
        for module in mir {
            finished_modules.push(std::mem::replace(
                &mut self.module,
                self.context
                    .create_module(&module_path_to_string(&module.path)),
            ));
            self.generate_module(module);
        }
        finished_modules.push(self.module);

        let finished_module = finished_modules.pop().unwrap();
        for module in finished_modules.into_iter() {
            if let Err(msg) = finished_module.link_in_module(module) {
                panic!(format!("LLVM linking error: {}", msg.to_string()))
            }
        }
        finished_module
            .verify()
            .map_err(|e| println!("{}", e.to_string()))
            .unwrap();
        self.mpm.run_on(&finished_module);
        finished_module
    }

    fn generate_module(&mut self, mir: MIRModule) {
        // Put all functions into the variables map first
        for (name, func) in mir.functions.iter().chain(mir.imported_func.iter()) {
            let func_val = if let MIRType::Function(func) = &func._type {
                let func = func.borrow();
                self.module
                    .add_function(&name, self.get_fn_type(&func), None)
            } else {
                panic!("that function ain't a function...")
            };

            self.variables.insert(
                PtrEqRc::new(func),
                func_val.as_global_value().as_pointer_value(),
            );
        }

        mir.types
            .into_iter()
            .for_each(|struc| self.struc(struc.1.borrow()));
        mir.functions
            .into_iter()
            .for_each(|(name, func)| self.function(name, func));

        self.mpm.run_on(&self.module);
    }

    /// Generates a struct and its body
    fn struc(&mut self, _struc: Ref<MIRStruct>) {
        let struc_val = self.types[&_struc.name];
        let body: Vec<BasicTypeEnum> = _struc
            .member_order
            .iter()
            .map(|mem| self.to_ir_type_no_ptr(&mem._type))
            .collect();
        struc_val.set_body(body.as_slice(), false);
    }

    /// Generates a function; function should already be declared in the module.
    fn function(&mut self, name: Rc<String>, func: Rc<MIRVariable>) {
        let func_val = self.module.get_function(&name).unwrap();
        if let MIRType::Function(func) = &func._type {
            let func = func.borrow_mut();
            if !func.blocks.is_empty() {
                self.function_body(func, func_val)
            }
        }
    }

    /// Generates a function's body.
    fn function_body(&mut self, mut func: RefMut<MIRFunction>, func_val: FunctionValue) {
        self.blocks.clear();

        let entry_b = func.blocks.remove_entry(&self.entry_const).unwrap();
        let entry_bb = self.context.append_basic_block(&func_val, &entry_b.0);

        // First, build all function alloca
        self.builder.position_at_end(&entry_bb);
        for (arg, arg_val) in func.parameters.iter().zip(func_val.get_param_iter()) {
            // If the type of the function parameter is a pointer (aka a struct or function),
            // creating an alloca isn't needed; the pointer can be used directly.
            if let BasicValueEnum::PointerValue(ptr) = arg_val {
                self.variables.insert(PtrEqRc::new(arg), ptr);
            } else {
                let alloca = self
                    .builder
                    .build_alloca(self.unwrap_ptr(arg_val.get_type()), &arg.name);
                self.builder.build_store(alloca, arg_val);
                self.variables.insert(PtrEqRc::new(arg), alloca);
            }
        }

        for (name, var) in func.variables.iter() {
            let alloca = self
                .builder
                .build_alloca(self.to_ir_type_no_ptr(&var._type), &name);
            self.variables.insert(PtrEqRc::new(var), alloca);
        }

        // Fill in all blocks first before generating any actual code;
        // otherwise referencing a block yet to be built would result in a panic
        for (name, _block) in func.blocks.iter() {
            let bb = self.context.append_basic_block(&func_val, name);
            self.blocks.insert(Rc::clone(name), bb);
        }

        for expression in entry_b.1.expressions.iter() {
            self.generate_expression(expression);
        }
        self.build_bb_end(&entry_b.1, entry_bb);

        // Fill all blocks
        for (name, block) in func.blocks.iter() {
            let bb = self.get_block(name);
            self.fill_basic_block(block, bb)
        }

        // Set the block terminators
        for (name, block) in func.blocks.iter() {
            let bb = self.get_block(name);
            self.build_bb_end(block, bb);
        }
    }

    fn fill_basic_block(&mut self, mir_bb: &MIRBlock, block: BasicBlock) {
        match block.get_first_instruction() {
            Some(inst) => self.builder.position_before(&inst),
            None => self.builder.position_at_end(&block),
        }

        for expression in mir_bb.expressions.iter() {
            self.generate_expression(expression);
        }
    }

    fn build_bb_end(&mut self, mir_bb: &MIRBlock, block: BasicBlock) {
        self.builder.position_at_end(&block);
        match &mir_bb.last {
            MIRFlow::None => self.builder.build_return(None),

            MIRFlow::Jump(block) => self
                .builder
                .build_unconditional_branch(&self.get_block(block)),

            MIRFlow::Branch {
                condition,
                then_b,
                else_b,
            } => {
                let condition = self.generate_expression(condition);
                if let BasicValueEnum::IntValue(condition) = condition {
                    self.builder.build_conditional_branch(
                        condition,
                        &self.get_block(then_b),
                        &self.get_block(else_b),
                    )
                } else {
                    panic!("br condition wasn't a boolean");
                }
            }

            MIRFlow::Switch { cases, default } => {
                // TODO: meh
                let cases: Vec<(IntValue, BasicBlock)> = cases
                    .iter()
                    .map(|(expr, block)| {
                        (
                            *self.generate_expression(expr).as_int_value(),
                            self.get_block(block),
                        )
                    })
                    .collect();
                let cases: Vec<(IntValue, &BasicBlock)> =
                    cases.iter().map(|(expr, block)| (*expr, block)).collect();

                self.builder.build_switch(
                    self.context.bool_type().const_int(1, false),
                    &self.get_block(default),
                    cases.as_slice(),
                )
            }

            MIRFlow::Return(value) => {
                let value = self.generate_expression(value);
                if value.get_type() == self.none_const.get_type() {
                    self.builder.build_return(None)
                } else {
                    self.builder.build_return(Some(&value))
                }
            }
        };
    }

    fn get_block(&self, name: &String) -> BasicBlock {
        self.blocks[name]
    }

    fn get_variable(&self, var: &Rc<MIRVariable>) -> PointerValue {
        let wrap = PtrEqRc::new(var);
        self.variables[&wrap]
    }

    fn generate_expression(&mut self, expression: &MIRExpression) -> BasicValueEnum {
        match expression {
            MIRExpression::Binary {
                left,
                operator,
                right,
            } => {
                let left = *self.generate_expression(left).as_int_value();
                let right = *self.generate_expression(right).as_int_value();

                BasicValueEnum::IntValue(match operator {
                    Type::Plus => self.builder.build_int_add(left, right, "add"),
                    Type::Minus => self.builder.build_int_sub(left, right, "sub"),
                    Type::Star => self.builder.build_int_mul(left, right, "mul"),
                    Type::Slash => {
                        let left = self.builder.build_signed_int_to_float(
                            left,
                            self.context.f64_type(),
                            "divconv",
                        );
                        let right = self.builder.build_signed_int_to_float(
                            right,
                            self.context.f64_type(),
                            "divconv",
                        );
                        let float_div = self.builder.build_float_div(left, right, "div");
                        self.builder.build_float_to_signed_int(
                            float_div,
                            self.context.i64_type(),
                            "divconv",
                        )
                    }

                    _ => {
                        self.builder
                            .build_int_compare(get_predicate(*operator), left, right, "cmp")
                    }
                })
            }

            MIRExpression::Bitcast { object, goal } => {
                let object = self.generate_expression(object);
                let type_ = self.to_ir_type(&MIRType::Struct(Rc::clone(goal)));
                self.builder.build_bitcast(object, type_, "bitcast")
            }

            MIRExpression::Call { callee, arguments } => {
                let callee = self.generate_expression(callee);
                if let BasicValueEnum::PointerValue(ptr) = callee {
                    let arguments: Vec<BasicValueEnum> = arguments
                        .iter()
                        .map(|arg| self.generate_expression(arg))
                        .collect();

                    let ret = self
                        .builder
                        .build_call(ptr, arguments.as_slice(), "call")
                        .try_as_basic_value();
                    ret.left().unwrap_or(self.none_const)
                } else {
                    panic!("Call target wasn't a function pointer");
                }
            }

            MIRExpression::DoRet => {
                self.builder.clear_insertion_position();
                self.none_const
            }

            MIRExpression::Function(func) => BasicValueEnum::PointerValue(
                self.module
                    .get_function(&func.borrow().name)
                    .unwrap()
                    .as_global_value()
                    .as_pointer_value(),
            ),

            MIRExpression::Phi(branches) => {
                // Insert block might be None if block return was hit
                let cur_block = match self.builder.get_insert_block() {
                    Some(b) => b,
                    None => return self.none_const,
                };

                let branches: Vec<(BasicValueEnum, BasicBlock)> = branches
                    .iter()
                    .map(|(expr, br)| {
                        let block = self.get_block(br);
                        match block.get_terminator() {
                            Some(inst) => self.builder.position_before(&inst),
                            None => self.builder.position_at_end(&block),
                        }
                        (self.generate_expression(expr), block)
                    })
                    .collect();
                let _type = branches[0].0.get_type();

                let branches_ref: Vec<(&dyn BasicValue, &BasicBlock)> = branches
                    .iter()
                    .map(|(expr, br)| (expr as &dyn BasicValue, br))
                    .collect();

                self.builder.position_at_end(&cur_block);
                let phi = self.builder.build_phi(_type, "phi");
                phi.add_incoming(branches_ref.as_slice());
                phi.as_basic_value()
            }

            MIRExpression::StructGet { object, index } => {
                let struc = self.generate_expression(object);
                if let BasicValueEnum::PointerValue(ptr) = struc {
                    let ptr = unsafe { self.builder.build_struct_gep(ptr, *index, "classgep") };
                    self.load_ptr(ptr)
                } else {
                    panic!("Get target wasn't a struct")
                }
            }

            MIRExpression::StructSet {
                object,
                index,
                value,
            } => {
                let struc = self.generate_expression(object);
                if let BasicValueEnum::PointerValue(ptr) = struc {
                    let ptr = unsafe { self.builder.build_struct_gep(ptr, *index, "classgep") };
                    let value = self.generate_expression(value);
                    let value = self.unwrap_value_ptr(value);
                    self.builder.build_store(ptr, value);
                    value
                } else {
                    panic!("Get target wasn't a struct")
                }
            }

            MIRExpression::Literal(literal) => match literal {
                Literal::Any => self.none_const,
                Literal::None => self.none_const,
                Literal::Bool(value) => self
                    .context
                    .bool_type()
                    .const_int(*value as u64, false)
                    .into(),

                Literal::I8(num) => self.context.i8_type().const_int(*num as u64, false).into(),
                Literal::I16(num) => self.context.i16_type().const_int(*num as u64, false).into(),
                Literal::I32(num) => self.context.i32_type().const_int(*num as u64, false).into(),
                Literal::I64(num) => self.context.i64_type().const_int(*num as u64, false).into(),

                Literal::F32(num) => self.context.f32_type().const_float((*num).into()).into(),
                Literal::F64(num) => self.context.f64_type().const_float(*num).into(),

                Literal::String(string) => {
                    // If the builder's insert position is not set, creating a global string pointer
                    // will segfault (https://github.com/TheDan64/inkwell/issues/32)
                    // This is usually only the case when a return expression unset the position
                    // earlier, in which case the actual value doesn't matter anyway.
                    if self.builder.get_insert_block().is_none() {
                        self.none_const
                    } else {
                        let const_str =
                            self.builder.build_global_string_ptr(&string, "literal-str");
                        BasicValueEnum::PointerValue(const_str.as_pointer_value())
                    }
                }

                _ => panic!("unknown literal"),
            },

            MIRExpression::Unary { right, operator } => {
                let expr = self.generate_expression(right);

                match expr {
                    BasicValueEnum::IntValue(int) => match operator {
                        Type::Bang => self.builder.build_not(int, "unarynot"),
                        Type::Minus => self.builder.build_int_neg(int, "unaryneg"),
                        _ => panic!("Invalid unary operator"),
                    }
                    .as_basic_value_enum(),

                    BasicValueEnum::FloatValue(float) => self
                        .builder
                        .build_float_neg(float, "unaryneg")
                        .as_basic_value_enum(),

                    _ => panic!("Invalid unary operator"),
                }
            }

            MIRExpression::VarGet(var) => self.load_ptr(self.get_variable(var)),

            MIRExpression::VarStore { var, value } => {
                let variable = self.get_variable(var);
                let value = self.generate_expression(value);

                // String pointers should not be unwrapped
                let value = if var._type == MIRType::String {
                    value
                } else {
                    self.unwrap_value_ptr(value)
                };

                self.builder.build_store(variable, value);
                value
            }
        }
    }

    /// Loads a pointer, turning it into a value.
    /// Does not load structs or functions, since they are only ever used as pointers.
    fn load_ptr(&mut self, ptr: PointerValue) -> BasicValueEnum {
        match ptr.get_type().get_element_type() {
            AnyTypeEnum::FunctionType(_) => BasicValueEnum::PointerValue(ptr),
            AnyTypeEnum::StructType(_) => BasicValueEnum::PointerValue(ptr),
            _ => self.builder.build_load(ptr, "var"),
        }
    }

    /// If the parameter 'ptr' is a struct pointer, the struct itself is returned.
    /// Otherwise, ptr is simply returned without modification.
    fn unwrap_ptr(&self, ty: BasicTypeEnum) -> BasicTypeEnum {
        if let BasicTypeEnum::PointerType(ptr) = ty {
            if let AnyTypeEnum::StructType(struc) = ptr.get_element_type() {
                return struc.as_basic_type_enum();
            }
        }
        ty
    }

    /// Same as [unwrap_ptr], but for values instead of types.
    fn unwrap_value_ptr(&self, val: BasicValueEnum) -> BasicValueEnum {
        if let BasicValueEnum::PointerValue(ptr) = val {
            if let AnyTypeEnum::StructType(_) = ptr.get_type().get_element_type() {
                return self.builder.build_load(ptr, "ptr-load");
            }
        }
        val
    }

    /// Converts a MIRType to the corresponding LLVM type.
    /// Structs/Arrays are returned as PointerType<StructType/ArrayType>.
    fn to_ir_type(&self, mir: &MIRType) -> BasicTypeEnum {
        let ir = self.to_ir_type_no_ptr(mir);
        match ir {
            BasicTypeEnum::StructType(struc) if ir != self.none_const.get_type() => {
                struc.ptr_type(AddressSpace::Generic).as_basic_type_enum()
            }
            _ => ir,
        }
    }

    /// Converts a MIRType to the corresponding LLVM type. Structs are returned as StructType.
    fn to_ir_type_no_ptr(&self, mir: &MIRType) -> BasicTypeEnum {
        match mir {
            MIRType::Any => self.none_const.get_type(),
            MIRType::None => self.none_const.get_type(),
            MIRType::Bool => self.context.bool_type().as_basic_type_enum(),

            MIRType::I8 => self.context.i8_type().as_basic_type_enum(),
            MIRType::I16 => self.context.i16_type().as_basic_type_enum(),
            MIRType::I32 => self.context.i32_type().as_basic_type_enum(),
            MIRType::I64 => self.context.i64_type().as_basic_type_enum(),

            MIRType::F32 => self.context.f32_type().as_basic_type_enum(),
            MIRType::F64 => self.context.f64_type().as_basic_type_enum(),

            MIRType::Array(type_) => self
                .to_ir_type(type_)
                .ptr_type(AddressSpace::Generic)
                .as_basic_type_enum(),

            MIRType::String => self
                .context
                .i8_type()
                .ptr_type(AddressSpace::Generic)
                .as_basic_type_enum(),
            MIRType::Function(func) => self
                .get_fn_type(&func.borrow())
                .ptr_type(AddressSpace::Generic)
                .as_basic_type_enum(),
            MIRType::Struct(struc) => self.types[&struc.borrow().name].as_basic_type_enum(),
        }
    }

    /// Generates the LLVM FunctionType of a MIR function.
    fn get_fn_type(&self, func: &Ref<MIRFunction>) -> FunctionType {
        let params: Vec<BasicTypeEnum> = func
            .parameters
            .iter()
            .map(|param| self.to_ir_type(&param._type))
            .collect();

        let ret_type = self.to_ir_type(&func.ret_type);
        if ret_type == self.none_const.get_type() {
            self.context.void_type().fn_type(params.as_slice(), false)
        } else {
            ret_type.fn_type(params.as_slice(), false)
        }
    }

    pub fn new() -> IRGenerator {
        let context = Context::create();
        let module = context.create_module("main");
        let builder = context.create_builder();

        let mpm = PassManager::create(());
        mpm.add_instruction_combining_pass();
        mpm.add_reassociate_pass();
        mpm.add_cfg_simplification_pass();
        mpm.add_basic_alias_analysis_pass();

        // Break tests
        // mpm.add_dead_arg_elimination_pass();
        // mpm.add_dead_store_elimination_pass();
        // mpm.add_global_dce_pass();
        // mpm.add_tail_call_elimination_pass();

        // Cause segfaults
        // mpm.add_gvn_pass();
        // mpm.add_loop_deletion_pass();
        // mpm.add_loop_unswitch_pass();
        // mpm.add_promote_memory_to_register_pass();

        mpm.add_instruction_combining_pass();
        mpm.add_reassociate_pass();

        let none_const = context
            .struct_type(&[BasicTypeEnum::IntType(context.bool_type())], true)
            .const_named_struct(&[BasicValueEnum::IntValue(
                context.bool_type().const_int(0, false),
            )]);

        IRGenerator {
            context,
            module,
            builder,
            mpm,

            variables: HashMap::with_capacity(10),
            blocks: HashMap::with_capacity(10),

            types: HashMap::with_capacity(10),
            none_const: none_const.into(),
            entry_const: String::from("entry"),
        }
    }
}

/// A Rc that can be compared by checking for pointer equality.
/// Used as a HashMap key to allow unique keys with the same data.
struct PtrEqRc<T: Hash>(Rc<T>);

impl<T: Hash> PtrEqRc<T> {
    fn new(rc: &Rc<T>) -> PtrEqRc<T> {
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

fn get_predicate(tok: Type) -> IntPredicate {
    match tok {
        Type::Greater => IntPredicate::SGT,
        Type::GreaterEqual => IntPredicate::SGE,
        Type::Less => IntPredicate::SLT,
        Type::LessEqual => IntPredicate::SLE,
        Type::EqualEqual => IntPredicate::EQ,
        Type::BangEqual => IntPredicate::NE,
        _ => panic!("invalid tok"),
    }
}
