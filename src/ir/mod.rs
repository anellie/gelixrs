/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 3:32 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::path::Path;
use std::{
    cell::RefMut,
    collections::HashMap,
    hash::{Hash, Hasher},
    rc::Rc,
};

use inkwell::{
    basic_block::BasicBlock,
    builder::Builder,
    context::Context,
    module::Module,
    types::{AnyTypeEnum, BasicType, BasicTypeEnum, FunctionType, StructType},
    values::{BasicValue, BasicValueEnum, FunctionValue, IntValue, PointerValue},
    AddressSpace, FloatPredicate, IntPredicate,
};

use super::{
    ast::literal::Literal,
    lexer::token::TType,
    mir::{
        nodes::{Block, Expr, Flow, Function, Type, Variable},
        MModule, MutRc,
    },
};
use crate::ir::intrinsics::fill_intrinsic_functions;
use crate::mir::get_iface_impls;

mod intrinsics;
mod types;
mod values;

/// A generator that creates LLVM IR out of Gelix mid-level IR (MIR).
///
/// Will panic when encountering invalid code; this should not happen however thanks to the
/// MIRGenerator validating the MIR it generates.
pub struct IRGenerator {
    // LLVM-related. Refer to their docs for more info.
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
    /// Simply a string called "entry". Used to find the entry block of functions.
    entry_const: String,
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

        fill_intrinsic_functions(
            &mut self,
            mir.iter()
                .find(|m| {
                    let module = m.borrow();
                    **module.path.0[0] == *"std" && **module.path.0[1] == *"intrinsics"
                })
                .unwrap(),
        );

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

        let entry_b = func.blocks.remove_entry(&self.entry_const).unwrap();
        let entry_bb = self.context.append_basic_block(&func_val, &entry_b.0);

        // First, build all function alloca
        self.builder.position_at_end(&entry_bb);
        self.build_parameter_alloca(&func, func_val);

        for (name, var) in func.variables.iter() {
            let alloca_ty = self.ir_ty(&var.type_);
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

    fn build_parameter_alloca(&mut self, func: &RefMut<Function>, func_val: FunctionValue) {
        for (arg, arg_val) in func.parameters.iter().zip(func_val.get_param_iter()) {
            if let Type::ClosureCaptured(captured) = &arg.type_ {
                // If this is the first arg on a closure containing all captured variables,
                // 'unpack' them and create separate entries for each in self.variables
                // so they can be used like regular variables.
                let arg_val = *arg_val.as_pointer_value();
                for (i, var) in captured.iter().enumerate() {
                    unsafe {
                        let field =
                            self.builder
                                .build_struct_gep(arg_val, i as u32, "capture-unwrap");
                        self.variables.insert(PtrEqRc::new(var), field);
                    }
                }
            } else if let BasicValueEnum::PointerValue(ptr) = arg_val {
                // If the type of the function parameter is a pointer (aka a struct or function),
                // creating an alloca isn't needed; the pointer can be used directly.
                self.variables.insert(PtrEqRc::new(arg), ptr);
            } else {
                let alloca = self.builder.build_alloca(arg_val.get_type(), &arg.name);
                self.builder.build_store(alloca, arg_val);
                self.variables.insert(PtrEqRc::new(arg), alloca);
            }
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

        self.variables
            .get(&wrap)
            .cloned()
            .unwrap_or_else(|| self.functions[&wrap].as_global_value().as_pointer_value())
    }

    fn generate_expression(&mut self, expression: &Expr) -> BasicValueEnum {
        match expression {
            Expr::Binary {
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
                            _ => self.builder.build_int_compare(
                                get_predicate(*operator),
                                left,
                                right,
                                "cmp",
                            ),
                        })
                    }

                    BasicTypeEnum::FloatType(_) => {
                        let left = *left.as_float_value();
                        let right = *right.as_float_value();
                        BasicValueEnum::FloatValue(match operator {
                            TType::Plus => self.builder.build_float_add(left, right, "add"),
                            TType::Minus => self.builder.build_float_sub(left, right, "sub"),
                            TType::Star => self.builder.build_float_mul(left, right, "mul"),
                            TType::Slash => self.builder.build_float_div(left, right, "div"),
                            _ => {
                                return BasicValueEnum::IntValue(self.builder.build_float_compare(
                                    get_float_predicate(*operator),
                                    left,
                                    right,
                                    "cmp",
                                ))
                            }
                        })
                    }

                    _ => panic!("invalid binary operation: {:?}", left.get_type()),
                }
            }

