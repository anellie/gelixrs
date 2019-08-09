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
};
use std::collections::HashMap;

pub struct Resolver<'r> {
    context: Context,
    builder: Builder,
    module: Module,

    declarations: Vec<Declaration<'r>>
}

impl<'r> Resolver<'r> {
    pub fn resolve(&mut self) -> Option<()> {
        Some(()) // TODO
    }

    pub fn new(declarations: Vec<Declaration>) -> Resolver {
        let context = Context::create();
        let module = context.create_module("main");
        let builder = context.create_builder();

        Resolver {
            context,
            module,
            builder,
            declarations
        }
    }

    /// Turns the resolver into a generator for IR. Call resolve() first.
    pub fn into_generator(mut self) -> IRGenerator<'r> {
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
        self.declarations.reverse();

        IRGenerator {
            context: self.context,
            module: self.module,
            builder: self.builder,
            fpm,
            
            variables: HashMap::with_capacity(10),
            current_fn: None,

            declarations: self.declarations,
        }
    }
}