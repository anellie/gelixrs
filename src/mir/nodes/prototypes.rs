/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/28/19 1:12 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crate::{
    ast::{
        Class as ASTClass, Function as ASTFunc, IFaceImpl as ASTImpl, IFaceImpl,
        Interface as ASTIFace, Type as ASTType, Enum as ASTEnum,
    },
    error::{Error, Res},
    lexer::token::Token,
    mir::{
        generator::{
            builder::{Context, MIRBuilder},
            module::DONE_PASSES,
            passes::{
                declaring_globals::{generate_mir_fn, get_function_name, insert_global_and_type},
                declaring_iface_impls::declare_impl,
            },
            MIRGenerator,
        },
        nodes::{Class, Interface, Type, Variable},
        MModule, MutRc,
    },
};
use either::Either::Right;
use crate::mir::nodes::Enum;

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
    pub impls: RefCell<Vec<(ASTImpl, MutRc<MModule>)>>,
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

        self.module
            .borrow_mut()
            .types
            .insert(Rc::clone(&name), ty.clone());
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
    Enum(Rc<ASTEnum>)
}

impl ProtoAST {
    fn get_name(&self, self_ref: &Rc<Prototype>) -> Rc<String> {
        match self {
            ProtoAST::Class(c) => Rc::clone(&c.name.lexeme),
            ProtoAST::Interface(i) => Rc::clone(&i.name.lexeme),
            ProtoAST::Enum(e) => Rc::clone(&e.name.lexeme),
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
            ProtoAST::Enum(e) => e.generics.as_ref().unwrap()
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

                let context = get_context(ast.generics.as_ref().unwrap(), arguments);
                let class = Class::from_ast(ast, context);
                Type::Class(class)
            }

            ProtoAST::Interface(ast) => {
                let mut ast = (**ast).clone();
                ast.name.lexeme = Rc::clone(&name);

                let context = get_context(ast.generics.as_ref().unwrap(), arguments);
                let iface = Interface::from_ast(ast, Some(self_ref), context);
                Type::Interface(iface)
            }

            ProtoAST::Function(ast) => {
                let mut ast = (**ast).clone();
                ast.sig.name.lexeme = Rc::clone(&name);

                let builder = MIRBuilder::with_context(
                    &self_ref.module,
                    get_context(ast.sig.generics.as_ref().unwrap(), arguments),
                );
                let mir_fn = generate_mir_fn(&builder, Right(ast), String::clone(name), None)?;
                let global = Variable::new(false, Type::Function(Rc::clone(&mir_fn)), name);
                insert_global_and_type(&builder.module, &global);

                Type::Function(mir_fn)
            }

            ProtoAST::Enum(ast) => {
                let mut ast = (**ast).clone();
                ast.name.lexeme = Rc::clone(&name);

                let context = get_context(ast.generics.as_ref().unwrap(), arguments);
                let enu = Enum::from_ast(ast, context);
                Type::Enum(enu)
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

fn attach_impls(
    builder: &mut MIRBuilder,
    ty: &Type,
    name: &Rc<String>,
    impls: &[(IFaceImpl, MutRc<MModule>)],
) -> Res<()> {
    for (im, module) in impls {
        builder.switch_module(module);
        let mut ast = im.clone();
        let mut tok = ast.implementor.get_token().clone();
        tok.lexeme = Rc::clone(&name);
        ast.implementor = ASTType::Ident(tok);
        declare_impl(ast, builder, Some(ty.clone()))?;
    }
    Ok(())
}

pub fn catch_up_passes(gen: &mut MIRGenerator, ty: &Type) -> Res<()> {
    let module = Rc::clone(&gen.module);
    let len = DONE_PASSES.with(|d| d.borrow().len());
    for i in 0..len {
        gen.switch_module(&module);
        DONE_PASSES.with(|d| d.borrow()[i].run_type(gen, ty.clone()))?
    }
    Ok(())
}
