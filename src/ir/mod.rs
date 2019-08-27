/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/27/19 9:19 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use super::{
    ast::literal::Literal,
    lexer::token::Type,
    mir::{
        mir::{MIRStruct, MIRFunction, MIRType, MIRBlock, MIRExpression, MIRFlow, MIRVariable},
        MIR,
    }
};
use inkwell::{
    basic_block::BasicBlock,
    builder::Builder,
    context::Context,
    module::Module,
    passes::PassManager,
    types::{AnyTypeEnum, BasicTypeEnum, BasicType, FunctionType, StructType},
    values::{BasicValue, BasicValueEnum, FunctionValue, PointerValue},
    AddressSpace, IntPredicate,
};
use std::{
    cell::{Ref, RefMut},
    collections::HashMap, 
    convert::TryInto, 
    rc::Rc,
};
use std::hash::{Hash, Hasher};

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

    // All variables, the currently compiled function, and its blocks.
    // Note that not all variables are valid - they are kept after going out of scope.
    // This is not an issue since the MIR generator checked against this already.
    variables: HashMap<PtrEqRc<MIRVariable>, PointerValue>,
    current_fn: Option<FunctionValue>,
    blocks: HashMap<Rc<String>, BasicBlock>,

    // All types (classes) that are available.
    types: HashMap<Rc<String>, StructType>,

    // A constant that is used for expressions that don't produce a value but are required to.
    none_const: BasicValueEnum,
    // Simply a string called "entry". Needed to find the entry block of functions.
    entry_const: String
}

impl IRGenerator {
    /// Generates IR. Will process MIR given.
    pub fn generate(mut self, mir: MIR) -> Module {
        // Put all functions into the variables map first
        for (name, func) in mir.functions.iter() {
            let func_val = if let MIRType::Function(func) = &func._type {
                let func = func.borrow();
                self.module.add_function(&name, self.get_fn_type(&func), None)
            } else { panic!("that function ain't a function...") };

            self.variables.insert(PtrEqRc::new(func), func_val.as_global_value().as_pointer_value());
        }

        mir.types.into_iter().for_each(|struc| self.struc(struc.borrow()));
        mir.functions
            .into_iter()
            .for_each(|(name, func)| self.function(name, func));

        self.mpm.run_on(&self.module);
        self.module
    }

    fn struc(&mut self, _struc: Ref<MIRStruct>) {
        unimplemented!()
    }

    fn function(&mut self, name: Rc<String>, func: Rc<MIRVariable>) {
        let func_val = self.module.get_function(&name).unwrap();
        self.current_fn = Some(func_val.clone());
        if let MIRType::Function(func) = &func._type {
            let func = func.borrow_mut();
            if !func.blocks.is_empty() {
                self.function_body(func, func_val)
            }
        }
    }

    fn function_body(&mut self, mut func: RefMut<MIRFunction>, func_val: FunctionValue) {
        self.blocks.clear();

        let entry_b = func.blocks.remove_entry(&self.entry_const).unwrap();
        let entry_bb = self.context.append_basic_block(&func_val, &entry_b.0);

        self.builder.position_at_end(&entry_bb);
        for (arg, arg_val) in func.parameters.iter().zip(func_val.get_param_iter()) {
            let alloca = self.builder.build_alloca(arg_val.get_type(), &arg.name);
            self.builder.build_store(alloca, arg_val);
            self.variables.insert(PtrEqRc::new(arg), alloca);
        }
        for (name, var) in func.variables.iter() {
            let alloca = self.builder.build_alloca(self.to_ir_type(&var._type), &name);
            self.variables.insert(PtrEqRc::new(var), alloca);
        }

        // Fill in all blocks first before generating any actual code;
        // otherwise referencing a block yet to be built would result in a panic
        for (name, block) in func.blocks.iter() {
            let bb = self.context.append_basic_block(&func_val, name);
            self.blocks.insert(Rc::clone(name), bb);
        }

        self.fill_basic_block(&entry_b.1, entry_bb);

        for (name, block) in func.blocks.iter() {
            let bb = self.get_block(name);
            self.fill_basic_block(block, bb);
        }
    }

