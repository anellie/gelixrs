use super::IRGenerator;
use super::super::{
    ast::{
        declaration::{Declaration, FuncSignature},
        statement::Statement,
        expression::Expression
    },
    lexer::token::Token,
};
use inkwell::{
    AddressSpace,
    builder::Builder,
    context::Context,
    module::Module,
    passes::PassManager,
    types::{BasicType, BasicTypeEnum, StructType},
    values::BasicValueEnum
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
    current_func_name: String
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

        Some(())
    }

    /// During the first pass, the types map is filled with all types.
    /// These types are opague, and filled later.
    fn first_pass(&mut self, declaration: &Declaration) -> Result<(), String> {
        match declaration {
            Declaration::Class { name, variables: _, methods: _ } => self.declare_type(name),
            Declaration::Enum { name, variants: _ } => self.declare_type(name),
            _ => Ok(())
        }
    }

    fn declare_type(&mut self, name: &Token) -> Result<(), String> {
        // The type will be correctly filled in in later passes
        let result = self.types.insert(name.lexeme.to_string(), self.context.opaque_struct_type(name.lexeme));
        if result.is_some() {
            Err(format!("The type/class/enum {} was declared more than once!", name.lexeme))   
        } else {
            Ok(())
        } 
    }

    /// During the second pass, all functions are declared.
    fn second_pass(&mut self, declaration: &Declaration) -> Result<(), String> {
        match declaration {
            Declaration::ExternFunction(func) => self.create_function(func),
            Declaration::Function(func) => self.create_function(&func.sig),
            _ => Ok(())
        }
    }

    fn create_function(&mut self, function: &FuncSignature) -> Result<(), String> {
        if self.module.get_function(&function.name.lexeme.to_string()).is_some() {
            return Err(format!("The function {} was declared more than once!", function.name.lexeme))   
        } 

        let mut parameters = Vec::new();
        for param in &function.parameters {
            parameters.push(self.resolve_type(&param._type)?);
        }
        let parameters = parameters.as_slice();

        let fn_type = if let Some(ret_type) = &function.return_type {
            self.resolve_type(&ret_type)?.fn_type(&parameters, false)
        } else {
            self.context.void_type().fn_type(&parameters, false)
        };

        self.module.add_function(function.name.lexeme, fn_type, None);
        self.environments.first_mut().unwrap().variables.insert(function.name.lexeme.to_string(), false);
        Ok(())
    }

    /// During the third pass, all variables inside functions are checked.
    /// This is to ensure the variable is defined and allowed in the current scope.
    fn third_pass(&mut self, declaration: &mut Declaration) -> Result<(), String> {
        match declaration {
            Declaration::Function(func) => {
                self.current_func_name = func.sig.name.lexeme.to_string();
               
                self.begin_scope();
                for param in func.sig.parameters.iter_mut() {
                    self.define_variable(&mut param.name, false, false)?;
                }
                self.resolve_expression(&mut func.body)?;
                self.end_scope();
            },
            _ => ()
        };

        Ok(())
    }

    fn resolve_statement(&mut self, statement: &mut Statement) -> Result<(), String> {
        match statement {
            Statement::Variable(var) => {
                self.resolve_expression(&mut var.initializer)?;
                self.define_variable(&mut var.name, !var.is_val, true)?;
            },

            Statement::Expression(expr) => self.resolve_expression(expr)?,

            _ => Err("Encountered unimplemented statement.")?,
        }

        Ok(())
    }

    fn resolve_expression(&mut self, expression: &mut Expression) -> Result<(), String> {
        match expression {
            Expression::Assignment { name, value } => {
                self.resolve_expression(value)?;
                let is_mut = self.find_var(name)?;
                if !is_mut {
                    return Err(format!("Variable {} is not assignable (val)", name.lexeme))
                }
            },

            Expression::Binary { left, operator: _, right } => {
                self.resolve_expression(left)?;
                self.resolve_expression(right)?;
            },

            Expression::Block(statements) => {
                self.begin_scope();
                for statement in statements {
                    self.resolve_statement(statement)?;
                }
                self.end_scope();
            },

            Expression::Call { callee, token: _, arguments } => {
                self.resolve_expression(callee)?;
                for arg in arguments {
                    self.resolve_expression(arg)?;
                }
            },

            Expression::Grouping(expr) => self.resolve_expression(expr)?,

            Expression::If { condition, then_branch, else_branch } => {
                self.resolve_expression(condition)?;
                self.resolve_expression(then_branch)?;
                if let Some(else_branch) = else_branch {
                    self.resolve_expression(else_branch)?;
                }
            },

            Expression::Return(expr) => {
                if let Some(expr) = expr {
                    self.resolve_expression(expr)?
                }
            },

            Expression::Literal(_) => (),

            Expression::Variable(name) => {
                self.find_var(name)?;
            },

            _ => Err("Encountered unimplemented expression.")?,
        }

        Ok(())
    }

    fn resolve_type(&mut self, token: &Token) -> Result<BasicTypeEnum, String> {
        Ok(match token.lexeme {
            "bool" => self.context.bool_type().as_basic_type_enum(),
            "f32" => self.context.f64_type().as_basic_type_enum(),
            "f64" => self.context.f64_type().as_basic_type_enum(),
            "i32" => self.context.i32_type().as_basic_type_enum(),
            "i64" => self.context.i64_type().as_basic_type_enum(),
            "String" => self.context.i8_type().ptr_type(AddressSpace::Generic).as_basic_type_enum(),
            _ => self.types.get(token.lexeme).ok_or(format!("Unknown type {}", token.lexeme))?.as_basic_type_enum(),
        })
    }

    fn define_variable(&mut self, token: &mut Token, mutable: bool, allow_redefine: bool) -> Result<(), String> {
        let name = token.lexeme.to_string();

        if self.find_var(token).is_ok() {
            let new_name = format!("{}-{}", name, self.environment_counter);
            self.environments.last_mut().unwrap().moved_vars.insert(name.clone(), new_name.clone());
            token.relocated = Some(new_name);
        }

        let was_defined = self.environments.last_mut().unwrap().variables.insert(name, mutable).is_some();
        
        if was_defined && !allow_redefine {
            Err(format!("Variable {} cannot be redefined in the same scope.", token.lexeme))
        } else {
            Ok(())
        }
    }

    fn find_var(&mut self, token: &mut Token) -> Result<bool, String> {
        let name = token.lexeme.to_string();

        for env in self.environments.iter().rev() {
            if env.variables.contains_key(&name) {
                if env.moved_vars.contains_key(&name) {
                    token.relocated = Some(env.moved_vars.get(&name).unwrap().to_string());
                }
                return Ok(*env.variables.get(&name).unwrap())
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
        types.insert("None".to_string(), context.struct_type(&[BasicTypeEnum::IntType(context.bool_type())], true));

        Resolver {
            context,
            module,
            builder,
            types,
            environments,
            environment_counter: 0,
            current_func_name: "".to_string()
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

        let none_const = self.types.get("None").unwrap().const_named_struct(&[BasicValueEnum::IntValue(self.context.bool_type().const_int(0, false))]);
        let none_const = BasicValueEnum::StructValue(none_const);

        IRGenerator {
            context: self.context,
            module: self.module,
            builder: self.builder,
            mpm,
            
            variables: HashMap::with_capacity(10),
            current_fn: None,

            declarations,

            none_const
        }
    }
}

/// An environment holds all variables in the current scope.
/// To keep track of all variables, a stack of them is used.
struct Environment {
    /// Key = name; value = mutability
    variables: HashMap<String, bool>,

    /// Variables that were moved due to a naming collision
    moved_vars: HashMap<String, String>
}

impl Environment {
    pub fn new() -> Environment {
        Environment {
            variables: HashMap::with_capacity(5),
            moved_vars: HashMap::with_capacity(5)
        }
    }
}
