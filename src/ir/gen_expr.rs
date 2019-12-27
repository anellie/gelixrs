/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 4:58 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::ir::{IRGenerator, PtrEqRc};
use inkwell::values::{BasicValueEnum, PointerValue, BasicValue, IntValue};
use crate::mir::get_iface_impls;
use inkwell::types::{StructType, FunctionType, AnyTypeEnum, BasicTypeEnum, BasicType};
use std::rc::Rc;
use crate::mir::nodes::{Expr, Flow, Type, Variable};
use crate::lexer::token::TType;
use crate::ast::Literal;
use inkwell::basic_block::BasicBlock;
use inkwell::{AddressSpace, IntPredicate, FloatPredicate};

impl IRGenerator {
    pub fn expression(&mut self, expression: &Expr) -> BasicValueEnum {
        match expression {
            Expr::Binary {
                left,
                operator,
                right,
            } => {
                let left = self.expression(left);
                let right = self.expression(right);

                match left.get_type() {
                    BasicTypeEnum::IntType(_) => {
                        let left = left.into_int_value();
                        let right = right.into_int_value();
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
                let callee = self.expression(callee);
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
                    .map(|arg| self.expression(arg))
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
                    .map(|arg| self.expression(arg))
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
                let obj = self.expression(object);
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
                        .build_unconditional_branch(&self.blocks[block]),

                    Flow::Branch {
                        condition,
                        then_b,
                        else_b,
                    } => {
                        let condition = self.expression(condition);
                        self.builder.build_conditional_branch(
                            *condition.as_int_value(),
                            &self.blocks[then_b],
                            &self.blocks[else_b],
                        )
                    }

                    Flow::Switch { cases, default } => {
                        let cases: Vec<(IntValue, BasicBlock)> = cases
                            .iter()
                            .map(|(expr, block)| {
                                (
                                    *self.expression(expr).as_int_value(),
                                    self.blocks[block],
                                )
                            })
                            .collect();
                        let cases: Vec<(IntValue, &BasicBlock)> =
                            cases.iter().map(|(expr, block)| (*expr, block)).collect();

                        self.builder.build_switch(
                            self.context.bool_type().const_int(1, false),
                            &self.blocks[default],
                            cases.as_slice(),
                        )
                    }

                    Flow::Return(value) => {
                        let value = self.expression(value);
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
                        let block = self.blocks[br];
                        match block.get_terminator() {
                            Some(inst) => self.builder.position_before(&inst),
                            None => self.builder.position_at_end(&block),
                        }
                        (self.expression(expr), block)
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
                let struc = self.expression(object);
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
                let struc = self.expression(object);
                let ptr = unsafe {
                    self.builder
                        .build_struct_gep(struc.into_pointer_value(), *index, "classgep")
                };
                let value = self.expression(value);
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
                let expr = self.expression(right);

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
                let value = self.expression(value);

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
