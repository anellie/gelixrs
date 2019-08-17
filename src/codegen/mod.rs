pub mod resolver;

use super::{
    ast::{
        declaration::{Class, DeclarationList, Function, Variable},
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
use std::{collections::HashMap, convert::TryInto};

/// A generator that creates LLVM IR.
/// Created through a [Resolver].
/// 
/// Will panic when encountering invalid code; this should not happen however thanks to the 
/// resolver validating it.
pub struct IRGenerator {
    /// LLVM-related. Refer to their docs for more info.
    context: Context,
    builder: Builder,
    module: Module,
    mpm: PassManager<Module>,

    /// All variables and the currently compiled function.
    /// Note that not all variables are valid - they are kept after going out of scope.
    /// This is not an issue since the resolver checked against this already.
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
    /// Generates IR. Will process all declarations given.
    pub fn generate(mut self) -> Module {
        while !self.decl_list.classes.is_empty() {
            let class = self.decl_list.classes.pop().unwrap();
            self.class(class);
        }

        while !self.decl_list.functions.is_empty() {
            let func = self.decl_list.functions.pop().unwrap();
            self.function(func);
        }

        self.mpm.run_on(&self.module);
        self.module
    }

    fn class(&mut self, class: Class) {
        for method in class.methods {
            self.function(method);
        }

        self.class_initializer(&class.name.lexeme, class.variables);
    }

    /// Creates a function that takes a zeroinitializer struct as a parameter
    /// and populates its fields with defaults.
    ///
    /// This functions cannot create & return a struct since alloca are destroyed on return,
    /// which would result in the function returning garbage stack data.
    fn class_initializer(&mut self, name: &str, variables: Vec<Variable>) {
        let class_def = self.types.remove(name).unwrap();
        let init_fn = class_def.initializer.unwrap();
        self.current_fn = Some(init_fn);

        let entry = self.context.append_basic_block(&init_fn, "entry");
        self.builder.position_at_end(&entry);

        let struc = init_fn.get_first_param().unwrap();
        if let BasicValueEnum::PointerValue(ptr) = struc {
            self.variables.insert("this".to_string(), ptr);
            for (index, variable) in variables.into_iter().enumerate() {
                let initializer = self.expression(variable.initializer);
                unsafe {
                    let gep_ptr = self.builder.build_struct_gep(ptr, index as u32, "classgep");
                    self.builder.build_store(gep_ptr, initializer);
                }
            }
        } else {
            panic!("Creating class initializer");
        }

        self.types.insert(name.to_string(), class_def);
        self.builder.build_return(None);
    }

    /// Compiles a function to IR. (The function was already declared by the resolver.)
    fn function(&mut self, func: Function) {
        let function = self
            .module
            .get_function(&func.sig.name.lexeme)
            .expect("Undefined function");
        self.current_fn = Some(function);

        let entry = self.context.append_basic_block(&function, "entry");
        self.builder.position_at_end(&entry);

        self.variables.reserve(func.sig.parameters.len());
        for (arg_val, arg_name) in function.get_param_iter().zip(func.sig.parameters.iter()) {
            let arg_name = &arg_name.name.lexeme;
            let alloca = self.create_entry_block_alloca(arg_val.get_type(), arg_name);
            self.builder.build_store(alloca, arg_val);
            self.variables.insert(arg_name.to_string(), alloca);
        }

        let body = self.expression(func.body);
        if func.sig.return_type.is_some() {
            self.builder.build_return(Some(&body));
        } else {
            self.builder.build_return(None);
        }

        if !function.verify(true) {
            panic!("Invalid generated function")
        }
    }

    /// Compile a statement. Statements are only found in blocks.
    /// TODO: Consider turning Variable into an expression too and eliminating statements
    /// altogether. (Variable definition could still be treated differently by the parser to
    /// prevent a function that just defines a variable or similar.)
    fn statement(&mut self, statement: Statement) {
        match statement {
            Statement::Variable(var) => self.var_statement(var),
            Statement::Expression(expr) => {
                self.expression(expr);
            }
        }
    }

    fn var_statement(&mut self, var: Variable) {
        let initial_value = self.expression(var.initializer);
        let alloca = self.create_entry_block_alloca(initial_value.get_type(), &var.name.lexeme);

        self.builder.build_store(alloca, initial_value);
        self.variables.insert(var.name.lexeme, alloca);
    }

    fn expression(&mut self, expression: Expression) -> BasicValueEnum {
        match expression {
            Expression::Assignment { name, value } => self.assignment(name, *value),
            Expression::Binary { left, operator, right } =>
                self.binary(*left, operator, *right),
            Expression::Block(expressions) => self.block_expr(expressions),
            Expression::Call { callee, arguments } => self.call_expr(*callee, arguments),
            Expression::For { condition, body } => self.for_expression(*condition, *body),
            Expression::Get { object, name } => self.get_expression(*object, name),
            Expression::Grouping(expr) => self.expression(*expr),
            Expression::If { condition, then_branch, else_branch } =>
                self.if_expression(*condition, *then_branch, else_branch),
            Expression::Literal(literal) => self.literal(literal),
            Expression::Return(expr) => self.return_expression(expr),
            Expression::Variable(name) => self.variable(name),
            _ => panic!("Encountered unimplemented expression"),
        }
    }

    fn assignment(&mut self, token: Token, value: Expression) -> BasicValueEnum {
        let name = token.lexeme;
        let value = self.expression(value);
        let var = self.variables.get(&name).expect("Undefined var");

        self.builder.build_store(*var, value);
        value
    }

    fn block_expr(&mut self, statements: Vec<Statement>) -> BasicValueEnum {
        statements.into_iter().fold(self.none_const, |_, stmt| {
            if let Statement::Expression(expr) = stmt {
                self.expression(expr)
            } else {
                self.statement(stmt);
                self.none_const
            }
        })
    }

    // TODO: Add float support
    // TODO: Make this less ugly
    fn binary(
        &mut self,
        left: Expression,
        operator: Token,
        right: Expression,
    ) -> BasicValueEnum {
        let left = self.expression(left);
        let right = self.expression(right);

        let left = if let BasicValueEnum::IntValue(int) = left { int } else { panic!("Only int are supported for math operations") };
        let right = if let BasicValueEnum::IntValue(int) = right { int } else { panic!("Only int are supported for math operations") };

        BasicValueEnum::IntValue(match operator.t_type {
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

            _ => panic!("Unsupported binary operand"),
        })
    }

    fn call_expr(
        &mut self,
        callee: Expression,
        arguments: Vec<Expression>,
    ) -> BasicValueEnum {
        match callee {
            Expression::Variable(token) => {
                let function = self.module.get_function(&token.lexeme);
                if let Some(function) = function {
                    return self.func_call(function, arguments, None);
                }

                let class = self.types.get(&token.lexeme);
                if let Some(class_def) = class {
                    let initializer = class_def.initializer.unwrap();
                    let struc = class_def._type.const_zero();
                    let alloca = self.create_entry_block_alloca(struc.get_type(), "classinit");

                    self.builder.build_store(alloca, struc);
                    self.func_call(initializer, arguments, Some(alloca.into()));
                    return self.builder.build_load(alloca, "classinitload");
                }

                panic!("Could not find matching func or class to call")
            }

            Expression::Get { object, name } => {
                let obj = self.expression(*object);
                if let BasicValueEnum::StructValue(struc) = obj {
                    let struc_type = struc.get_type();
                    let class_def = self
                        .types
                        .iter()
                        .find(|&_type| _type.1._type == struc_type)
                        .unwrap()
                        .1;

                    let function = class_def.methods[&name.lexeme].clone();
                    self.func_call(function, arguments, Some(struc.into()))
                } else {
                    panic!("call_expr: Get expression without struct")
                }
            }

            _ => panic!("Unsupported callee"),
        }
    }

    fn func_call(
        &mut self,
        function: FunctionValue,
        arguments: Vec<Expression>,
        first_arg: Option<BasicValueEnum>,
    ) -> BasicValueEnum {
        let mut args = Vec::with_capacity(arguments.len() + (first_arg.is_some() as usize));
        if let Some(arg) = first_arg {
            args.push(arg);
        }
        for arg in arguments {
            args.push(self.expression(arg));
        }

        let ret_type = self
            .builder
            .build_call(function, args.as_slice(), "tmp")
            .try_as_basic_value();
        ret_type.left().unwrap_or(self.none_const)
    }

    fn get_expression(
        &mut self,
        object: Expression,
        name: Token,
    ) -> BasicValueEnum {
        let ptr = self.pointer_from_get(object, name);
        self.builder.build_load(ptr, "classload")
    }

    fn pointer_from_get(
        &mut self,
        object: Expression,
        name: Token,
    ) -> PointerValue {
        match object {
            Expression::Variable(obj_name) => {
                let struc = self.variables[&obj_name.lexeme];
                self.get_from_struct(&struc, name)
            }

            Expression::Get { object, name: inner_name } => {
                let object = self.pointer_from_get(*object, inner_name);
                self.get_from_struct(&object, name)
            }

            _ => panic!("Invalid get expression"),
        }
    }

    fn get_from_struct(&self, struc: &PointerValue, name: Token) -> PointerValue {
        let struc_def = self.find_class(*struc);
        let ptr_index = struc_def.var_map[&name.lexeme];

        unsafe {
            self.builder.build_struct_gep(*struc, ptr_index, "classgep")
        }
    }

    fn for_expression(&mut self, condition: Expression, body: Expression) -> BasicValueEnum {
        let cond_block = self.context.append_basic_block(&self.cur_fn(), "forcond");
        let loop_block = self.context.append_basic_block(&self.cur_fn(), "forloop");
        let cont_block = self.context.append_basic_block(&self.cur_fn(), "forcont");

        self.builder.build_unconditional_branch(&cond_block);
        self.builder.position_at_end(&cond_block);
        let condition = self.expression(condition);
        let condition = if let BasicValueEnum::IntValue(value) = condition {
             self.builder.build_int_compare(
                IntPredicate::NE,
                value,
                self.context.bool_type().const_int(0, false),
                "forcond",
             )
        } else { panic!("For condition") };
        self.builder.build_conditional_branch(condition, &loop_block, &cont_block);

        self.builder.position_at_end(&loop_block);
        let body = self.expression(body);
        let body_alloca = self.create_entry_block_alloca(body.get_type(), "forbody");
        self.builder.build_store(body_alloca, body);
        self.builder.build_unconditional_branch(&cond_block);

        self.builder.position_at_end(&cont_block);
        self.builder.build_load(body_alloca, "forbody")
    }

    fn if_expression(
        &mut self,
        condition: Expression,
        then_b: Expression,
        else_b: Option<Box<Expression>>,
    ) -> BasicValueEnum {
        let parent = self.cur_fn();
        let condition = self.expression(condition);

        let condition = if let BasicValueEnum::IntValue(value) = condition {
            self.builder.build_int_compare(
                IntPredicate::NE,
                value,
                self.context.bool_type().const_int(0, false),
                "ifcond",
            )
        } else { panic!("If condition wasn't a boolean") };

        let then_bb = self.context.append_basic_block(&parent, "then");
        let else_bb = self.context.append_basic_block(&parent, "else");
        let cont_bb = self.context.append_basic_block(&parent, "ifcont");

        if else_b.is_none() {
            self.builder.build_conditional_branch(condition, &then_bb, &cont_bb);
        } else {
            self.builder.build_conditional_branch(condition, &then_bb, &else_bb);
        }

        self.builder.position_at_end(&then_bb);
        let then_val = self.expression(then_b);
        self.builder.build_unconditional_branch(&cont_bb);

        self.builder.position_at_end(&else_bb);
        let ret_val = if let Some(else_b) = else_b {
            let else_val = self.expression(*else_b);

            self.builder.position_at_end(&cont_bb);
            let phi = self.builder.build_phi(then_val.get_type(), "ifphi");
            phi.add_incoming(&[
                (&then_val, &then_bb),
                (&else_val, &else_bb)
            ]);
            phi.as_basic_value()
        } else {
            self.literal(Literal::None)
        };

        self.builder.position_at_end(&else_bb);
        self.builder.build_unconditional_branch(&cont_bb);
        self.builder.position_at_end(&cont_bb);

        ret_val
    }

    fn return_expression(
        &mut self,
        expression: Option<Box<Expression>>,
    ) -> BasicValueEnum {
        if let Some(expression) = expression {
            let expression = self.expression(*expression);
            self.builder.build_return(Some(&expression));
        } else {
            self.builder.build_return(None);
        }

        // Ensure no code is written to the block after the ret instruction.
        self.builder.clear_insertion_position();

        // Even though it is an expression, code after it gets discarded anyway
        self.none_const
    }

    // TODO: Array literals
    fn literal(&mut self, literal: Literal) -> BasicValueEnum {
        match literal {
            Literal::None => self.none_const,
            Literal::Bool(value) => self
                .context
                .bool_type()
                .const_int(value as u64, false)
                .into(),
            Literal::Int(num) => self
                .context
                .i64_type()
                .const_int(num.try_into().unwrap(), false)
                .into(),
            Literal::Float(num) => self.context.f32_type().const_float(num.into()).into(),
            Literal::Double(num) => self.context.f64_type().const_float(num).into(),
            Literal::String(string) => {
                let const_str = self.builder.build_global_string_ptr(&string, "literal-str");
                BasicValueEnum::PointerValue(const_str.as_pointer_value())
            }
            _ => panic!("What is that?"),
        }
    }

    fn variable(&mut self, name: Token) -> BasicValueEnum {
        let var = self.variables.get(&name.lexeme).expect("Couldn't find variable");
        self.builder.build_load(*var, &name.lexeme)
    }

    fn find_class(&self, struc: PointerValue) -> &ClassDef {
        let struc_ptr_type = struc.get_type().get_element_type();
        if let AnyTypeEnum::StructType(struc_type) = struc_ptr_type {
            self.types
                .iter()
                .find(|&_type| _type.1._type == struc_type)
                .unwrap()
                .1
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
    pub _type: StructType,
    pub var_map: HashMap<String, u32>,
    pub methods: HashMap<String, FunctionValue>,
    // Note that the initializer is only None during creation in the Resolver.
    // By the time the IRGen gets to it, it is always Some.
    pub initializer: Option<FunctionValue>,
}

impl ClassDef {
    pub fn new(_type: StructType) -> ClassDef {
        ClassDef {
            _type,
            var_map: HashMap::new(),
            methods: HashMap::new(),
            initializer: None,
        }
    }
}
