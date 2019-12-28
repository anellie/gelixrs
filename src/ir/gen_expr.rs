/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/28/19 9:01 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::{
    ast::Literal,
    ir::{IRGenerator, PtrEqRc},
    lexer::token::TType,
    mir::{
        get_iface_impls,
        nodes::{Expr, Flow, Function, Type, Variable},
        MutRc,
    },
};
use inkwell::{
    types::{AnyTypeEnum, BasicType, BasicTypeEnum, StructType},
    values::{BasicValue, BasicValueEnum, PointerValue},
    AddressSpace::Generic,
    FloatPredicate, IntPredicate,
};
use std::rc::Rc;

impl IRGenerator {
    pub fn expression(&mut self, expression: &Expr) -> BasicValueEnum {
        match expression {
            Expr::Allocate { type_, heap } => {
                let ty = self.ir_ty(type_);
                self.create_alloc(ty, heap.get()).into()
            }

            Expr::Binary {
                left,
                operator,
                right,
            } => {
                let left = self.expression(left);
                let right = self.expression(right);
                self.binary(left, *operator, right)
            }

            Expr::Call { callee, arguments } => {
                let callee = self.expression(callee);
                let (ptr, first_arg) = self.function_from_callee(callee);
                self.build_call(ptr, arguments.iter(), first_arg)
            }

            Expr::CallDyn {
                index, arguments, ..
            } => self.call_dyn(*index, arguments),

            Expr::CastToInterface { object, to, store } => {
                self.cast_to_interface(object, to, store)
            }

            Expr::ConstructClosure {
                function,
                global,
                captured,
            } => self.construct_closure(function, global, captured),

            Expr::Flow(flow) => self.flow(flow),

            Expr::ModifyRefCount { object, count } => self.modify_ref_count(object, *count),

            Expr::Phi(branches) => self.phi(branches),

            Expr::StructGet { object, index } => {
                let struc = self.expression(object);
                let ptr = self.struct_gep(struc.into_pointer_value(), *index);
                self.load_ptr(ptr)
            }

            Expr::StructSet {
                object,
                index,
                value,
            } => {
                let struc = self.expression(object);
                let ptr = self.struct_gep(struc.into_pointer_value(), *index);
                let value = self.expression(value);
                self.builder.build_store(ptr, value);
                value
            }

            Expr::Literal(literal) => self.literal(literal),

            Expr::Unary { right, operator } => self.unary(right, *operator),

            Expr::VarGet(var) => self.load_ptr(self.get_variable(var)),

            Expr::VarStore { var, value } => {
                let var = self.get_variable(var);
                let val = self.expression(value);

                self.builder.build_store(var, val);
                val
            }
        }
    }