    fn fill_basic_block(&mut self, mir_bb: &MIRBlock, block: BasicBlock) {
        self.builder.position_at_end(&block);

        for expression in mir_bb.expressions.iter() {
            self.generate_expression(expression);
        }

        match &mir_bb.last {
            MIRFlow::Jump(block) => self.builder.build_unconditional_branch(&self.get_block(block)),

            MIRFlow::Branch { condition, then_b, else_b } => {
                let condition = self.generate_expression(condition);
                if let BasicValueEnum::IntValue(condition) = condition {
                    self.builder.build_conditional_branch(
                        condition,
                        &self.get_block(then_b),
                        &self.get_block(else_b)
                    )
                } else {
                    panic!("br condition wasn't a boolean");
                }
            },

            MIRFlow::Return(value) => {
                if let Some(value) = value {
                    let value = self.generate_expression(value);
                    self.builder.build_return(Some(&value))
                } else {
                    self.builder.build_return(None)
                }
            },
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
            // TODO: This is verbose and not very flexible.
            // Should really be replaced with operator overloading in gelix or similar
            MIRExpression::Binary { left, operator, right } => {
                let left = self.generate_expression(left);
                let right = self.generate_expression(right);

                let left = if let BasicValueEnum::IntValue(int) = left { int } else { panic!("Only int are supported for math operations") };
                let right = if let BasicValueEnum::IntValue(int) = right { int } else { panic!("Only int are supported for math operations") };

                BasicValueEnum::IntValue(match operator.t_type {
                    Type::Plus => self.builder.build_int_add(left, right, "add"),
                    Type::Minus => self.builder.build_int_sub(left, right, "sub"),
                    Type::Star => self.builder.build_int_mul(left, right, "mul"),
                    Type::Slash => {
                        let left = self.builder.build_signed_int_to_float(left, self.context.f64_type(), "divconv");
                        let right = self.builder.build_signed_int_to_float(right, self.context.f64_type(), "divconv");
                        let float_div = self.builder.build_float_div(left, right, "div");
                        self.builder.build_float_to_signed_int(float_div, self.context.i64_type(), "divconv")
                    },

                    Type::Greater => self.builder.build_int_compare(IntPredicate::SGT, left, right, "cmp"),
                    Type::GreaterEqual => self.builder.build_int_compare(IntPredicate::SGE, left, right, "cmp"),
                    Type::Less => self.builder.build_int_compare(IntPredicate::SLT, left, right, "cmp"),
                    Type::LessEqual => self.builder.build_int_compare(IntPredicate::SLE, left, right, "cmp"),

                    Type::EqualEqual => self.builder.build_int_compare(IntPredicate::EQ, left, right, "cmp"),
                    Type::BangEqual => self.builder.build_int_compare(IntPredicate::NE, left, right, "cmp"),

                    _ => panic!("Unsupported binary operand"),
                })
            },

            MIRExpression::Call { callee, arguments } => {
                let callee = self.generate_expression(callee);
                if let BasicValueEnum::PointerValue(ptr) = callee {
                    let arguments: Vec<BasicValueEnum> = arguments
                        .iter()
                        .map(|arg| self.generate_expression(arg))
                        .collect();

                    let ret = self.builder.build_call(ptr, arguments.as_slice(), "call").try_as_basic_value();
                    ret.left().unwrap_or(self.none_const)
                } else {
                    panic!("Call target wasn't a function pointer");
                }
            },

            MIRExpression::Function(func) => {
                BasicValueEnum::PointerValue(
                    self.module
                        .get_function(&func.borrow().name)
                        .unwrap()
                        .as_global_value()
                        .as_pointer_value()
                )
            }

            MIRExpression::Phi(branches) => {
                let branches: Vec<(BasicValueEnum, BasicBlock)> = branches
                    .iter()
                    .map(|(expr, br)| (self.generate_expression(expr), self.get_block(br)))
                    .collect();
                let _type = branches[0].0.get_type();

                let branches_ref: Vec<(&dyn BasicValue, &BasicBlock)> = branches
                    .iter()
                    .map(|(expr, br)| (expr as &dyn BasicValue, br))
                    .collect();

                let phi = self.builder.build_phi(_type, "phi");
                phi.add_incoming(branches_ref.as_slice());
                phi.as_basic_value()
            }

            MIRExpression::StructGet { object, index } => {
                let struc = self.generate_expression(object);
                if let BasicValueEnum::PointerValue(ptr) = struc {
                    let ptr = unsafe { self.builder.build_struct_gep(ptr, *index, "classgep") };
                    BasicValueEnum::PointerValue(ptr)
                } else {
                    panic!("Get target wasn't a struct")
                }
            },

            MIRExpression::StructSet { object, index, value } => {
                let struc = self.generate_expression(object);
                if let BasicValueEnum::PointerValue(ptr) = struc {
                    let ptr = unsafe { self.builder.build_struct_gep(ptr, *index, "classgep") };
                    let value = self.generate_expression(value);
                    self.builder.build_store(ptr, value);
                    value
                } else {
                    panic!("Get target wasn't a struct")
                }
            },

            MIRExpression::Literal(literal) => {
                match literal {
                    Literal::None => self.none_const,
                    Literal::Bool(value) => self
                        .context
                        .bool_type()
                        .const_int(*value as u64, false)
                        .into(),
                    Literal::Int(num) => self
                        .context
                        .i64_type()
                        .const_int((*num).try_into().unwrap(), false)
                        .into(),
                    Literal::Float(num) => self.context.f32_type().const_float((*num).into()).into(),
                    Literal::Double(num) => self.context.f64_type().const_float(*num).into(),
                    Literal::String(string) => {
                        let const_str = self.builder.build_global_string_ptr(&string, "literal-str");
                        BasicValueEnum::PointerValue(const_str.as_pointer_value())
                    }
                    _ => panic!("What is that?"),
                }
            },

            // TODO: This is stupidly verbose
            MIRExpression::Unary { operator, right } => {
                let expr = self.generate_expression(right);
                match operator.t_type {
                    Type::Minus => match expr {
                        BasicValueEnum::IntValue(int) => {
                            BasicValueEnum::IntValue(self.builder.build_int_neg(int.into(), "unaryneg"))
                        }

                        BasicValueEnum::FloatValue(float) => BasicValueEnum::FloatValue(
                            self.builder.build_float_neg(float.into(), "unaryneg"),
                        ),

                        _ => panic!("Invalid unary negation"),
                    },

                    Type::Bang => {
                        if let BasicValueEnum::IntValue(int) = expr {
                            BasicValueEnum::IntValue(self.builder.build_int_neg(int, "unaryinv"))
                        } else {
                            panic!("Invalid unary binary negation")
                        }
                    }

                    _ => panic!("Invalid unary operator"),
                }
            },

            MIRExpression::VarGet(var) => {
                let var = self.get_variable(var);
                // If it is a function, loading it isn't be possible.
                if let AnyTypeEnum::FunctionType(_) = var.get_type().get_element_type() {
                    BasicValueEnum::PointerValue(var)
                } else {
                    self.builder.build_load(var, "var")
                }
            },

            MIRExpression::VarStore { var, value } => {
                let variable = self.get_variable(var);
                let value = self.generate_expression(value);
                self.builder.build_store(variable, value);
                value
            },
        }
    }

