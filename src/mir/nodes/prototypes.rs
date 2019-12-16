/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/16/19 9:25 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use indexmap::IndexMap;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::Function as ASTFunc;
use crate::ast::IFaceImpl as ASTImpl;
use crate::ast::Interface as ASTIFace;
use crate::ast::{Class as ASTClass, IFaceImpl};
use crate::error::{Error, Res};
use crate::lexer::token::Token;
use crate::mir::nodes::{Class, Type};
use crate::mir::{mutrc_new, MModule, MutRc};

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
    pub proto: Prototypes,
}

impl Prototype {
    pub fn build(&self, arguments: Vec<Type>, err_tok: &Token) -> Res<Type> {
        if let Some(inst) = self.instances.borrow().get(&arguments) {
            return Ok(inst.clone());
        }

        let ty = match &self.proto {
            Prototypes::Class(class) => class.borrow().build(&arguments, err_tok),
            Prototypes::Interface(iface) => iface.borrow().build(&arguments, err_tok),
            Prototypes::Function(func) => func.borrow().build(&arguments, err_tok),
        }?;
        self.instances.borrow_mut().insert(arguments, ty.clone());

        Ok(ty)
    }
}

#[derive(Debug, Clone, EnumAsGetters)]
pub enum Prototypes {
    Class(MutRc<ClassPrototype>),
    Interface(MutRc<InterfacePrototype>),
    Function(MutRc<FunctionPrototype>),
}

#[derive(Debug)]
pub struct ClassPrototype {
    pub ast: ASTClass,
    pub impls: Vec<ASTImpl>,
    pub module: MutRc<MModule>,
}

impl ClassPrototype {
    fn build(&self, arguments: &[Type], err_tok: &Token) -> Res<Type> {
        check_generic_arguments(
            &self.module,
            self.ast.generics.as_ref().unwrap(),
            arguments,
            err_tok,
        )?;

        let mut ast = self.ast.clone();
        let name = get_name(&self.ast.name.lexeme, arguments);
        ast.name.lexeme = Rc::clone(&name);

        let class = mutrc_new(Class {
            name: Rc::clone(&name),
            members: IndexMap::with_capacity(ast.variables.len()),
            methods: HashMap::with_capacity(ast.methods.len()),
            instantiator: Default::default(),
            constructors: Vec::with_capacity(ast.constructors.len()),
            ast: Rc::new(ast),
        });
        let ty = Type::Class(class);

        attach_impls(&ty, self.impls.clone())?;
        catch_up_passes(&ty)?;
        Ok(ty)
    }
}

#[derive(Debug)]
pub struct InterfacePrototype {
    pub ast: ASTIFace,
    pub impls: Vec<ASTImpl>,
}

impl InterfacePrototype {
    fn build(&self, _arguments: &[Type], _err_tok: &Token) -> Res<Type> {
        unimplemented!()
    }
}

#[derive(Debug)]
pub struct FunctionPrototype {
    pub ast: ASTFunc,
}

impl FunctionPrototype {
    fn build(&self, _arguments: &[Type], _err_tok: &Token) -> Res<Type> {
        unimplemented!()
    }
}

fn get_name(name: &Rc<String>, args: &[Type]) -> Rc<String> {
    let mut arg_names = args[0].to_string();
    for arg in args.iter().skip(1) {
        arg_names = format!("{}, {}", arg_names, arg);
    }
    Rc::new(format!("{}<{}>", name, arg_names))
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

fn catch_up_passes(ty: &Type) -> Res<()> {
    Ok(())
}
