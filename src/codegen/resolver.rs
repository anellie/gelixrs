use super::super::{
    ast::{
        declaration::{Declaration, FuncSignature},
        expression::{LOGICAL_BINARY, Expression},
        literal::Literal,
        statement::Statement,
    },
    lexer::token::Token,
};
use super::IRGenerator;
use inkwell::{
    builder::Builder,
    context::Context,
    module::Module,
    passes::PassManager,
    types::{BasicType, BasicTypeEnum, StructType},
    values::BasicValueEnum,
    AddressSpace,
};
use std::collections::HashMap;

/// A resolver. Resolves all variables and types.
pub struct Resolver {
    context: Context,
    builder: Builder,
    module: Module,

    types: HashMap<String, StructType>,
    environments: Vec<Environment>,

    // Used to make unique variable names when a name collision occurs.
    // Increments every time a new scope is created.
    environment_counter: usize,

    // Just for error reporting.
    current_func_name: String,
}

impl Resolver {
    /// Will do all passes after one another.
    pub fn resolve(&mut self, declarations: &mut Vec<Declaration>) -> Option<()> {
        for declaration in declarations.iter_mut() {
            Resolver::check_error(self.first_pass(declaration), &self.current_func_name)?;
        }

        for declaration in declarations.iter_mut() {
            Resolver::check_error(self.second_pass(declaration), &self.current_func_name)?;
        }

        for declaration in declarations.iter_mut() {
            Resolver::check_error(self.third_pass(declaration), &self.current_func_name)?;
        }

        for declaration in declarations.iter_mut() {
            Resolver::check_error(self.fourth_pass(declaration), &self.current_func_name)?;
        }

        Some(())
    }

    /// During the first pass, the types map is filled with all types.
    /// These types are opague, and filled later.
    fn first_pass(&mut self, declaration: &Declaration) -> Result<(), String> {
        match declaration {
            Declaration::Class { name, variables: _, methods: _, } => self.declare_type(name),
            Declaration::Enum { name, variants: _ } => self.declare_type(name),
            _ => Ok(()),
        }
    }

    fn declare_type(&mut self, name: &Token) -> Result<(), String> {
        let struc = self.context.opaque_struct_type(name.lexeme);
        let exists = self.types.insert(
            name.lexeme.to_string(),
            struc
        );

        if exists.is_none() {
            Ok(())
        } else {
            Err(format!(
                "The type/class/enum {} was declared more than once!",
                name.lexeme
            ))
        }
    }

    /// During the second pass, all functions are declared.
    fn second_pass(&mut self, declaration: &Declaration) -> Result<(), String> {
        match declaration {
            Declaration::ExternFunction(func) => self.create_function(func),
            Declaration::Function(func) => self.create_function(&func.sig),
            _ => Ok(()),
        }
    }

    fn create_function(&mut self, function: &FuncSignature) -> Result<(), String> {
        if self
            .module
            .get_function(&function.name.lexeme.to_string())
            .is_some()
        {
            return Err(format!(
                "The function {} was declared more than once!",
                function.name.lexeme
            ));
        } 

        let mut parameters = Vec::new();
        for param in &function.parameters {
            parameters.push(self.resolve_type(&param._type)?);
        }
        let parameters = parameters.as_slice();

        let mut call_type = self.get_type("None")?;
        let fn_type = if let Some(ret_type) = &function.return_type {
            call_type = self.resolve_type(&ret_type)?;
            call_type.fn_type(&parameters, false)
        } else {
            self.context.void_type().fn_type(&parameters, false)
        };

        self.module.add_function(function.name.lexeme, fn_type, None);
        self.environments
            .first_mut()
            .unwrap()
            .variables
            .insert(function.name.lexeme.to_string(), VarDef::new(false, call_type));
        Ok(())
    }

    /// During the third pass, all class structs are filled.
    /// TODO: Structs that have structs as a field won't init properly due to wrong order
    fn third_pass(&mut self, declaration: &mut Declaration) -> Result<(), String> {
        if let Declaration::Class { name, variables, methods: _ } = declaration {
            let mut fields = Vec::with_capacity(variables.len());
            for field in variables {
                fields.push(self.resolve_expression(&mut field.initializer)?);
            }

            let struc = self.types.get(name.lexeme).unwrap();
            struc.set_body(fields.as_slice(), false);
        }

        Ok(())
    }

