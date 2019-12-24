/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/24/19 4:21 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast::Function as ASTFunc;
use crate::ast::IFaceImpl as ASTImpl;
use crate::ast::Interface as ASTIFace;
use crate::ast::Type as ASTType;
use crate::ast::{Class as ASTClass, IFaceImpl};
use crate::error::{Error, Res};
use crate::lexer::token::Token;
use crate::mir::generator::builder::{Context, MIRBuilder};
use crate::mir::generator::module::DONE_PASSES;
use crate::mir::generator::passes::declaring_globals::get_function_name;
use crate::mir::generator::MIRGenerator;
use crate::mir::nodes::{Class, Function, Interface, Type, Variable};
use crate::mir::{mutrc_new, MModule, MutRc};
use crate::mir::generator::passes::declaring_iface_impls::declare_impl;

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
    pub impls: RefCell<Vec<ASTImpl>>,
    pub module: MutRc<MModule>,
    pub ast: ProtoAST,
}

impl Prototype {
    pub fn build(
        &self,
        arguments: Vec<Type>,
        err_tok: &Token,
        self_ref: Rc<Prototype>,
    ) -> Res<Type> {
        if let Some(inst) = self.instances.borrow().get(&arguments) {
            return Ok(inst.clone());
        }

        check_generic_arguments(&self.module, self.ast.get_parameters(), &arguments, err_tok)?;

        let name = get_name(self.ast.get_name(&self_ref), &arguments);
        let ty = self.ast.create_mir(&name, &arguments, self_ref)?;
        let mut generator = MIRGenerator::new(MIRBuilder::new(&self.module));

        self.module.borrow_mut().types.insert(Rc::clone(&name), ty.clone());
        self.instances.borrow_mut().insert(arguments, ty.clone());

        generator.builder.context = ty.context().unwrap();
        attach_impls(&mut generator.builder, &ty, &name, &self.impls.borrow())?;
        catch_up_passes(&mut generator, &ty)?;

        Ok(ty)
    }
}

#[derive(Debug, Clone, EnumIsA)]
pub enum ProtoAST {
    Class(Rc<ASTClass>),
    Interface(Rc<ASTIFace>),
    Function(Rc<ASTFunc>),
}

impl ProtoAST {
    fn get_name(&self, self_ref: &Rc<Prototype>) -> Rc<String> {
        match self {
            ProtoAST::Class(c) => Rc::clone(&c.name.lexeme),
            ProtoAST::Interface(i) => Rc::clone(&i.name.lexeme),
            ProtoAST::Function(f) => Rc::new(get_function_name(
                &self_ref.module.borrow().path,
                &f.sig.name.lexeme,
            )),
        }
    }

    fn get_parameters(&self) -> &[Token] {
        match self {
            ProtoAST::Class(c) => c.generics.as_ref().unwrap(),
            ProtoAST::Interface(i) => i.generics.as_ref().unwrap(),
            ProtoAST::Function(f) => f.sig.generics.as_ref().unwrap(),
        }
    }

    fn create_mir(
        &self,
        name: &Rc<String>,
        arguments: &[Type],
        self_ref: Rc<Prototype>,
    ) -> Res<Type> {
        Ok(match self {
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
                    proto: Some(self_ref),
                    context: get_context(ast.generics.as_ref().unwrap(), arguments),
                    ast: Rc::new(ast),
                });
                Type::Interface(iface)
            }

            ProtoAST::Function(ast) => {
                let mut ast = (**ast).clone();
                ast.sig.name.lexeme = Rc::clone(&name);

                let mut builder = MIRBuilder::new(&self_ref.module);
                builder.context = get_context(ast.sig.generics.as_ref().unwrap(), arguments);

                let ret_type = ast
                    .sig
                    .return_type
                    .as_ref()
                    .map(|ty| builder.find_type(ty))
                    .unwrap_or(Ok(Type::None))?;

                let mut parameters = Vec::with_capacity(ast.sig.parameters.len());
                for param in ast.sig.parameters.iter() {
                    parameters.push(Rc::new(Variable {
                        mutable: false,
                        name: Rc::clone(&param.name.lexeme),
                        type_: builder.find_type(&param.type_)?,
                    }));
                }

                let func = mutrc_new(Function {
                    name: (**name).clone(),
                    parameters,
                    blocks: Default::default(),
                    variables: Default::default(),
                    ret_type,
                    context: builder.context,
                    ast: Some(Rc::new(ast)),
                });

                let global = Rc::new(Variable {
                    name: Rc::clone(&name),
                    type_: Type::Function(Rc::clone(&func)),
                    mutable: false,
                });
                self_ref
                    .module
                    .borrow_mut()
                    .globals
                    .insert(Rc::clone(&name), global);

                Type::Function(func)
            }
        })
    }
}

fn get_name(name: Rc<String>, args: &[Type]) -> Rc<String> {
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

fn attach_impls(builder: &mut MIRBuilder, ty: &Type, name: &Rc<String>, impls: &[IFaceImpl]) -> Res<()> {
    for im in impls {
        let mut ast = im.clone();
        let mut tok = ast.implementor.get_token().clone();
        tok.lexeme = Rc::clone(&name);
        ast.implementor = ASTType::Ident(tok);
        declare_impl(ast, builder, Some(ty.clone()))?;
    }
    Ok(())
}

fn catch_up_passes(gen: &mut MIRGenerator, ty: &Type) -> Res<()> {
    let len = DONE_PASSES.with(|d| d.borrow().len());
    for i in 0..len {
        DONE_PASSES.with(|d| d.borrow()[i].run_type(gen, ty.clone()))?
    }
    Ok(())
}
