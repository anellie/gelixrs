pub mod resolver;

use super::{
    ast::{
        declaration::{DeclarationList, Class, Function, Variable},
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
    types::{AnyTypeEnum, BasicType, StructType},
    values::{BasicValueEnum, FunctionValue, PointerValue},
    IntPredicate,
};
use std::{
    collections::HashMap,
    convert::TryInto
};

/// A generator that creates LLVM IR.
/// Created through a [Resolver].
pub struct IRGenerator {
    /// LLVM-related. Refer to their docs for more info.
    context: Context,
    builder: Builder,
    module: Module,
    mpm: PassManager<Module>,

    // All variables in the current scope and the currently compiled function.
    variables: HashMap<String, PointerValue>,
    current_fn: Option<FunctionValue>,

    // All declarations remaining to be compiled.
    decl_list: DeclarationList,

    // All types (classes) that were produced by the [Resolver].
    types: HashMap<String, ClassDef>,
    // A constant that is used for expressions that don't produce a value but are required to.
    none_const: BasicValueEnum,
}

impl IRGenerator {
    /// Generates IR. Will process all statements given.
    pub fn generate(mut self) -> Option<Module> {
        while !self.decl_list.classes.is_empty() {
            let class = self.decl_list.classes.pop().unwrap();
            self.class(class).ok()?;
        }

        while !self.decl_list.functions.is_empty() {
            let func = self.decl_list.functions.pop().unwrap();
            self.function(func).ok()?;
        }

        self.mpm.run_on(&self.module);
        Some(self.module)
    }

    // TODO: Define methods
    fn class(&mut self, class: Class) -> Result<(), String> {
        let mut default_values = Vec::with_capacity(class.variables.len());
        for var in class.variables {
            default_values.push(self.expression(var.initializer)?);
        }

        let mut class_def = self.types.get_mut(&class.name.lexeme).unwrap();
        class_def.default_values = Some(default_values);

        Ok(())
    }

    /// Compiles a function to IR. (The function was already declared by the resolver.)
    fn function(&mut self, func: Function) -> Result<(), String> {
        let function = self
            .module
            .get_function(&func.sig.name.lexeme)
            .ok_or("Internal error: Undefined function.")?;
        self.current_fn = Some(function);

        // See second_pass() in the resolver for why the function already has a block.
        let entry = function.get_first_basic_block().unwrap();
        self.builder.position_at_end(&entry);

        self.variables.reserve(func.sig.parameters.len());
        for (i, arg) in function.get_param_iter().enumerate() {
            let arg_name = &func.sig.parameters[i].name.lexeme;
            let alloca = self.create_entry_block_alloca(arg.get_type(), arg_name);
            self.builder.build_store(alloca, arg);
            self.variables.insert(func.sig.parameters[i].name.lexeme.to_string(), alloca);
        }

        let body = self.expression(func.body)?;
        if func.sig.return_type.is_some() {
            self.builder.build_return(Some(&body));
        } else {
            self.builder.build_return(None);
        }

        if function.verify(true) {
            Ok(())
        } else {
            unsafe { function.delete(); }
            Err(format!(
                "Invalid generated function '{}'. See LLVM error output for details.",
                func.sig.name.lexeme
            ))
        }
    }

    /// Compile a statement. Statements are only found in blocks.
    fn statement(&mut self, statement: Statement) -> Result<(), String> {
        match statement {
            Statement::Variable(var) => self.var_statement(var),
            Statement::Expression(expr) => {
                self.expression(expr)?;
                Ok(())
        }
            _ => Err(format!(
                "Encountered unimplemented statement '{:?}'.",
                statement
            )),
    }
    }

    fn var_statement(&mut self, var: Variable) -> Result<(), String> {
        let initial_value = self.expression(var.initializer)?;
        let alloca = self.create_entry_block_alloca(initial_value.get_type(), &var.name.lexeme);

        self.builder.build_store(alloca, initial_value);
        self.variables.insert(var.name.lexeme, alloca);

        Ok(())
    }

    fn expression(&mut self, expression: Expression) -> Result<BasicValueEnum, String> {
        match expression {
            Expression::Assignment { name, value } => self.assignment(name, *value),
            Expression::Binary { left, operator, right } => self.binary(*left, operator, *right),
            Expression::Block(expressions) => self.block_expr(expressions),
            Expression::Call { callee, token: _, arguments } => self.call_expr(*callee, arguments),
            Expression::Get { object, name } => self.get_expression(*object, name),
            Expression::Grouping(expr) => self.expression(*expr),
            Expression::If { condition, then_branch, else_branch } => self.if_expression(*condition, *then_branch, else_branch),
            Expression::Literal(literal) => Ok(self.literal(literal)),
            Expression::Return(expr) => self.return_expression(expr),
            Expression::Variable(name) => self.variable(name),
            _ => Err("Encountered unimplemented expression.".to_string()),
        }
    }

    fn assignment(&mut self, token: Token, value: Expression) -> Result<BasicValueEnum, String> {
        let name = token.lexeme;

        let value = self.expression(value)?;
        let var = self
            .variables
            .get(&name)
            .ok_or(format!("Undefined variable '{}'.", name))?;

        self.builder.build_store(*var, value);
        Ok(value)
    }

    fn block_expr(&mut self, mut statements: Vec<Statement>) -> Result<BasicValueEnum, String> {
        if statements.is_empty() {
            return Ok(self.literal(Literal::None))
        }

        statements.reverse();
        loop {
            let statement = statements.pop().unwrap();

            if statements.is_empty() {
                break if let Statement::Expression(expr) = statement {
                    self.expression(expr)
                } else {
                    self.statement(statement)?;
                    Ok(self.literal(Literal::None))
                };
            }

            self.statement(statement)?;
        }
    }

    // TODO: Add float support
    // TODO: Make this less ugly
    fn binary(
        &mut self,
        left: Expression,
        operator: Token,
        right: Expression,
    ) -> Result<BasicValueEnum, String> {
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

            _ => Err("Unsupported binary operand.")?,
        }))
    }

    fn call_expr(
        &mut self,
        callee: Expression,
        arguments: Vec<Expression>,
    ) -> Result<BasicValueEnum, String> {
        if let Expression::Variable(token) = callee {
            let function = self.module.get_function(&token.lexeme);
            if let Some(function) = function {
                return self.func_call(function, arguments)
            }

            let class = self.types.get(&token.lexeme);
            if let Some(class_def) = class {
                return self.class_call(class_def, arguments)
            }

            Err(format!("Could not find matching func or class '{}' to call.", token.lexeme))
        } else {
            Err("Unsupported callee.".to_string())?
        }
    }

    fn func_call(
        &mut self,
        function: FunctionValue,
        arguments: Vec<Expression>,
    ) -> Result<BasicValueEnum, String> {
        let mut args = Vec::with_capacity(arguments.len());
        for arg in arguments {
            args.push(self.expression(arg)?);
        }

        let ret_type = self.builder
            .build_call(function, args.as_slice(), "tmp")
            .try_as_basic_value();
        if ret_type.is_left() {
            Ok(ret_type.left().unwrap())
        } else {
            Ok(self.none_const)
        }
    }

    // TODO: call init()
    fn class_call(
        &self,
        class: &ClassDef,
        _arguments: Vec<Expression>,
    ) -> Result<BasicValueEnum, String> {
        if let Some(defaults) = &class.default_values {
            Ok(BasicValueEnum::StructValue(class._type.const_named_struct(defaults.as_slice())))
        } else {
            //self.class(class.ast_node); TODO:
            Ok(BasicValueEnum::StructValue(class._type.const_named_struct(class.default_values.as_ref().unwrap().as_slice())))
        }
    }

    fn get_expression(&mut self, object: Expression, name: Token) -> Result<BasicValueEnum, String> {
        let ptr = self.pointer_from_get(object, name)?;
        Ok(self.builder.build_load(ptr, "classload"))
    }

    fn pointer_from_get(&mut self, object: Expression, name: Token) -> Result<PointerValue, String> {
        match object {
            Expression::Variable(obj_name) => {
                let struc = self.variables.get(&obj_name.lexeme).unwrap();
                self.get_from_struct(struc, name)
            }

            Expression::Get { object, name: inner_name} => {
                let object = self.pointer_from_get(*object, inner_name)?;
                self.get_from_struct(&object, name)
            }

            _ => Err("Invalid get expression.".to_string())
        }
    }

    fn get_from_struct(&self, struc: &PointerValue, name: Token) -> Result<PointerValue, String> {
        let struc_def = self.find_class(*struc);
        let ptr_index = struc_def.var_map.get(&name.lexeme).unwrap();

        unsafe {
            Ok(self.builder.build_struct_gep(*struc, *ptr_index, "classgep"))
        }
    }

    fn if_expression(
        &mut self,
        condition: Expression,
        then_b: Expression,
        else_b: Option<Box<Expression>>,
    ) -> Result<BasicValueEnum, String> {
        let parent = self.cur_fn();
        let condition = self.expression(condition)?;

        if let BasicValueEnum::IntValue(value) = condition {
            let condition = self.builder.build_int_compare(
                IntPredicate::NE,
                value,
                self.context.bool_type().const_int(0, false),
                "ifcond",
            );

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

            self.builder.position_at_end(&else_bb);
            let ret_val = if let Some(else_b) = else_b {
                let else_val = self.expression(*else_b)?;

                self.builder.position_at_end(&cont_bb);
                let phi = self.builder.build_phi(then_val.get_type(), "ifphi");
                phi.add_incoming(&[
                    (&then_val, &then_bb),
                    (&else_val, &else_bb)
                ]);
                Ok(phi.as_basic_value())
            } else {
                Ok(self.literal(Literal::None))
            };

            self.builder.position_at_end(&else_bb);
            self.builder.build_unconditional_branch(&cont_bb);
            self.builder.position_at_end(&cont_bb);

            ret_val
        } else {
            Err("If condition needs to be a boolean.".to_string())
        }
    }

    fn return_expression(
        &mut self,
        expression: Option<Box<Expression>>,
    ) -> Result<BasicValueEnum, String> {
        if let Some(expression) = expression {
            let expression = self.expression(*expression)?;
            self.builder.build_return(Some(&expression));
        } else {
            self.builder.build_return(None);
        }

        // Ensure no code is written to the block after the ret instruction.
        self.builder.clear_insertion_position();

        // Even though it is an expression, code after it gets discarded anyway
        Ok(self.none_const)
    }

    // TODO: Array literals
    fn literal(&mut self, literal: Literal) -> BasicValueEnum {
        match literal {
            Literal::None => self.none_const,
            Literal::Bool(value) => BasicValueEnum::IntValue(self.context.bool_type().const_int(value as u64, false)),
            Literal::Int(num) => BasicValueEnum::IntValue(self.context.i64_type().const_int(num.try_into().unwrap(), false)),
            Literal::Float(num) => BasicValueEnum::FloatValue(self.context.f32_type().const_float(num.into())),
            Literal::Double(num) => BasicValueEnum::FloatValue(self.context.f64_type().const_float(num)),
            Literal::String(string) => {
                let const_str = self.builder.build_global_string_ptr(&string, "literal-str");
                BasicValueEnum::PointerValue(const_str.as_pointer_value())
            }
            _ => panic!("What is that?"),
        }
    }

    fn variable(&mut self, name: Token) -> Result<BasicValueEnum, String> {
        match self.variables.get(&name.lexeme) {
            Some(var) => Ok(self.builder.build_load(*var, &name.lexeme)),
            None => Err(format!("Could not find variable '{}'.", name.lexeme)),
        }
    }

    fn find_class(&self, struc: PointerValue) -> &ClassDef {
        let struc_ptr_type = struc.get_type().get_element_type();
        if let AnyTypeEnum::StructType(struc_type) = struc_ptr_type {    
            self.types.iter().find(|&_type| _type.1._type == struc_type).unwrap().1
        } else {
            panic!("Invalid class instance pointer!!")
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

struct ClassDef {
    pub default_values: Option<Vec<BasicValueEnum>>,
    pub _type: StructType,
    pub var_map: HashMap<String, u32>,
}

impl ClassDef {
    pub fn new(_type: StructType) -> ClassDef {
        ClassDef {
            default_values: None,
            _type,
            var_map: HashMap::new()
        }
    }
}