            Expr::Call { callee, arguments } => {
                let callee = self.generate_expression(callee);
                let ptr = callee.into_pointer_value();
                let (ptr, first_arg) = match ptr.get_type().get_element_type() {
                    // Regular function call
                    AnyTypeEnum::FunctionType(_) => (ptr, None),
                    // Closure call with captured as first arg
                    AnyTypeEnum::StructType(_) => unsafe {
                        (
                            self.builder
                                .build_load(
                                    self.builder.build_struct_gep(ptr, 0, "closuregep"),
                                    "closureload",
                                )
                                .into_pointer_value(),
                            Some(self.builder.build_load(
                                self.builder.build_struct_gep(ptr, 1, "captgep"),
                                "captload",
                            )),
                        )
                    },
                    _ => panic!("Invalid callee"),
                };

                let mut arguments: Vec<BasicValueEnum> = arguments
                    .iter()
                    .map(|arg| self.generate_expression(arg))
                    .collect();
                if let Some(arg) = first_arg {
                    arguments.insert(0, arg);
                }

                let ret = self
                    .builder
                    .build_call(ptr, arguments.as_slice(), "call")
                    .try_as_basic_value();
                let ret = ret.left().unwrap_or(self.none_const);
                self.build_local_store(ret)
            }

            Expr::CallDyn {
                index, arguments, ..
            } => {
                let mut arguments: Vec<BasicValueEnum> = arguments
                    .iter()
                    .map(|arg| self.generate_expression(arg))
                    .collect();

                let iface_struct = arguments[0];
                let iface_ptr = iface_struct.into_pointer_value();
                let method_ptr = unsafe {
                    // First index the interface struct; index 0 is the vtable
                    let vtable = self.builder.build_struct_gep(iface_ptr, 1, "vtablegep");
                    let vtable = *self
                        .builder
                        .build_load(vtable, "loadtable")
                        .as_pointer_value();

                    // Then index the method in the vtable
                    let method = self
                        .builder
                        .build_struct_gep(vtable, *index as u32, "methodgep");
                    *self
                        .builder
                        .build_load(method, "loadmethod")
                        .as_pointer_value()
                };

                let implementor_ptr = unsafe {
                    let ptr = self.builder.build_struct_gep(iface_ptr, 0, "implgep");
                    *self
                        .builder
                        .build_load(ptr, "loadmethod")
                        .as_pointer_value()
                };
                arguments[0] = implementor_ptr.into();

                let ret = self
                    .builder
                    .build_call(method_ptr, arguments.as_slice(), "call")
                    .try_as_basic_value();
                let ret = ret.left().unwrap_or(self.none_const);
                self.build_local_store(ret)
            }

            Expr::CastToInterface { object, to } => {
                let implementor = object.get_type();
                let obj = self.generate_expression(object);
                let iface_struct_ty: StructType = *self.ir_ty(to).as_struct_type();
                let vtable_ty = *iface_struct_ty.get_field_types()[1]
                    .as_pointer_type()
                    .get_element_type()
                    .as_struct_type();

                let vtable = self.get_vtable(implementor, to, vtable_ty);
                self.create_tmp_iface_struct(iface_struct_ty, obj, vtable)
            }

            Expr::ConstructClosure {
                function,
                global,
                captured,
            } => {
                let func_ptr = self.get_variable(global);
                let func_ty: FunctionType =
                    *func_ptr.get_type().get_element_type().as_function_type();
                let mut params = func_ty.get_param_types();
                params[0] = self.context.i64_type().into();
                let func_ty = func_ty.get_return_type().unwrap().fn_type(&params, false);
                let func_ptr = self.builder.build_bitcast(
                    func_ptr,
                    func_ty.ptr_type(AddressSpace::Generic),
                    "ccast",
                );

                let captured_ty = self
                    .ir_ty(&Type::ClosureCaptured(Rc::clone(captured)))
                    .into_struct_type();
                let captured_vals = self.create_captured_values(captured_ty, captured);
                let captured_vals = self
                    .builder
                    .build_ptr_to_int(captured_vals, self.context.i64_type(), "captcast")
                    .into();

                let ty = self.ir_ty(&function.borrow().to_closure_type());
                self.build_local_struct(ty, [func_ptr, captured_vals].iter())
            }

