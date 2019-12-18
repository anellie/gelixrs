/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/18/19 11:08 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast::{Class as ASTClass, IFaceImpl};
use crate::ast::Function as ASTFunc;
use crate::ast::IFaceImpl as ASTImpl;
use crate::ast::Interface as ASTIFace;
use crate::error::{Error, Res};
use crate::lexer::token::Token;
use crate::mir::{MModule, MutRc, mutrc_new};
use crate::mir::generator::builder::{Context, MIRBuilder};
use crate::mir::generator::MIRGenerator;
use crate::mir::generator::module::DONE_PASSES;
use crate::mir::nodes::{Class, Interface, Type};

/// A prototype that classes can be instantiated from.
/// This prototype is kept in AST form,
/// as all other MIR codegen would have to handle lots of
/// edge cases and be aware of prototypes otherwise.
///
/// Instead of that, prototypes are simply compiled on demand -
/// whenever they are instanced by using them somewhere,
/// the MIR generator takes the AST and generates it like
/// a regular node, with the generic parameters substituted
/// for their arguments.
///
/// The mayor drawback of this is that prototypes will
/// not produce compiler errors when not instanced,
/// but this is a small drawback compared to the complexity
/// of handling prototypes another way.
/// (Also, this missing check does not lead to unsound compiled code -
/// not producing unsound code is the most important reason of type checking.)
#[derive(Debug, Clone)]
pub struct Prototype {
    pub name: Rc<String>,
    pub instances: RefCell<HashMap<Vec<Type>, Type>>,
    pub impls: Vec<ASTImpl>,
    pub module: MutRc<MModule>,
    pub ast: ProtoAST,
}

impl Prototype {
    pub fn build(&self, arguments: Vec<Type>, err_tok: &Token) -> Res<Type> {
        if let Some(inst) = self.instances.borrow().get(&arguments) {
            return Ok(inst.clone());
        }

        check_generic_arguments(&self.module, self.ast.get_parameters(), &arguments, err_tok)?;

        let name = get_name(self.ast.get_name(), &arguments);
        let ty = self.ast.create_mir(&name, &arguments);
        let mut generator = MIRGenerator::new(MIRBuilder::new(&self.module));
        attach_impls(&ty, self.impls.clone())?;
        catch_up_passes(&mut generator, &ty)?;

        self.module.borrow_mut().types.insert(name, ty.clone());
        self.instances.borrow_mut().insert(arguments, ty.clone());

        Ok(ty)
    }
}

#[derive(Debug, Clone)]
pub enum ProtoAST {
    Class(Rc<ASTClass>),
    Interface(Rc<ASTIFace>),
    Function(Rc<ASTFunc>),
}

impl ProtoAST {
    fn get_name(&self) -> &Rc<String> {
        match self {
            ProtoAST::Class(c) => &c.name.lexeme,
            ProtoAST::Interface(i) => &i.name.lexeme,
            ProtoAST::Function(f) => &f.sig.name.lexeme,
        }
    }

    fn get_parameters(&self) -> &[Token] {
        match self {
            ProtoAST::Class(c) => c.generics.as_ref().unwrap(),
            ProtoAST::Interface(i) => i.generics.as_ref().unwrap(),
            ProtoAST::Function(f) => f.sig.generics.as_ref().unwrap(),
        }
    }

    fn create_mir(&self, name: &Rc<String>, arguments: &[Type]) -> Type {
        match self {
            ProtoAST::Class(ast) => {
                let mut ast = (**ast).clone();
                ast.name.lexeme = Rc::clone(&name);
                let class = mutrc_new(Class {
                    name: Rc::clone(&name),
                    members: IndexMap::with_capacity(ast.variables.len()),
                    methods: HashMap::with_capacity(ast.methods.len()),
                    instantiator: Default::default(),
                    constructors: Vec::with_capacity(ast.constructors.len()),
                    context: get_context(ast.generics.as_ref().unwrap(), arguments),
                    ast: Rc::new(ast),
                });
                Type::Class(class)
            }

            ProtoAST::Interface(ast) => {
                let mut ast = (**ast).clone();
                ast.name.lexeme = Rc::clone(&name);
                let iface = mutrc_new(Interface {
                    name: Rc::clone(&name),
                    methods: IndexMap::with_capacity(ast.methods.len()),
                    proto: None,
                    context: get_context(ast.generics.as_ref().unwrap(), arguments),
                    ast: Rc::new(ast),
                });
                Type::Interface(iface)
            }

            ProtoAST::Function(_) => unimplemented!(),
        }
    }
}

fn get_name(name: &Rc<String>, args: &[Type]) -> Rc<String> {
    let mut arg_names = args[0].to_string();
    for arg in args.iter().skip(1) {
        arg_names = format!("{}, {}", arg_names, arg);
    }
    Rc::new(format!("{}<{}>", name, arg_names))
}

fn get_context(params: &[Token], args: &[Type]) -> Context {
    Context {
        type_aliases: Rc::new(
            params
                .iter()
                .map(|p| Rc::clone(&p.lexeme))
                .zip(args.iter().cloned())
                .collect(),
        ),
    }
}

fn check_generic_arguments(
    module: &MutRc<MModule>,
    parameters: &[Token],
    arguments: &[Type],
    err_tok: &Token,
) -> Result<(), Error> {
    if parameters.len() != arguments.len() {
        return Err(Error::new(
            err_tok,
            "MIR",
            format!(
                "Wrong amount of generic parameters (expected {}; got {})",
                parameters.len(),
                arguments.len()
            ),
            &module.borrow().path,
        ));
    }
    Ok(())
}

fn attach_impls(ty: &Type, impls: Vec<IFaceImpl>) -> Res<()> {
    Ok(())
}

fn catch_up_passes(gen: &mut MIRGenerator, ty: &Type) -> Res<()> {
    let len = DONE_PASSES.with(|d| d.borrow().len());
    for i in 0..len {
        DONE_PASSES.with(|d| d.borrow()[i].run_type(gen, ty.clone()))?
    }
    Ok(())
}
