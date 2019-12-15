/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 2:07 AM.
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
use crate::error::{Error, Res};
use crate::lexer::token::Token;
use crate::mir::{MModule, MutRc};
use crate::mir::nodes::{Class, Interface, Type, Variable};

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
pub trait Prototype {
    /// A simple function that returns the name for Eq and Hash traits.
    fn name(&self) -> Rc<String>;
    /// Build the prototype into a type.
    fn build(&self, arguments: Vec<Type>, err_tok: &Token) -> Res<Type>;
}

impl PartialEq for dyn Prototype {
    fn eq(&self, other: &Self) -> bool {
        self.name() == other.name()
    }
}

impl Hash for dyn Prototype {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name().hash(state)
    }
}

pub struct ClassPrototype {
    pub ast: ASTClass,
    pub impls: Vec<ASTImpl>,
    pub instances: RefCell<HashMap<Vec<Type>, MutRc<Class>>>,
}

impl Prototype for ClassPrototype {
    fn name(&self) -> Rc<String> {
        Rc::clone(&self.ast.name.lexeme)
    }

    fn build(&self, arguments: Vec<Type>, err_tok: &Token) -> Res<Type> {
        /*  if let Some(inst) = self.instances.borrow().get(&arguments) {
              return Ok(Type::Class(Rc::clone(&inst)))
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
          Ok(Type::Class(class))
          */
        unimplemented!()
    }
}


/// See doc on [ClassPrototype].
#[derive(Debug)]
pub struct InterfacePrototype {
    pub ast: ASTIFace,
    pub impls: Vec<ASTImpl>,
    pub instances: RefCell<HashMap<Vec<Type>, MutRc<Interface>>>,
}

impl Prototype for InterfacePrototype {
    fn name(&self) -> Rc<String> {
        Rc::clone(&self.ast.name.lexeme)
    }

    fn build(&self, arguments: Vec<Type>, err_tok: &Token) -> Res<Type> {
        /*
        if let Some(inst) = self.instances.borrow().get(&arguments) {
            return Ok(Type::Interface(Rc::clone(&inst)))
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
        Ok(Type::Interface(iface))
        */
        unimplemented!()
    }
}

pub struct FunctionPrototype {
    pub ast: ASTFunc,
    pub instances: RefCell<HashMap<Vec<Type>, Rc<Variable>>>,
}

impl Prototype for FunctionPrototype {
    fn name(&self) -> Rc<String> {
        Rc::clone(&self.ast.sig.name.lexeme)
    }

    fn build(&self, arguments: Vec<Type>, err_tok: &Token) -> Res<Type> {
        /*       if let Some(inst) = self.instances.borrow().get(&arguments) {
                   return Ok(Type::Function(Rc::clone(&inst.type_.as_function())))
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
               Ok(Type::Function(func.type_.as_function()))
       */
        unimplemented!()
    }
}

fn check_generic_arguments(
    module: &MutRc<MModule>,
    parameters: &Vec<Token>,
    arguments: &Vec<Type>,
    err_tok: &Token
) -> Result<(), Error> {
    if parameters.len() != arguments.len() {
        return Err(Error::new(err_tok, "MIR", format!(
            "Wrong amount of generic parameters (expected {}; got {})",
            parameters.len(),
            arguments.len()
        ), &module.borrow().path));
    }
    Ok(())
}
