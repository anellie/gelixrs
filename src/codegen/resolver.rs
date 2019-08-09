use super::IRGenerator;
use super::super::{
    ast::{
        declaration::{Declaration, Function, FuncSignature, Variable},
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
    types::{BasicType, BasicTypeEnum},
    values::{BasicValueEnum, FunctionValue, PointerValue},
};
use std::collections::HashMap;

pub struct Resolver {
    context: Context,
    builder: Builder,
    module: Module,

    types: HashMap<String, Box<dyn BasicType>>,
    func_declared: Vec<String>,
    functions: HashMap<String, FunctionValue>
}

impl Resolver {
    pub fn resolve(&mut self, declarations: &Vec<Declaration>) -> Option<()> {
        for declaration in declarations {
            Resolver::check_error(self.first_pass(&declaration))?;
        }

        for declaration in declarations {
            Resolver::check_error(self.second_pass(&declaration))?;
        }

        Some(())
    }

    // During the first pass, the types map is filled with all types.
    // These types are not yet filled in however.
    fn first_pass(&mut self, declaration: &Declaration) -> Result<(), String> {
        match declaration {
            Declaration::Class { name, variables: _, methods: _ } => self.declare_type(name),
            Declaration::Enum { name, variants: _ } => self.declare_type(name),
            Declaration::CFunc(func) => self.declare_function(func),
            Declaration::Function(func) => self.declare_function(&func.sig)
        }
    }

    fn declare_type(&mut self, name: &Token) -> Result<(), String> {
        // The type will be correctly filled in in later passes
        let result = self.types.insert(name.lexeme.to_string(), Box::new(self.context.bool_type()));
        if result.is_some() {
            Err(format!("The type/class/enum {} was declared more than once!", name.lexeme))   
        } else {
            Ok(())
        } 
    }

    fn declare_function(&mut self, function: &FuncSignature) -> Result<(), String> {
        if self.func_declared.contains(&function.name.lexeme.to_string()) {
            Err(format!("The function {} was declared more than once!", function.name.lexeme))   
        } else {
            self.func_declared.push(function.name.lexeme.to_string());
            Ok(())
        } 
    }

    fn second_pass(&mut self, declaration: &Declaration) -> Result<(), String> {
        match declaration {
            Declaration::CFunc(func) => self.external_function(func),
            Declaration::Class { name, variables, methods } => self.class(name, variables, methods),
            Declaration::Enum { name, variants } => self._enum(name, variants),
            Declaration::Function(func) => self.function(func)
        }
    }

    fn external_function(&mut self, func: &FuncSignature) -> Result<(), String> {
        Ok(())
    }

    fn class(
        &mut self,         
        name: &Token,
        variables: &Vec<Variable>,
        methods: &Vec<Function>,
    ) -> Result<(), String> {
        Ok(())
    }    
    
    fn _enum(&mut self, name: &Token, variants: &Vec<Token>) -> Result<(), String> {
        Ok(())

    }    
    
    fn function(&mut self, func: &Function) -> Result<(), String> {
        Ok(())

    }
/*
    fn resolve_type(&mut self) -> Result<BasicType, String> {

    }
*/
    fn check_error(result: Result<(), String>) -> Option<()> {
        result.or_else(|err| {
            eprintln!("[Resolver] {}", err);
            Err(())
        }).ok()
    }

    pub fn new() -> Resolver {
        let context = Context::create();
        let module = context.create_module("main");
        let builder = context.create_builder();

        Resolver {
            context,
            module,
            builder,
            types: HashMap::with_capacity(10),
            func_declared: Vec::with_capacity(10),
            functions: HashMap::with_capacity(10)
        }
    }

    /// Turns the resolver into a generator for IR. Call resolve() first.
    pub fn into_generator(self, mut declarations: Vec<Declaration>) -> IRGenerator {
        let fpm = PassManager::create(&self.module);
        fpm.add_instruction_combining_pass();
        fpm.add_reassociate_pass();
        fpm.add_gvn_pass();
        fpm.add_cfg_simplification_pass();
        fpm.add_basic_alias_analysis_pass();
        fpm.add_promote_memory_to_register_pass();
        fpm.add_instruction_combining_pass();
        fpm.add_reassociate_pass();

        // The generator pops the declarations off the top.
        declarations.reverse();

        IRGenerator {
            context: self.context,
            module: self.module,
            builder: self.builder,
            fpm,
            
            variables: HashMap::with_capacity(10),
            current_fn: None,

            declarations,
        }
    }
}