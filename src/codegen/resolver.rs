use super::IRGenerator;
use super::super::{
    ast::declaration::{Declaration, Function, FuncSignature, Variable},
    lexer::token::Token,
};
use inkwell::{
    builder::Builder,
    context::Context,
    module::Module,
    passes::PassManager,
    types::{BasicType, BasicTypeEnum, StructType},
};
use std::collections::HashMap;

/// A resolver. Mainly responsible for resolving types.
/// Variable resolution is done in [IRGenerator] directly.
pub struct Resolver {
    context: Context,
    builder: Builder,
    module: Module,

    types: HashMap<String, StructType>,
}

impl Resolver {
    /// Will do all passes after one another.
    pub fn resolve(&mut self, declarations: &Vec<Declaration>) -> Option<()> {
        for declaration in declarations {
            Resolver::check_error(self.first_pass(&declaration))?;
        }

        for declaration in declarations {
            Resolver::check_error(self.second_pass(&declaration))?;
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
            Declaration::CFunc(func) => self.create_function(func),
            Declaration::Class { name, variables, methods } => self.class(name, variables, methods),
            Declaration::Enum { name, variants } => self._enum(name, variants),
            Declaration::Function(func) => self.create_function(&func.sig)
        }
    }

    fn create_function(&mut self, function: &FuncSignature) -> Result<(), String> {
        if self.module.get_function(&function.name.lexeme.to_string()).is_some() {
            return Err(format!("The function {} was declared more than once!", function.name.lexeme))   
        } 

        let mut parameters = Vec::new();
        for param in &function.parameters {
            parameters.push(self.resolve_type(&param.0)?);
        }
        let parameters = parameters.as_slice();

        let fn_type = if let Some(ret_type) = &function.return_type {
            self.resolve_type(&ret_type)?.fn_type(&parameters, false)
        } else {
            self.context.void_type().fn_type(&parameters, false)
        };
        
        self.module.add_function(function.name.lexeme, fn_type, None);
        Ok(())
    }

    fn class(
        &mut self,         
        _name: &Token,
        _variables: &Vec<Variable>,
        _methods: &Vec<Function>,
    ) -> Result<(), String> {
        Ok(())
    }    
    
    fn _enum(&mut self, _name: &Token, _variants: &Vec<Token>) -> Result<(), String> {
        Ok(())
    }    
    

    fn resolve_type(&mut self, token: &Token) -> Result<BasicTypeEnum, String> {
        Ok(match token.lexeme {
            "f32" => self.context.f64_type().as_basic_type_enum(),
            "f64" => self.context.f64_type().as_basic_type_enum(),
            "i32" => self.context.i32_type().as_basic_type_enum(),
            "i64" => self.context.i64_type().as_basic_type_enum(),
            "bool" => self.context.bool_type().as_basic_type_enum(),
            _ => self.types.get(token.lexeme).ok_or(format!("Unknown type {}", token.lexeme))?.as_basic_type_enum(),
        })
    }

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