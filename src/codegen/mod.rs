pub mod resolver;

use super::{
    ast::{
        declaration::{Declaration, Function, Variable},
        expression::Expression,
        literal::Literal,
        statement::Statement,
    },
    lexer::token::{Token, Type},
};
use inkwell::{
    builder::Builder,
    context::Context,
    module::Module,
    passes::PassManager,
    types::BasicType,
    values::{BasicValueEnum, FunctionValue, PointerValue},
    IntPredicate,
};
use std::collections::HashMap;
use std::convert::TryInto;

/// A generator that creates LLVM IR.
/// Created through a [Resolver].
pub struct IRGenerator<'i> {
    /// LLVM-related. Refer to their docs for more info.
    context: Context,
    builder: Builder,
    module: Module,
    fpm: PassManager<FunctionValue>,

    // All variables in the current scope and the currently compiled function.
    variables: HashMap<String, PointerValue>,
    current_fn: Option<FunctionValue>,

    // All statements remaining to be compiled. Reverse order.
    declarations: Vec<Declaration<'i>>,
}

impl<'i> IRGenerator<'i> {
    /// Generates IR. Will process all statements given.
    pub fn generate(&mut self) {
        while !self.declarations.is_empty() {
            let declaration = self.declarations.pop().unwrap();
            let result = self.declaration(declaration);

            if let Err(msg) = result {
                eprintln!("Error during code generation: {}", msg); // TODO: Maybe some more useful error messages at some point
                break;
            }
        }

        self.module.print_to_stderr();
    }