            Expr::Flow(flow) => {
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
                        } else if let BasicValueEnum::PointerValue(ptr) = value {
                            self.builder
                                .build_return(Some(&self.builder.build_load(ptr, "retload")))
                        } else {
                            self.builder.build_return(Some(&value))
                        }
                    }
                };
                self.builder.clear_insertion_position();
                self.none_const
            }

            Expr::Phi(branches) => {
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

            Expr::StructGet { object, index } => {
                let struc = self.generate_expression(object);
                let ptr = unsafe {
                    self.builder
                        .build_struct_gep(struc.into_pointer_value(), *index, "classgep")
                };
                self.load_ptr(ptr)
            }

            Expr::StructSet {
                object,
                index,
                value,
            } => {
                let struc = self.generate_expression(object);
                let ptr = unsafe {
                    self.builder
                        .build_struct_gep(struc.into_pointer_value(), *index, "classgep")
                };
                let value = self.generate_expression(value);
                self.builder.build_store(ptr, self.unwrap_struct_ptr(value));
                value
            }

            Expr::Literal(literal) => match literal {
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
                        let const_str = self.builder.build_global_string_ptr(&string, "str");
                        let const_str = self.builder.build_ptr_to_int(
                            const_str.as_pointer_value(),
                            self.context.i64_type(),
                            "strtoint",
                        );
                        let string_builder = self
                            .module
                            .get_function("std/intrinsics:build_string_literal")
                            .unwrap();
                        let string = self
                            .builder
                            .build_call(
                                string_builder,
                                &[
                                    const_str.into(),
                                    self.context
                                        .i64_type()
                                        .const_int((string.len() + 1) as u64, false)
                                        .into(),
                                ],
                                "str",
                            )
                            .try_as_basic_value()
                            .left()
                            .unwrap();
                        self.build_local_store(string)
                    }
                }

                _ => panic!("unknown literal"),
            },

            Expr::Unary { right, operator } => {
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

            Expr::VarGet(var) => self.load_ptr(self.get_variable(var)),

            Expr::VarStore { var, value } => {
                let variable = self.get_variable(var);
                let value = self.generate_expression(value);

                self.builder
                    .build_store(variable, self.unwrap_struct_ptr(value));
                value
            }
        }
    }

    fn create_tmp_iface_struct(
        &self,
        ty: StructType,
        implementor: BasicValueEnum,
        vtable: BasicValueEnum,
    ) -> BasicValueEnum {
        self.build_local_struct(
            ty.into(),
            [self.coerce_to_void_ptr(implementor), vtable].iter(),
        )
    }

    fn create_captured_values(
        &mut self,
        ty: StructType,
        captured: &[Rc<Variable>],
    ) -> PointerValue {
        let value = self.builder.build_malloc(ty, "captmalloc");
        for (i, var) in captured.iter().enumerate() {
            let ptr = unsafe { self.builder.build_struct_gep(value, i as u32, "captgep") };
            let value = self.load_ptr(self.get_variable(var));
            self.builder.build_store(ptr, self.unwrap_struct_ptr(value));
        }
        value
    }

    /// Returns the vtable of the interface implementor given.
    /// Will generate functions as needed to fill the vtable.
    fn get_vtable(
        &mut self,
        implementor: Type,
        iface: &Type,
        vtable: StructType,
    ) -> BasicValueEnum {
        let field_tys = vtable.get_field_types();
        let mut field_tys = field_tys.iter();
        let methods = get_iface_impls(&implementor).unwrap().borrow().interfaces[iface]
            .methods
            .iter()
            .map(|(_, method)| self.functions[&PtrEqRc::new(method)])
            .map(|func| func.as_global_value().as_pointer_value())
            .map(|func| {
                self.builder.build_bitcast(
                    func,
                    *field_tys.next().unwrap().as_pointer_type(),
                    "funccast",
                )
            })
            .collect::<Vec<BasicValueEnum>>();
        let global = self.module.add_global(vtable, None, "vtable");
        global.set_initializer(&vtable.const_named_struct(&methods));
        global.as_pointer_value().into()
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
            entry_const: String::from("entry"),
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
