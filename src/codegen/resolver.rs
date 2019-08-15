use super::super::{
    ast::{
        declaration::{Declaration, FuncSignature},
        expression::{LOGICAL_BINARY, Expression},
        literal::Literal,
        statement::Statement,
    },
    lexer::token::Token,
};
use super::{ClassDef, IRGenerator};
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
use std::mem;

/// A resolver. Resolves all variables and types.
pub struct Resolver {
    context: Context,
    builder: Builder,
    module: Module,

    /// All classes.
    types: HashMap<String, ClassDef>,
    // All environments/scopes currently.
    environments: Vec<Environment>,

    /// Used to make unique variable names when a name collision occurs.
    /// Increments every time a new scope is created.
    environment_counter: usize,

    /// Just for error reporting.
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
    /// These types are opaque, and filled later.
    fn first_pass(&mut self, declaration: &Declaration) -> Result<(), String> {
        match declaration {
            Declaration::Class(class) => self.declare_type(&class.name),
            Declaration::Enum { name, variants: _ } => self.declare_type(name),
            _ => Ok(()),
        }
    }

    fn declare_type(&mut self, name: &Token) -> Result<(), String> {
        let struc = self.context.opaque_struct_type(&name.lexeme);
        let exists = self.types.insert(
            name.lexeme.to_string(),
            ClassDef::new(struc)
        );

        if exists.is_none() {
            self.environments
                .first_mut()
                .unwrap()
                .variables
                .insert(name.lexeme.to_string(), VarDef::new(false, BasicTypeEnum::StructType(struc)));
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
            Declaration::Function(func) => {
                self.create_function(&func.sig)?;
                let function = self
                    .module
                    .get_function(&func.sig.name.lexeme)
                    .ok_or("Internal error: Undefined function.")?;

                // Work around a bug where the builder will cause a segfault when
                // creating a global string while not having a position set.
                // This can be an issue when creating default class field values.
                // https://github.com/TheDan64/inkwell/issues/32
                let entry = self.context.append_basic_block(&function, "entry");
                self.builder.position_at_end(&entry);

                Ok(())
            }

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

        self.module.add_function(&function.name.lexeme, fn_type, None);
        // TODO: Using the function as a variable gets incorrectly resolved to the return type
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
        if let Declaration::Class(class) = declaration {
            let mut fields = Vec::with_capacity(class.variables.len());
            let mut fields_map = HashMap::new();

            for (i, field) in class.variables.iter_mut().enumerate() {
                fields.push(self.resolve_expression(&mut field.initializer)?);
                fields_map.insert(field.name.lexeme.to_string(), i as u32);
            }

            let mut class_def = self.types.get_mut(&class.name.lexeme).unwrap();
            class_def._type.set_body(fields.as_slice(), false);
            class_def.var_map = fields_map;
        }

        Ok(())
    }

    /// During the fourth pass, all variables inside functions are checked.
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
                        false, 
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
                self.define_variable(&mut var.name, !var.is_val, _type)?;
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

            Expression::Get { object, name } => {
                let object = self.resolve_expression(object)?;

                if let BasicTypeEnum::StructType(struc) = object {
                    let class_def = self.find_class(struc);
                    let index = class_def.var_map.get(&name.lexeme).ok_or(format!("Unknown class field '{}'.", name.lexeme))?;
                    class_def._type.get_field_type_at_index(*index).ok_or("Internal error trying to get class field.".to_string())
                } else {
                    Err("Get syntax (x.y) is only supported on class instances.".to_string())
                }
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
        self.get_type(&token.lexeme)
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
                ._type
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
        _type: BasicTypeEnum
    ) -> Result<(), String> {
        let def = VarDef::new(mutable, _type);

        if self.find_var(token).is_ok() {
            let cur_env = self.environments.last_mut().unwrap();
            let new_name = format!("{}-{}", token.lexeme, self.environment_counter);
            let old_name = mem::replace(&mut token.lexeme, new_name);
            cur_env
                .moved_vars
                .insert(old_name.clone(), token.lexeme.clone());
            cur_env
                .variables
                .insert(old_name, def.clone());
        }

        let cur_env = self.environments.last_mut().unwrap();
        cur_env
            .variables
            .insert(token.lexeme.to_string(), def);

        Ok(())
    }

    fn find_var(&mut self, token: &mut Token) -> Result<VarDef, String> {
        for env in self.environments.iter().rev() {
            if let Some(var) = env.variables.get(&token.lexeme) {
                if env.moved_vars.contains_key(&token.lexeme) {
                    token.lexeme = env.moved_vars.get(&token.lexeme).unwrap().to_string();
                }
                return Ok(var.clone());
            }
        }

        Err(format!("Variable {} is not defined.", token.lexeme))
    }

    fn find_class(&mut self, struc: StructType) -> &ClassDef {
        self.types.iter().find(|&_type| _type.1._type == struc).unwrap().1
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
            eprintln!("[Resolver] {} (occurred in function: {})", err, func);
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
            ClassDef::new(context.struct_type(&[BasicTypeEnum::IntType(context.bool_type())], true)),
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
                ._type
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
#[derive(Debug)]
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

#[derive(Clone, Debug)]
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
