/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/5/19 10:03 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::ast::Class as ASTClass;
use crate::ast::IFaceImpl as ASTImpl;
use crate::ast::Interface as ASTIFace;
use crate::ast::Function as ASTFunc;
use crate::mir::{MutRc};
use crate::mir::generator::{MIRGenerator, MIRError};
use crate::mir::nodes::{
    Class, Interface, Type, Variable,
};

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
    pub instances: HashMap<Vec<Type>, MutRc<Class>>
}

impl ClassPrototype {
    pub fn build(
        &mut self,
        _gen: &mut MIRGenerator,
        _arguments: Vec<Type>,
    ) -> Result<MutRc<Class>, MIRError> {
        unimplemented!()
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
    pub instances: HashMap<Vec<Type>, MutRc<Interface>>
}

impl InterfacePrototype {
    pub fn build(&mut self,
         _gen: &mut MIRGenerator,
         _arguments: &Vec<Type>
    ) -> Result<MutRc<Interface>, String> {
        unimplemented!()
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
    pub impls: Vec<ASTImpl>,
    pub instances: HashMap<Vec<Type>, Rc<Variable>>
}

impl FunctionPrototype {
    pub fn build(
        &mut self,
        _gen: &mut MIRGenerator,
        _arguments: &Vec<Type>,
    ) -> Result<Rc<Variable>, String> {
        unimplemented!()
    }
}

fn check_generic_arguments(
    parameters: &Vec<Rc<String>>,
    arguments: &Vec<Type>,
) -> Result<(), String> {
    if parameters.len() != arguments.len() {
        return Err(format!(
            "Wrong amount of generic parameters (expected {}; got {})",
            parameters.len(),
            arguments.len()
        ));
    }
    Ok(())
}
