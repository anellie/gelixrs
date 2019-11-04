/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 11/4/19 8:03 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{
    cell::{Ref, RefMut},
    collections::HashMap,
    hash::{Hash, Hasher},
    rc::Rc,
};

use inkwell::{AddressSpace, basic_block::BasicBlock, builder::Builder, context::Context, FloatPredicate, IntPredicate, module::Module, passes::PassManager, types::{AnyTypeEnum, BasicType, BasicTypeEnum, FunctionType, StructType}, values::{BasicValue, BasicValueEnum, FunctionValue, IntValue, PointerValue}};

use super::{
    ast::literal::Literal,
    lexer::token::TType,
    mir::{
        MIRModule, MutRc,
        nodes::{Block, Class, Expression, Flow, Function, Type, Variable},
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
    /// Simply a string called "entry". Used to find the entry block of functions.
    entry_const: String,
}

impl IRGenerator {
    /// Generates IR. Will process all MIR modules given.
    pub fn generate(mut self, mir: Vec<MIRModule>) -> Module {
        for module in &mir {
            for (_, function) in &module.functions {
                self.declare_function(function);
            }
        }

        for module in mir {
            for (_, function) in module.functions {
                self.function(function);
            }
        }

        self.module
            .verify()
            .map_err(|e| println!("{}", e.to_string()))
            .unwrap();
        self.mpm.run_on(&self.module);
        self.module
    }

    /// Generates a type, if it was not found in self.types.
    fn build_type(&mut self, ty: &Type) -> BasicTypeEnum {
        let ir_ty = match ty {
            Type::Function(func) => self
                .get_fn_type(&func.borrow())
                .ptr_type(AddressSpace::Generic)
                .as_basic_type_enum(),

            Type::Class(class) => self.build_class(Rc::clone(class)).as_basic_type_enum(),

            _ => panic!("Unknown type to build")
        };
        self.types.insert(ty.clone(), ir_ty);
        ir_ty
    }

    /// Generates a class struct and its body.
    fn build_class(&mut self, class: MutRc<Class>) -> StructType {
        let struc_val = self.context.opaque_struct_type(&class.borrow().name);
        let body: Vec<BasicTypeEnum> = class
            .borrow()
            .members
            .iter()
            .map(|(_, mem)| self.to_ir_type_no_ptr(&mem.type_))
            .collect();
        struc_val.set_body(body.as_slice(), false);
        struc_val
    }

    /// Declares a function. All functions must be declared before generating
    /// code; as a reference to an undeclared function is otherwise possible
    /// (and leads to a panic)
    fn declare_function(&mut self, func: &Rc<Variable>) {
        let func_ty = func.type_.as_function();
        let fn_ty = self.get_fn_type(&func_ty.borrow());
        let func_val = self.module.add_function(&func_ty.borrow().name, fn_ty, None);
        self.functions.insert(PtrEqRc::new(&func), func_val);
    }

    /// Generates a function, should it have a body.
    fn function(&mut self, func_var: Rc<Variable>) {
        let func_ty = func_var.type_.as_function();
        let func = func_ty.borrow_mut();
        if !func.blocks.is_empty() {
            let func_val = self.functions[&PtrEqRc::new(&func_var)];
            self.function_body(func, func_val)
        }
    }

    /// Generates a function's body.
    fn function_body(&mut self, mut func: RefMut<Function>, func_val: FunctionValue) {
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
            let alloca_ty = self.to_ir_type_no_ptr(&var.type_);
            let alloca = self.builder.build_alloca(alloca_ty, &name);
            self.variables.insert(PtrEqRc::new(var), alloca);
        }

        // Fill in all blocks first before generating any actual code;
        // otherwise referencing a block yet to be built would result in a panic
        for (name, _block) in func.blocks.iter() {
            let bb = self.context.append_basic_block(&func_val, name);
            self.blocks.insert(Rc::clone(name), bb);
        }

        self.fill_basic_block(&entry_b.1, entry_bb);

        // Fill all blocks
        for (name, block) in func.blocks.iter() {
            let bb = self.get_block(name);
            self.position_at_block(bb);
            self.fill_basic_block(block, bb)
        }
    }

    fn position_at_block(&mut self, block: BasicBlock) {
        match block.get_first_instruction() {
            Some(inst) => self.builder.position_before(&inst),
            None => self.builder.position_at_end(&block),
        }
    }

    fn fill_basic_block(&mut self, mir_bb: &Block, block: BasicBlock) {
        for expression in mir_bb.iter() {
            self.generate_expression(expression);
        }

        // Blocks that return from a None-type function do not have a MIR terminator
        // so we create a void return
        if block.get_terminator().is_none() {
            self.builder.build_return(None);
        }
    }

    fn get_block(&self, name: &String) -> BasicBlock {
        self.blocks[name]
    }

    fn get_variable(&self, var: &Rc<Variable>) -> PointerValue {
        let wrap = PtrEqRc::new(var);
        self.variables.get(&wrap).cloned().unwrap_or_else(|| self.functions[&wrap].as_global_value().as_pointer_value())
    }

    fn generate_expression(&mut self, expression: &Expression) -> BasicValueEnum {
        match expression {
            Expression::Binary {
                left,
                operator,
                right,
            } => {
                let left = self.generate_expression(left);
                let right = self.generate_expression(right);

                match left.get_type() {
                    BasicTypeEnum::IntType(_) => {
                        let left = *left.as_int_value();
                        let right = *right.as_int_value();
                        BasicValueEnum::IntValue(match operator {
                            TType::Plus => self.builder.build_int_add(left, right, "add"),
                            TType::Minus => self.builder.build_int_sub(left, right, "sub"),
                            TType::Star => self.builder.build_int_mul(left, right, "mul"),
                            TType::Slash => self.builder.build_int_signed_div(left, right, "div"),
                            _ => self.builder.build_int_compare(get_predicate(*operator), left, right, "cmp")
                        })
                    },

                    BasicTypeEnum::FloatType(_) => {
                        let left = *left.as_float_value();
                        let right = *right.as_float_value();
                        BasicValueEnum::FloatValue(match operator {
                            TType::Plus => self.builder.build_float_add(left, right, "add"),
                            TType::Minus => self.builder.build_float_sub(left, right, "sub"),
                            TType::Star => self.builder.build_float_mul(left, right, "mul"),
                            TType::Slash => self.builder.build_float_div(left, right, "div"),
                            _ => return BasicValueEnum::IntValue(self.builder.build_float_compare(get_float_predicate(*operator), left, right, "cmp"))
                        })
                    }

                    _ => panic!("invalid binary operation"),
                }
            }

            Expression::Call { callee, arguments } => {
                let callee = self.generate_expression(callee);
                let ptr = callee.into_pointer_value();
                let arguments: Vec<BasicValueEnum> = arguments
                    .iter()
                    .map(|arg| self.generate_expression(arg))
                    .collect();

                let ret = self
                    .builder
                    .build_call(ptr, arguments.as_slice(), "call")
                    .try_as_basic_value();
                ret.left().unwrap_or(self.none_const)
            }

            Expression::Flow(flow) => {
                // If still inserting, set insertion position to the end of the block.
                // It might not be at the end if a Phi node in another block was compiled first,
                // in which case the insertion position is before that phi expression.
                match self.builder.get_insert_block() {
                    Some(b) => self.builder.position_at_end(&b),
                    None => return self.none_const,
                }

                match &**flow {
                    Flow::None => self.builder.build_return(None),

                    Flow::Jump(block) => self
                        .builder
                        .build_unconditional_branch(&self.get_block(block)),

                    Flow::Branch {
                        condition,
                        then_b,
                        else_b,
                    } => {
                        let condition = self.generate_expression(condition);
                        self.builder.build_conditional_branch(
                            *condition.as_int_value(),
                            &self.get_block(then_b),
                            &self.get_block(else_b),
                        )
                    }

                    Flow::Switch { cases, default } => {
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

                    Flow::Return(value) => {
                        let value = self.generate_expression(value);
                        if value.get_type() == self.none_const.get_type() {
                            self.builder.build_return(None)
                        } else {
                            self.builder.build_return(Some(&value))
                        }
                    }
                };
                self.builder.clear_insertion_position();
                self.none_const
            }

            Expression::Phi(branches) => {
                // Block inserting into might be None if block return was hit
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
                let type_ = branches[0].0.get_type();

                let branches_ref: Vec<(&dyn BasicValue, &BasicBlock)> = branches
                    .iter()
                    .map(|(expr, br)| (expr as &dyn BasicValue, br))
                    .collect();

                self.builder.position_at_end(&cur_block);
                let phi = self.builder.build_phi(type_, "phi");
                phi.add_incoming(branches_ref.as_slice());
                phi.as_basic_value()
            }

            Expression::StructGet { object, index } => {
                let struc = self.generate_expression(object);
                let ptr = unsafe { self.builder.build_struct_gep(struc.into_pointer_value(), *index, "classgep") };
                self.load_ptr(ptr)
            }

            Expression::StructSet {
                object,
                index,
                value,
            } => {
                let struc = self.generate_expression(object);
                let ptr = unsafe { self.builder.build_struct_gep(struc.into_pointer_value(), *index, "classgep") };
                let value = self.generate_expression(value);
                let value = self.unwrap_value_ptr(value);
                self.builder.build_store(ptr, value);
                value
            }

            Expression::Literal(literal) => match literal {
                Literal::Any | Literal::None => self.none_const,
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

            Expression::Unary { right, operator } => {
                let expr = self.generate_expression(right);

                match expr {
                    BasicValueEnum::IntValue(int) => match operator {
                        TType::Bang => self.builder.build_not(int, "unarynot"),
                        TType::Minus => self.builder.build_int_neg(int, "unaryneg"),
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

            Expression::VarGet(var) => self.load_ptr(self.get_variable(var)),

            Expression::VarStore { var, value } => {
                let variable = self.get_variable(var);
                let value = self.generate_expression(value);

                // String pointers should not be unwrapped
                let value = if var.type_ == Type::String {
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
    fn to_ir_type(&mut self, mir: &Type) -> BasicTypeEnum {
        let ir = self.to_ir_type_no_ptr(mir);
        match ir {
            BasicTypeEnum::StructType(struc) if ir != self.none_const.get_type() => {
                struc.ptr_type(AddressSpace::Generic).as_basic_type_enum()
            }
            _ => ir,
        }
    }

    /// Converts a MIRType to the corresponding LLVM type. Structs are returned as StructType.
    fn to_ir_type_no_ptr(&mut self, mir: &Type) -> BasicTypeEnum {
        self.types.get(mir).copied().unwrap_or_else(|| self.build_type(mir))
    }

    /// Generates the LLVM FunctionType of a MIR function.
    fn get_fn_type(&mut self, func: &Ref<Function>) -> FunctionType {
        let params: Vec<BasicTypeEnum> = func
            .parameters
            .iter()
            .map(|param| self.to_ir_type(&param.type_))
            .collect();

        if func.ret_type == Type::None {
            self.context.void_type().fn_type(params.as_slice(), false)
        } else {
            let ret_type = self.to_ir_type(&func.ret_type);
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

        let mut types = HashMap::with_capacity(20);
        types.insert(Type::Any, none_const.get_type().into());
        types.insert(Type::None, none_const.get_type().into());
        types.insert(Type::Bool, context.bool_type().into());

        types.insert(Type::I8, context.i8_type().into());
        types.insert(Type::I16, context.i16_type().into());
        types.insert(Type::I32, context.i32_type().into());
        types.insert(Type::I64, context.i64_type().into());

        types.insert(Type::F32, context.f32_type().into());
        types.insert(Type::F64, context.f64_type().into());

        types.insert(
            Type::String,
            context.i8_type().ptr_type(AddressSpace::Generic).into(),
        );

        IRGenerator {
            context,
            module,
            builder,
            mpm,

            variables: HashMap::with_capacity(10),
            blocks: HashMap::with_capacity(10),

            types,
            functions: HashMap::with_capacity(10),

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

fn get_predicate(tok: TType) -> IntPredicate {
    match tok {
        TType::Greater => IntPredicate::SGT,
        TType::GreaterEqual => IntPredicate::SGE,
        TType::Less => IntPredicate::SLT,
        TType::LessEqual => IntPredicate::SLE,
        TType::EqualEqual => IntPredicate::EQ,
        TType::BangEqual => IntPredicate::NE,
        _ => panic!("invalid tok"),
    }
}

fn get_float_predicate(tok: TType) -> FloatPredicate {
    match tok {
        TType::Greater => FloatPredicate::OGT,
        TType::GreaterEqual => FloatPredicate::OGE,
        TType::Less => FloatPredicate::OLT,
        TType::LessEqual => FloatPredicate::OLE,
        TType::EqualEqual => FloatPredicate::OEQ,
        TType::BangEqual => FloatPredicate::ONE,
        _ => panic!("invalid tok"),
    }
}