    /// During the fourth pass, all variables inside functions are checked. TODO: also check class vars/methods
    /// This is to ensure the variable is defined and allowed in the current scope.
    fn fourth_pass(&mut self, declaration: &mut Declaration) -> Result<(), String> {
        match declaration {
            Declaration::Function(func) => {
                self.current_func_name = func.sig.name.lexeme.to_string();
               
                self.begin_scope();
                for param in func.sig.parameters.iter_mut() {
                    let param_type = self.resolve_type(&param._type)?;
                    self.define_variable(
                        &mut param.name, 
                        false, false, 
                        param_type
                    )?;
                }
                self.resolve_expression(&mut func.body)?;
                self.end_scope();
            }
            _ => (),
        };

        Ok(())
    }

    fn resolve_statement(&mut self, statement: &mut Statement) -> Result<(), String> {
        match statement {
            Statement::Variable(var) => {
                let _type = self.resolve_expression(&mut var.initializer)?;
                self.define_variable(&mut var.name, !var.is_val, true, _type)?;
            }

            Statement::Expression(expr) => { 
                self.resolve_expression(expr)?; 
            }

            _ => Err("Encountered unimplemented statement.")?,
        }

        Ok(())
    }

    fn resolve_expression(&mut self, expression: &mut Expression) -> Result<BasicTypeEnum, String> {
        match expression {
            Expression::Assignment { name, value } => {
                let var = self.find_var(name)?;
                if var.mutable {
                    let expr_type = self.resolve_expression(value)?;
                    if expr_type == var._type {
                        Ok(expr_type)
                    } else {
                        Err(format!("Variable {} is a different type.", name.lexeme))
                    }
                } else {
                    Err(format!("Variable {} is not assignable (val)", name.lexeme))
                }
            }

            Expression::Binary { left, operator, right } => {
                let left = self.resolve_expression(left)?;
                let right = self.resolve_expression(right)?;

                if left == right {
                    if LOGICAL_BINARY.contains(&operator.t_type) {
                        Ok(self.get_type("bool")?)
                    } else {
                        Ok(left)
                    }
                } else {
                    Err(format!("Binary operands have incompatible types! {:?} and {:?}", left, right))
                }
            }

            Expression::Block(statements) => {
                self.begin_scope();
                for statement in statements.iter_mut() {
                    self.resolve_statement(statement)?;
                }

                let mut ret_type = self.get_type("None")?;
                let last = statements.last_mut();
                if let Some(last) = last {
                    if let Statement::Expression(expr) = last {
                        ret_type = self.resolve_expression(expr)?;
                    }
                }

                self.end_scope();

                Ok(ret_type)
            }

            // TODO: Check type of function arguments
            Expression::Call { callee, token: _, arguments } => {
                for arg in arguments {
                    self.resolve_expression(arg)?;
                }

                self.resolve_expression(callee)
            }

            Expression::Grouping(expr) => self.resolve_expression(expr),

            Expression::If { condition, then_branch, else_branch } => {
                let condition_type = self.resolve_expression(condition)?;
                if condition_type != self.get_type("bool")? {
                    Err("If condition must be a boolean.")?
                }

                let then_type = self.resolve_expression(then_branch)?;
                if let Some(else_branch) = else_branch {
                    let else_type = self.resolve_expression(else_branch)?;
                    if then_type == else_type {
                        return Ok(then_type)
                    }
                }

                self.get_type("None")
            }

            // TODO: Check that return expr is correct type
            Expression::Return(expr) => {
                if let Some(expr) = expr {
                    self.resolve_expression(expr)?;
                }
                self.get_type("None")
            }

            Expression::Literal(literal) => Ok(self.type_from_literal(literal)),

            Expression::Variable(name) => Ok(self.find_var(name)?._type),

            _ => Err("Encountered unimplemented expression.")?,
        }
    }

    fn resolve_type(&mut self, token: &Token) -> Result<BasicTypeEnum, String> {
        self.get_type(token.lexeme)
    }

    fn get_type(&mut self, name: &str) -> Result<BasicTypeEnum, String> {
        Ok(match name {
            "bool" => self.context.bool_type().as_basic_type_enum(),
            "f32" => self.context.f32_type().as_basic_type_enum(),
            "f64" => self.context.f64_type().as_basic_type_enum(),
            "i32" => self.context.i32_type().as_basic_type_enum(),
            "i64" => self.context.i64_type().as_basic_type_enum(),
            "String" => self
                .context
                .i8_type()
                .ptr_type(AddressSpace::Generic)
                .as_basic_type_enum(),
            _ => self
                .types
                .get(name)
                .ok_or(format!("Unknown type {}", name))?
                .as_basic_type_enum(),
        })
    }

    fn type_from_literal(&mut self, literal: &Literal) -> BasicTypeEnum {
        match literal {
            Literal::Bool(_) => self.context.bool_type().as_basic_type_enum(),
            Literal::Float(_) => self.context.f32_type().as_basic_type_enum(),
            Literal::Double(_) => self.context.f64_type().as_basic_type_enum(),
            Literal::Int(_) => self.context.i64_type().as_basic_type_enum(),
            Literal::String(_) => self
                .context
                .i8_type()
                .ptr_type(AddressSpace::Generic)
                .as_basic_type_enum(),
            _ => panic!("Unimplemented literal!")
        }
    }

