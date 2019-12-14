/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/14/19 6:34 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::ast::Class as ASTClass;
use crate::ast::Function as ASTFunc;
use crate::ast::IFaceImpl as ASTImpl;
use crate::ast::Interface as ASTIFace;
use crate::ast::Type as ASTType;
use crate::lexer::token::Token;
use crate::mir::generator::{MIRError, MIRGenerator};
use crate::mir::generator::passes::declare_class::create_class;
use crate::mir::generator::passes::declare_func::create_function;
use crate::mir::generator::passes::declare_interface::create_interface;
use crate::mir::generator::passes::fill_class::fill_class;
use crate::mir::generator::passes::iface_impl::iface_impl;
use crate::mir::MutRc;
use crate::mir::nodes::{Class, Interface, Type, Variable};
use crate::option::Flatten;

/// A prototype that classes can be instantiated from.
/// This prototype is kept in AST form,
/// as all other MIR codegen would have to handle lots of
/// edge cases and be aware of prototypes otherwise.
///
/// Instead of that, prototypes are simply compiled on demand -
/// whenever they are instanced by using them somewhere,
/// the MIR generator takes the AST and generates it like
/// a regular class, with the generic parameters substituted
/// for their arguments.
///
/// The mayor drawback of this is that prototypes will
/// not produce compiler errors when not instanced,
/// but this is a small drawback compared to the complexity
/// of handling prototypes another way.
/// (Also, this missing check does not lead to unsound compiled code -
/// not producing unsound code is the most important reason of type checking.)
#[derive(Debug)]
pub struct ClassPrototype {
    pub ast: ASTClass,
    pub impls: Vec<ASTImpl>,
    pub instances: RefCell<HashMap<Vec<Type>, MutRc<Class>>>,
}

impl ClassPrototype {
    pub fn build(
        &self,
        gen: &mut MIRGenerator,
        arguments: Vec<Type>,
        err_tok: &Token
    ) -> Result<MutRc<Class>, MIRError> {
        if let Some(inst) = self.instances.borrow().get(&arguments) {
            return Ok(Rc::clone(&inst))
        }

        gen.builder.push_current_pointer();

        let generics = self.ast.generics.as_ref().unwrap();
        check_generic_arguments(gen, generics, &arguments, err_tok)?;
        gen.set_type_aliases(generics, &arguments);

        let mut ast = self.ast.clone();
        ast.name.lexeme = Rc::new(format!("{}-{}", ast.name.lexeme, self.instances.borrow().len()));

        let class = create_class(gen, &mut ast)?;
        self.instances.borrow_mut().insert(arguments, Rc::clone(&class));
        fill_class(gen, &ast)?;

        let mut impls = self.impls.clone();
        for im in impls.iter_mut() {
            iface_impl(gen, im, Some(Type::Class(Rc::clone(&class))))?;
        }

        gen.generate_constructors(&ast)?;
        for func in ast.methods.iter().chain(impls.iter().map(|i| i.methods.iter()).flatten()) {
            gen.generate_function(func)?;
        }

        gen.builder.load_last_pointer();
        gen.clear_type_aliases();
        Ok(class)
    }
}

impl PartialEq for ClassPrototype {
    fn eq(&self, other: &Self) -> bool {
        self.ast.name.lexeme == other.ast.name.lexeme
    }
}

impl Hash for ClassPrototype {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ast.name.lexeme.hash(state)
    }
}

/// See doc on [ClassPrototype].
#[derive(Debug)]
pub struct InterfacePrototype {
    pub ast: ASTIFace,
    pub impls: Vec<ASTImpl>,
    pub instances: RefCell<HashMap<Vec<Type>, MutRc<Interface>>>,
}

impl InterfacePrototype {
    pub fn build(
        &self,
        gen: &mut MIRGenerator,
        arguments: Vec<Type>,
        err_tok: &Token
    ) -> Result<MutRc<Interface>, MIRError> {
        if let Some(inst) = self.instances.borrow().get(&arguments) {
            return Ok(Rc::clone(&inst))
        }

        gen.builder.push_current_pointer();

        let generics = self.ast.generics.as_ref().unwrap();
        check_generic_arguments(gen, generics, &arguments, err_tok)?;
        gen.set_type_aliases(generics, &arguments);

        let mut ast = self.ast.clone();
        ast.name.lexeme = Rc::new(format!("{}-{}", ast.name.lexeme, self.instances.borrow().len()));
        let iface = create_interface(gen, &mut ast)?;

        gen.builder.load_last_pointer();
        gen.clear_type_aliases();
        self.instances.borrow_mut().insert(arguments, Rc::clone(&iface));
        Ok(iface)
    }
}

impl PartialEq for InterfacePrototype {
    fn eq(&self, other: &Self) -> bool {
        self.ast.name.lexeme == other.ast.name.lexeme
    }
}

/// A function. See notes on [ClassPrototype].
#[derive(Debug)]
pub struct FunctionPrototype {
    pub ast: ASTFunc,
    pub instances: RefCell<HashMap<Vec<Type>, Rc<Variable>>>,
}

impl FunctionPrototype {
    pub fn build(
        &mut self,
        gen: &mut MIRGenerator,
        arguments: Vec<Type>,
        err_tok: &Token
    ) -> Result<Rc<Variable>, MIRError> {
        if let Some(inst) = self.instances.borrow().get(&arguments) {
            return Ok(Rc::clone(&inst))
        }

        gen.builder.push_current_pointer();

        let generics = self.ast.sig.generics.as_ref().unwrap();
        check_generic_arguments(gen, generics, &arguments, err_tok)?;
        gen.set_type_aliases(generics, &arguments);

        let old_name = Rc::clone(&self.ast.sig.name.lexeme);
        self.ast.sig.name.lexeme = Rc::new(format!("{}-{}", old_name, self.instances.borrow().len()));
        let func = create_function(gen, &self.ast.sig, self.ast.body.is_none())?;
        gen.generate_function(&self.ast)?;
        self.ast.sig.name.lexeme = old_name;

        gen.builder.load_last_pointer();
        gen.clear_type_aliases();
        self.instances.borrow_mut().insert(arguments, Rc::clone(&func));
        Ok(func)
    }
}

fn check_generic_arguments(
    gen: &mut MIRGenerator,
    parameters: &Vec<Token>,
    arguments: &Vec<Type>,
    err_tok: &Token
) -> Result<(), MIRError> {
    if parameters.len() != arguments.len() {
        return Err(gen.error(err_tok, err_tok, &format!(
            "Wrong amount of generic parameters (expected {}; got {})",
            parameters.len(),
            arguments.len()
        )));
    }
    Ok(())
}