    fn binary(
        &self,
        left: BasicValueEnum,
        operator: TType,
        right: BasicValueEnum,
    ) -> BasicValueEnum {
        match left.get_type() {
            BasicTypeEnum::IntType(_) => {
                let left = left.into_int_value();
                let right = right.into_int_value();
                BasicValueEnum::IntValue(match operator {
                    TType::Plus => self.builder.build_int_add(left, right, "add"),
                    TType::Minus => self.builder.build_int_sub(left, right, "sub"),
                    TType::Star => self.builder.build_int_mul(left, right, "mul"),
                    TType::Slash => self.builder.build_int_signed_div(left, right, "div"),
                    _ => {
                        self.builder
                            .build_int_compare(get_predicate(operator), left, right, "cmp")
                    }
                })
            }

            BasicTypeEnum::FloatType(_) => {
                let left = left.into_float_value();
                let right = right.into_float_value();
                BasicValueEnum::FloatValue(match operator {
                    TType::Plus => self.builder.build_float_add(left, right, "add"),
                    TType::Minus => self.builder.build_float_sub(left, right, "sub"),
                    TType::Star => self.builder.build_float_mul(left, right, "mul"),
                    TType::Slash => self.builder.build_float_div(left, right, "div"),
                    _ => {
                        return BasicValueEnum::IntValue(self.builder.build_float_compare(
                            get_float_predicate(operator),
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

    fn function_from_callee(
        &mut self,
        callee: BasicValueEnum,
    ) -> (PointerValue, Option<BasicValueEnum>) {
        let ptr = callee.into_pointer_value();
        match ptr.get_type().get_element_type() {
            // Regular function call
            AnyTypeEnum::FunctionType(_) => (ptr, None),

            // Closure call with captured as first arg
            AnyTypeEnum::StructType(_) => (
                self.load_ptr(self.struct_gep(ptr, 0)).into_pointer_value(),
                Some(self.load_ptr(self.struct_gep(ptr, 1))),
            ),

            _ => panic!("Invalid callee"),
        }
    }

    fn call_dyn(&mut self, index: usize, arguments: &[Expr]) -> BasicValueEnum {
        let mut arguments = arguments.iter();

        let iface_struct = self.expression(arguments.next().unwrap());
        let iface_ptr = iface_struct.into_pointer_value();

        let vtable_ptr = self
            .load_ptr(self.struct_gep(iface_ptr, 1))
            .into_pointer_value();
        let method_ptr = self
            .load_ptr(self.struct_gep(vtable_ptr, index))
            .into_pointer_value();

        let implementor_ptr = self
            .load_ptr(self.struct_gep(iface_ptr, 0))
            .into_pointer_value();
        self.build_call(method_ptr, arguments, Some(implementor_ptr.into()))
    }

    fn build_call<'a, T: Iterator<Item = &'a Expr>>(
        &mut self,
        ptr: PointerValue,
        arguments: T,
        first_arg: Option<BasicValueEnum>,
    ) -> BasicValueEnum {
        let arguments: Vec<BasicValueEnum> = first_arg
            .into_iter()
            .chain(arguments.map(|a| self.expression(a)))
            .collect();
        let ret = self
            .builder
            .build_call(ptr, &arguments, "call")
            .try_as_basic_value();
        ret.left().unwrap_or(self.none_const)
    }

    fn cast_to_interface(&mut self, object: &Expr, to: &Type, store: &Expr) -> BasicValueEnum {
        let obj = self.expression(object);
        let iface_ty = self.ir_ty(to).into_struct_type();
        let vtable_ty = iface_ty.get_field_types()[2]
            .as_pointer_type()
            .get_element_type()
            .into_struct_type();

        let vtable = self.get_vtable(object.get_type(), to, vtable_ty);
        let store_location = self.expression(store);
        self.write_struct(
            store_location.into_pointer_value(),
            [self.coerce_to_void_ptr(obj), vtable].iter(),
        );
        store_location
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
        let mut field_tys = field_tys.iter().skip(1);
        let impls = get_iface_impls(&implementor).unwrap();
        let impls = impls.borrow();
        let methods_iter = impls.interfaces[iface]
            .methods
            .iter()
            .map(|(_, method)| self.functions[&PtrEqRc::new(method)])
            .map(|func| {
                self.builder.build_bitcast(
                    func.as_global_value().as_pointer_value(),
                    *field_tys.next().unwrap().as_pointer_type(),
                    "funccast",
                )
            });
        let methods = Some(self.context.i32_type().const_int(0, false).into())
            .into_iter()
            .chain(methods_iter)
            .collect::<Vec<BasicValueEnum>>();
        let global = self.module.add_global(vtable, None, "vtable");
        global.set_initializer(&vtable.const_named_struct(&methods));
        global.as_pointer_value().into()
    }

    fn construct_closure(
        &mut self,
        function: &MutRc<Function>,
        global: &Rc<Variable>,
        captured: &Rc<Vec<Rc<Variable>>>,
    ) -> BasicValueEnum {
        let func_ptr = self.cast_first_param_to_i64(self.get_variable(global));

        let captured_ty = self.ir_ty(&Type::ClosureCaptured(Rc::clone(captured)));
        let captured_vals = self.create_captured_values(captured_ty, captured);
        let captured_vals = self
            .builder
            .build_ptr_to_int(captured_vals, self.context.i64_type(), "captcast")
            .into();

        let ty = self.ir_ty(&function.borrow().to_closure_type());
        let alloc = self.create_alloc(ty, true);
        self.write_struct(alloc, [func_ptr, captured_vals].iter());
        alloc.into()
    }

    /// Takes a PointerValue<FunctionValue> and casts its type
    /// to have the first parameter type be replaced with i64.
    fn cast_first_param_to_i64(&self, val: PointerValue) -> BasicValueEnum {
        let func_ty = val.get_type().get_element_type().into_function_type();
        let mut params = func_ty.get_param_types();
        params[0] = self.context.i64_type().into();
        let func_ty = if let Some(ret_type) = func_ty.get_return_type() {
            ret_type.fn_type(&params, false)
        } else {
            self.context.void_type().fn_type(&params, false)
        };
        self.builder
            .build_bitcast(val, func_ty.ptr_type(Generic), "bc")
    }

    fn create_captured_values(
        &mut self,
        ty: BasicTypeEnum,
        captured: &[Rc<Variable>],
    ) -> PointerValue {
        let alloc = self.create_alloc(ty, true);
        for (i, var) in captured.iter().enumerate() {
            let value = self.load_ptr(self.get_variable(var));
            self.builder.build_store(self.struct_gep(alloc, i), value);
        }
        alloc
    }

    fn flow(&mut self, flow: &Flow) -> BasicValueEnum {
        // If still inserting, set insertion position to the end of the block.
        // It might not be at the end if a Phi node in another block was compiled first,
        // in which case the insertion position is before that phi expression.
        match self.builder.get_insert_block() {
            Some(b) => self.builder.position_at_end(&b),
            None => return self.none_const,
        }

        match flow {
            Flow::None => self.builder.build_return(None),

            Flow::Jump(block) => self.builder.build_unconditional_branch(&self.blocks[block]),

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
                let cases: Vec<_> = cases
                    .iter()
                    .map(|(expr, block)| {
                        (self.expression(expr).into_int_value(), self.blocks[block])
                    })
                    .collect();
                let cases: Vec<_> = cases.iter().map(|(expr, block)| (*expr, block)).collect();

                self.builder.build_switch(
                    self.context.bool_type().const_int(1, false),
                    &self.blocks[default],
                    &cases,
                )
            }

            Flow::Return(value) => {
                let value = self.expression(value);
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

    fn modify_ref_count(&mut self, object: &Expr, count: u64) -> BasicValueEnum {
        let object = self.expression(object);
        // IRGenerator::struct_gep can't be used here as it
        // only allows indexing past the refcount field.
        let int = unsafe {
            self.builder
                .build_struct_gep(object.into_pointer_value(), 0, "rcgep")
        };

        let new = self.builder.build_int_add(
            self.load_ptr(int).into_int_value(),
            self.context.i32_type().const_int(count, false).into(),
            "rcadd",
        );

        self.builder.build_store(int, new);
        object
    }

    fn phi(&mut self, branches: &[(Expr, Rc<String>)]) -> BasicValueEnum {
        // Block inserting into might be None if block return was hit
        let cur_block = match self.builder.get_insert_block() {
            Some(b) => b,
            None => return self.none_const,
        };

        let branches: Vec<_> = branches
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

        let branches_ref: Vec<_> = branches
            .iter()
            .map(|(expr, br)| (expr as &dyn BasicValue, br))
            .collect();

        self.builder.position_at_end(&cur_block);
        let phi = self.builder.build_phi(type_, "phi");
        phi.add_incoming(&branches_ref);
        phi.as_basic_value()
    }

    fn literal(&mut self, literal: &Literal) -> BasicValueEnum {
        match literal {
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
                    self.builder
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
                        .unwrap()
                }
            }

            _ => panic!("unknown literal"),
        }
    }

    fn unary(&mut self, right: &Expr, operator: TType) -> BasicValueEnum {
        let expr = self.expression(right);
        match expr {
            BasicValueEnum::IntValue(int) => match operator {
                TType::Bang => self.builder.build_not(int, "unarynot"),
                TType::Minus => self.builder.build_int_neg(int, "unaryneg"),
                _ => panic!("Invalid unary operator"),
            }
            .into(),

            BasicValueEnum::FloatValue(float) => {
                self.builder.build_float_neg(float, "unaryneg").into()
            }

            _ => panic!("Invalid unary operator"),
        }
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
