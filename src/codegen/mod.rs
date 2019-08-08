use super::{
    ast::{
        expression::Expression,
        literal::Literal,
        statement::{Function, Statement, Variable},
    },
    lexer::token::{Token, Type},
};
use inkwell::{
    builder::Builder,
    context::Context,
    module::Module,
    passes::PassManager,
    types::{BasicType},
    values::{BasicValueEnum, FunctionValue, PointerValue},
    IntPredicate,
};
use std::collections::HashMap;
use std::convert::TryInto;

/// A generator that creates LLVM IR from a vector of Statements.
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
    statements: Vec<Statement<'i>>,
}

impl<'i> IRGenerator<'i> {
    /// Generates IR. Will process all statements given.
    pub fn generate(&mut self) {
        let main_fn = self.declare_function(&Function {
                name: Token {
                    t_type: Type::Identifier,
                    lexeme: "entry",
                line: 0,
                },
                return_type: None,
                parameters: Vec::with_capacity(0),
                body: Box::new(Expression::This(Token { t_type: Type::Identifier, line: 0, lexeme: "NOPE" })),
        });

        let main_block = self.context.append_basic_block(&main_fn, "entry");
        self.builder.position_at_end(&main_block);
        self.current_fn = Some(main_fn);
        
        while !self.statements.is_empty() {
            let statement = self.statements.pop().unwrap();
            let result = self.statement(statement);

            // Ensure the builder is not in some other function that was created during the statement
            self.builder.position_at_end(&main_fn.get_last_basic_block().unwrap());

            if let Err(msg) = result {
                eprintln!("Error during code generation: {}", msg); // TODO: Maybe some more useful error messages at some point
                break;
            }
        }

        self.builder.build_return(None);
        
        if main_fn.verify(true) {
            // Currently, optimization will just clear the fn since it only consists of expressions with no side-effects.
            // self.fpm.run_on(&main_fn);
        }

        self.module.print_to_stderr();
    }

    fn statement(&mut self, statement: Statement) -> Result<(), &'static str> {
        match statement {
            Statement::Expression(expr) => { self.expression(expr)?; },
            Statement::Function(func) => { self.func_declaration(func)?; },
            Statement::Variable(var) => { self.var_declaration(var)?; },
            _ => return Err("Encountered unimplemented statement."),
        };

        Ok(())
    }

    fn func_declaration(&mut self, func: Function) -> Result<(), &'static str> {
        let function = self.declare_function(&func);

        let entry = self.context.append_basic_block(&function, "entry");
        self.builder.position_at_end(&entry);

        self.current_fn = Some(function);

        self.variables.reserve(func.parameters.len());
        for (i, arg) in function.get_param_iter().enumerate() {
            let arg_name = func.parameters[i].0.lexeme;
            let alloca = self.create_entry_block_alloca(arg.get_type(), arg_name);
            self.builder.build_store(alloca, arg);
            self.variables.insert(func.parameters[i].0.lexeme.to_string(), alloca);
        }

        let body = self.expression(*func.body)?;
        self.builder.build_return(None);

        if function.verify(true) {
            self.fpm.run_on(&function);
            Ok(())
        } else {
            unsafe { function.delete(); }
            Err("Invalid generated function.")
        }
    }

    fn var_declaration(&mut self, var: Variable) -> Result<(), &'static str> {
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
            Expression::If { condition, then_branch, else_branch } => self.if_expr(*condition, *then_branch, else_branch)?,
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

    // TODO: Add float support
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

    // TODO: Do if without else even work?
    fn if_expr(&mut self, condition: Expression, then_b: Expression, else_b: Option<Box<Expression>>) -> Result<BasicValueEnum, &'static str> {
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
            let then_val = self.expression(then_b)?;
            self.builder.build_unconditional_branch(&cont_bb);

            let then_bb = self.builder.get_insert_block().unwrap();

            self.builder.position_at_end(&cont_bb);
            let phi = self.builder.build_phi(self.context.i64_type(), "ifphi"); // todo

            if let Some(else_b) = else_b {
                self.builder.position_at_end(&else_bb);
                let else_val = self.expression(*else_b)?;
                self.builder.build_unconditional_branch(&cont_bb);
                let else_bb = self.builder.get_insert_block().unwrap();

                phi.add_incoming(&[
                    (&then_val, &then_bb),
                    (&else_val, &else_bb)
                ]);
            }

            self.builder.position_at_end(&cont_bb);

            Ok(phi.as_basic_value())
        } else {
            Err("If condition needs to be a boolean or integer.")
        }
    }

    fn literal(&mut self, literal: Literal) -> BasicValueEnum {
        match literal {
            Literal::Bool(value) => BasicValueEnum::IntValue(self.context.bool_type().const_int(value as u64, false)),
            Literal::Int(num) => BasicValueEnum::IntValue(self.context.i64_type().const_int(num.try_into().unwrap(), false)),
            Literal::Float(num) => BasicValueEnum::FloatValue(self.context.f32_type().const_float(num.into())),
            Literal::Double(num) => BasicValueEnum::FloatValue(self.context.f32_type().const_float(num)),
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

    fn declare_function(&mut self, func: &Function) -> FunctionValue {
        let fn_type = self.context.void_type().fn_type(&[], false); // todo
        self.module.add_function(func.name.lexeme, fn_type, None)
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

    /// Creates a new generator. Put into action by generate().
    pub fn new(mut statements: Vec<Statement>) -> IRGenerator {
        let context = Context::create();
        let module = context.create_module("main");
        let builder = context.create_builder();

        let fpm = PassManager::create(&module);
        fpm.add_instruction_combining_pass();
        fpm.add_reassociate_pass();
        fpm.add_gvn_pass();
        fpm.add_cfg_simplification_pass();
        fpm.add_basic_alias_analysis_pass();
        fpm.add_promote_memory_to_register_pass();
        fpm.add_instruction_combining_pass();
        fpm.add_reassociate_pass();

        // The generator pops the statements off the top.
        statements.reverse();

        IRGenerator {
            context,
            module,
            builder,
            fpm,
            
            variables: HashMap::with_capacity(10),
            current_fn: None,

            statements,
        }
    }
}