    /// Compiles a single top-level declaration
    fn declaration(&mut self, declaration: Declaration) -> Result<(), &'static str> {
        match declaration {
            Declaration::CFunc(_) => Ok(()), // Resolver already declared it; nothing to be done here
            Declaration::Function(func) => self.function(func),
            _ => Err("Encountered unimplemented declaration."),
        }
    }

    /// Compiles a function to IR. (The function was already declared by the resolver.)
    fn function(&mut self, func: Function) -> Result<(), &'static str> {
        let function = self.module.get_function(func.sig.name.lexeme).ok_or("Internal error: Undefined function.")?;

        let entry = self.context.append_basic_block(&function, "entry");
        self.builder.position_at_end(&entry);

        self.current_fn = Some(function);

        self.variables.reserve(func.sig.parameters.len());
        for (i, arg) in function.get_param_iter().enumerate() {
            let arg_name = func.sig.parameters[i].1.lexeme;
            let alloca = self.create_entry_block_alloca(arg.get_type(), arg_name);
            self.builder.build_store(alloca, arg);
            self.variables.insert(func.sig.parameters[i].1.lexeme.to_string(), alloca);
        }

        if func.sig.return_type.is_some() {
            if let Statement::Expression(expr) = *func.body {
                let body = self.expression(expr)?;
                self.builder.build_return(Some(&body));
            } else {
                return Err("Function has a return type but returns void.");
            }
        } else {
            self.statement(*func.body)?;
            self.builder.build_return(None);
        }

        if function.verify(true) {
            // self.fpm.run_on(&function); todo
            Ok(())
        } else {
            unsafe { function.delete(); }
            Err("Invalid generated function.")
        }
    }

    /// Compile a statement. Statements are only found in functions.
    fn statement(&mut self, statement: Statement) -> Result<(), &'static str> {
        match statement {
            Statement::Block(statements) => self.block_statement(statements),
            Statement::If { condition, then_branch, else_branch } => self.if_statement(*condition, *then_branch, else_branch),
            Statement::Variable(var) => self.var_statement(var),
            Statement::Expression(expr) => { 
                self.expression(expr)?; 
                Ok(()) 
            },
            _ => Err("Encountered unimplemented statement."),
        }
    }

    fn block_statement(&mut self, mut statements: Vec<Statement>) -> Result<(), &'static str> {
        statements.reverse();
        loop {
            if statements.is_empty() { break Ok(()) }
            self.statement(statements.pop().ok_or("Empty block?")?)?;
        }
    }

    fn if_statement(&mut self, condition: Expression, then_b: Statement, else_b: Option<Box<Statement>>) -> Result<(), &'static str> {
        let parent = self.cur_fn();
        let condition = self.expression(condition)?;

        if let BasicValueEnum::IntValue(value) = condition {
            let condition = self.builder.build_int_compare(IntPredicate::NE, value, self.context.bool_type().const_int(0, false), "ifcond");

            let then_bb = self.context.append_basic_block(&parent, "then");
            let else_bb = self.context.append_basic_block(&parent, "else");
            let cont_bb = self.context.append_basic_block(&parent, "ifcont");

            if else_b.is_none() {
                self.builder.build_conditional_branch(condition, &then_bb, &cont_bb);
            } else {
                self.builder.build_conditional_branch(condition, &then_bb, &else_bb);
            }

            self.builder.position_at_end(&then_bb);
            self.statement(then_b)?;
            self.builder.build_unconditional_branch(&cont_bb);

            self.builder.position_at_end(&else_bb);
            if let Some(else_b) = else_b {
                self.statement(*else_b)?;
            }
            self.builder.build_unconditional_branch(&cont_bb);

            self.builder.position_at_end(&cont_bb);
            Ok(())
        } else {
            Err("If condition needs to be a boolean.")
        }
    }

    fn var_statement(&mut self, var: Variable) -> Result<(), &'static str> {
        let initial_value = self.expression(var.initializer)?;
        let alloca = self.create_entry_block_alloca(initial_value.get_type(), var.name.lexeme);

        self.builder.build_store(alloca, initial_value);
        self.variables.insert(var.name.lexeme.to_string(), alloca);

        Ok(())
    }

    fn expression(&mut self, expression: Expression) -> Result<BasicValueEnum, &'static str> {
        Ok(match expression {
            Expression::Assignment { name, value } => self.assignment(name, *value)?,
            Expression::Binary { left, operator, right } => self.binary(*left, operator, *right)?,
            Expression::Block(expressions) => self.block_expr(expressions)?,
            Expression::Call { callee, token: _, arguments } => self.call_expr(*callee, arguments)?,
            Expression::IfElse { condition, then_branch, else_branch } => self.if_expr(*condition, *then_branch, *else_branch)?,
            Expression::Literal(literal) => self.literal(literal),
            Expression::Variable(name) => self.variable(name)?,
            _ => Err("Encountered unimplemented expression.")?,
        })
    }

    fn assignment(&mut self, name: Token, value: Expression) -> Result<BasicValueEnum, &'static str> {
        let value = self.expression(value)?;
        let var = self.variables.get(name.lexeme).ok_or("Undefined variable.")?;

        self.builder.build_store(*var, value);
        Ok(value)
    }

    fn block_expr(&mut self, mut statements: Vec<Statement>) -> Result<BasicValueEnum, &'static str> {
        statements.reverse();
        loop {
            if statements.len() == 1 {
                if let Statement::Expression(expr) = statements.pop().ok_or("Empty block?")? {
                    break self.expression(expr)
                } else {
                    panic!("Internal error: Block was incorrectly classified!");
                }
            }

            self.statement(statements.pop().ok_or("Empty block?")?)?;
        }
    }

    // TODO: Add float support
    // TODO: Make this less ugly
    fn binary(&mut self, left: Expression, operator: Token, right: Expression) -> Result<BasicValueEnum, &'static str> {
        let left = self.expression(left)?;
        let right = self.expression(right)?;

        let left = if let BasicValueEnum::IntValue(int) = left { int } else { Err("Only int are supported for math operations.")? };
        let right = if let BasicValueEnum::IntValue(int) = right { int } else { Err("Only int are supported for math operations.")? };

        Ok(BasicValueEnum::IntValue(match operator.t_type {
            Type::Plus => self.builder.build_int_add(left, right, "tmpadd"),
            Type::Minus => self.builder.build_int_sub(left, right, "tmpsub"),
            Type::Star => self.builder.build_int_mul(left, right, "tmpmul"),
            Type::Slash => {
                let left = self.builder.build_signed_int_to_float(left, self.context.f64_type(), "tmpdivconv");
                let right = self.builder.build_signed_int_to_float(right, self.context.f64_type(), "tmpdivconv");
                let float_div = self.builder.build_float_div(left, right, "tmpdiv");
                self.builder.build_float_to_signed_int(float_div, self.context.i64_type(), "tmpdivconv")
            },

            Type::Greater => self.builder.build_int_compare(IntPredicate::SGT, left, right, "tmpcmp"),
            Type::GreaterEqual => self.builder.build_int_compare(IntPredicate::SGE, left, right, "tmpcmp"),
            Type::Less => self.builder.build_int_compare(IntPredicate::SLT, left, right, "tmpcmp"),
            Type::LessEqual => self.builder.build_int_compare(IntPredicate::SLE, left, right, "tmpcmp"),

            Type::EqualEqual => self.builder.build_int_compare(IntPredicate::EQ, left, right, "tmpcmp"),
            Type::BangEqual => self.builder.build_int_compare(IntPredicate::NE, left, right, "tmpcmp"),
            _ => Err("Unsupported binary operand.")?
        }))
    }

    fn call_expr(&mut self, callee: Expression, arguments: Vec<Expression>) -> Result<BasicValueEnum, &'static str> {
        if let Expression::Variable(token) = callee {
            let function = self.module.get_function(token.lexeme).ok_or("Unknown function.")?;
            let mut compiled_args = Vec::with_capacity(arguments.len());

            for arg in arguments {
                compiled_args.push(self.expression(arg)?);
            }

            let argsv: Vec<BasicValueEnum> = compiled_args.iter().by_ref().map(|&val| val.into()).collect();
            self.builder.build_call(function, argsv.as_slice(), "tmp").try_as_basic_value().left().ok_or("Invalid call.")
        } else {
            Err("Unsupported callee.")?
        }
    }

    fn if_expr(&mut self, condition: Expression, then_b: Expression, else_b: Expression) -> Result<BasicValueEnum, &'static str> {
        let parent = self.cur_fn();
        let condition = self.expression(condition)?;

        if let BasicValueEnum::IntValue(value) = condition {
            let condition = self.builder.build_int_compare(IntPredicate::NE, value, self.context.bool_type().const_int(0, false), "ifcond");

            let then_bb = self.context.append_basic_block(&parent, "then");
            let else_bb = self.context.append_basic_block(&parent, "else");
            let cont_bb = self.context.append_basic_block(&parent, "ifcont");

            self.builder.build_conditional_branch(condition, &then_bb, &else_bb);

            self.builder.position_at_end(&then_bb);
            let then_val = self.expression(then_b)?;
            self.builder.build_unconditional_branch(&cont_bb);
            let then_bb = self.builder.get_insert_block().unwrap();

            self.builder.position_at_end(&else_bb);
            let else_val = self.expression(else_b)?;
            self.builder.build_unconditional_branch(&cont_bb);
            let else_bb = self.builder.get_insert_block().unwrap();

            self.builder.position_at_end(&cont_bb);
            let phi = self.builder.build_phi(then_val.get_type(), "ifphi");
            phi.add_incoming(&[
                (&then_val, &then_bb),
                (&else_val, &else_bb)
            ]);

            self.builder.position_at_end(&cont_bb);
            Ok(phi.as_basic_value())
        } else {
            Err("If condition needs to be a boolean or integer.")
        }
    }

    // TODO: Array literals
    fn literal(&mut self, literal: Literal) -> BasicValueEnum {
        match literal {
            Literal::Bool(value) => BasicValueEnum::IntValue(self.context.bool_type().const_int(value as u64, false)),
            Literal::Int(num) => BasicValueEnum::IntValue(self.context.i64_type().const_int(num.try_into().unwrap(), false)),
            Literal::Float(num) => BasicValueEnum::FloatValue(self.context.f32_type().const_float(num.into())),
            Literal::Double(num) => BasicValueEnum::FloatValue(self.context.f64_type().const_float(num)),
            Literal::String(string) => BasicValueEnum::VectorValue(self.context.const_string(&string, false)),
            _ => panic!("What is that?")
        }
    }

    fn variable(&mut self, name: Token) -> Result<BasicValueEnum, &'static str> {
        match self.variables.get(name.lexeme) {
            Some(var) => Ok(self.builder.build_load(*var, name.lexeme)),
            None => Err("Could not find variable."),
        }
    }

    fn create_entry_block_alloca<T: BasicType>(&self, ty: T, name: &str) -> PointerValue {
        let builder = self.context.create_builder();
        let entry = self.cur_fn().get_first_basic_block().unwrap();

        match entry.get_first_instruction() {
            Some(inst) => builder.position_before(&inst),
            None => builder.position_at_end(&entry),
        }

        builder.build_alloca(ty, name)
    }

    fn cur_fn(&self) -> FunctionValue {
        self.current_fn.unwrap()
    }
}