    fn define_variable(
        &mut self,
        token: &mut Token,
        mutable: bool,
        allow_redefine: bool,
        _type: BasicTypeEnum
    ) -> Result<(), String> {
        let name = token.lexeme.to_string();

        if self.find_var(token).is_ok() {
            let new_name = format!("{}-{}", name, self.environment_counter);
            self.environments
                .last_mut()
                .unwrap()
                .moved_vars
                .insert(name.clone(), new_name.clone());
            token.relocated = Some(new_name);
        }

        let was_defined = self
            .environments
            .last_mut()
            .unwrap()
            .variables
            .insert(name, VarDef::new(mutable, _type))
            .is_some();
        
        if was_defined && !allow_redefine {
            Err(format!(
                "Variable {} cannot be redefined in the same scope.",
                token.lexeme
            ))
        } else {
            Ok(())
        }
    }

    fn find_var(&mut self, token: &mut Token) -> Result<VarDef, String> {
        let name = token.lexeme.to_string();

        for env in self.environments.iter().rev() {
            if env.variables.contains_key(&name) {
                if env.moved_vars.contains_key(&name) {
                    token.relocated = Some(env.moved_vars.get(&name).unwrap().to_string());
                }
                // TODO: Cloning is not ideal, but I think BasicTypeEnum is just a pointer anyways
                return Ok(env.variables.get(&name).unwrap().clone());
            }
        }

        Err(format!("Variable {} is not defined.", name))
    }
    
    fn begin_scope(&mut self) {
        self.environments.push(Environment::new());
        self.environment_counter += 1;
    }

    fn end_scope(&mut self) {
        self.environments.pop();
    }

    fn check_error(result: Result<(), String>, func: &String) -> Option<()> {
        result.or_else(|err| {
            eprintln!("[Resolver] {} (occured in function: {})", err, func);
            Err(())
        }).ok()
    }

    pub fn new() -> Resolver {
        let context = Context::create();
        let module = context.create_module("main");
        let builder = context.create_builder();
        
        let mut environments = Vec::with_capacity(10);
        environments.push(Environment::new()); // global environment

        let mut types = HashMap::with_capacity(10);
        types.insert(
            "None".to_string(),
            context.struct_type(&[BasicTypeEnum::IntType(context.bool_type())], true),
        );

        Resolver {
            context,
            module,
            builder,
            types,
            environments,
            environment_counter: 0,
            current_func_name: "".to_string(),
        }
    }

    /// Turns the resolver into a generator for IR. Call resolve() first.
    pub fn into_generator(self, mut declarations: Vec<Declaration>) -> IRGenerator {
        let mpm = PassManager::create(());
        mpm.add_instruction_combining_pass();
        mpm.add_reassociate_pass();
        // mpm.add_gvn_pass(); This pass causes unpredictable SIGSEGV... WTH?
        mpm.add_cfg_simplification_pass();
        mpm.add_basic_alias_analysis_pass();
        mpm.add_promote_memory_to_register_pass();
        mpm.add_instruction_combining_pass();
        mpm.add_reassociate_pass();

        // The generator pops the declarations off the top.
        declarations.reverse();

        let none_const =
            self.types
                .get("None")
                .unwrap()
                .const_named_struct(&[BasicValueEnum::IntValue(
                    self.context.bool_type().const_int(0, false),
                )]);
        let none_const = BasicValueEnum::StructValue(none_const);

        IRGenerator {
            context: self.context,
            module: self.module,
            builder: self.builder,
            mpm,
            
            variables: HashMap::with_capacity(10),
            current_fn: None,

            declarations,

            types: self.types,
            none_const,
        }
    }
}

/// An environment holds all variables in the current scope.
/// To keep track of all variables, a stack of them is used.
struct Environment {
    /// Key = name; all variables in this environment/scope
    variables: HashMap<String, VarDef>,

    /// Variables that were moved due to a naming collision
    moved_vars: HashMap<String, String>,
}

impl Environment {
    pub fn new() -> Environment {
        Environment {
            variables: HashMap::with_capacity(5),
            moved_vars: HashMap::with_capacity(5),
        }
    }
}

#[derive(Clone)]
struct VarDef {
    pub mutable: bool,
    pub _type: BasicTypeEnum 
}

impl VarDef {
    pub fn new(mutable: bool, _type: BasicTypeEnum) -> VarDef {
        VarDef {
            mutable,
            _type
        }
    }
}