    fn to_ir_type(&self, mir: &MIRType) -> BasicTypeEnum {
        match mir {
            MIRType::None => self.none_const.get_type(),
            MIRType::Bool => self.context.bool_type().as_basic_type_enum(),
            MIRType::Int => self.context.i64_type().as_basic_type_enum(),
            MIRType::Float => self.context.f32_type().as_basic_type_enum(),
            MIRType::Double => self.context.f64_type().as_basic_type_enum(),
            MIRType::String => self
                .context
                .i8_type()
                .ptr_type(AddressSpace::Generic)
                .as_basic_type_enum(),
            MIRType::Function(func) => self.get_fn_type(&func.borrow()).ptr_type(AddressSpace::Generic).as_basic_type_enum(),
            MIRType::Struct(struc) => self.types[&struc.borrow().name].ptr_type(AddressSpace::Generic).as_basic_type_enum()
        }
    }

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

    fn cur_fn(&self) -> FunctionValue {
        self.current_fn.unwrap()
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
            current_fn: None,
            blocks: HashMap::with_capacity(10),

            types: HashMap::with_capacity(10),
            none_const: none_const.into(),
            entry_const: String::from("entry")
        }
    }
}

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

impl<T: Hash> Eq for PtrEqRc<T> {
}

impl<T: Hash> Hash for PtrEqRc<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}
