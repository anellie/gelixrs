/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 11/4/19 8:03 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use indexmap::IndexMap;

use crate::mir::{MutRc, mutrc_new};
use crate::mir::nodes::{Block, Class, ClassMember, Function, IFaceMethod, Interface, Type, Variable};

#[derive(Debug, Default)]
pub struct ClassPrototype {
    pub name: Rc<String>,
    pub members: IndexMap<Rc<String>, Rc<ClassMember>>,
    pub methods: HashMap<Rc<String>, MutRc<FunctionPrototype>>,
    pub interfaces: IndexMap<Rc<String>, MutRc<Interface>>,
    pub generic_args: Vec<Rc<String>>,
    pub instances: HashMap<Vec<Type>, MutRc<Class>>,
}

impl ClassPrototype {
    pub fn build(&mut self, arguments: &Vec<Type>) -> Result<MutRc<Class>, String> {
        if let Some(class) = self.instances.get(arguments) {
            return Ok(Rc::clone(&class));
        }

        check_generic_arguments(&self.generic_args, arguments)?;

        let members = self.members.iter().map(|(name, member)| {
            let mut member = ClassMember::clone(member);
            member.type_ = replace_generic(member.type_, arguments);
            (Rc::clone(name), Rc::new(member))
        }).collect();

        let methods = self
            .methods
            .iter()
            .map(|(name, method)| {
                let method = method.borrow_mut().build(arguments).unwrap();
                let var = Variable {
                    mutable: false,
                    type_: Type::Function(method),
                    name: Rc::clone(name),
                };
                (Rc::clone(name), Rc::new(var))
            })
            .collect();

        // TODO: replace generic types in interfaces
        let interfaces = self
            .interfaces
            .clone();

        let class = mutrc_new(Class {
            name: Rc::clone(&self.name),
            members,
            methods,
            interfaces,
        });

        self.instances.insert(arguments.clone(), Rc::clone(&class));
        Ok(class)
    }
}

impl PartialEq for ClassPrototype {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Hash for ClassPrototype {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state)
    }
}

#[derive(Debug, Default)]
pub struct InterfacePrototype {
    pub name: Rc<String>,
    pub methods: IndexMap<Rc<String>, IFaceMethod>,
    pub generic_args: Vec<Rc<String>>,
    pub instances: HashMap<Vec<Type>, MutRc<Interface>>,
}

impl InterfacePrototype {
    pub fn build(&mut self, arguments: &Vec<Type>) -> Result<MutRc<Interface>, String> {
        if let Some(iface) = self.instances.get(arguments) {
            return Ok(Rc::clone(&iface));
        }

        check_generic_arguments(&self.generic_args, arguments)?;

        let methods = self.methods.iter().map(|(name, method)| {
            (
                Rc::clone(&name),
                IFaceMethod {
                    name: Rc::clone(&method.name),
                    parameters: method.parameters.iter().cloned().map(|p| replace_generic(p, arguments)).collect(),
                    ret_type: replace_generic(method.ret_type.clone(), arguments),
                    default_impl: method.default_impl.clone(),
                }
            )
        }).collect();

        let interface = mutrc_new(Interface {
            name: Rc::clone(&self.name),
            methods,
        });

        self.instances.insert(arguments.clone(), Rc::clone(&interface));
        Ok(interface)
    }
}

impl PartialEq for InterfacePrototype {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

/// A function.
#[derive(Debug, Default)]
pub struct FunctionPrototype {
    pub name: String,
    pub parameters: Vec<Rc<Variable>>,
    pub blocks: HashMap<Rc<String>, Block>,
    pub variables: HashMap<Rc<String>, Rc<Variable>>,
    pub ret_type: Type,
    pub generic_args: Vec<Rc<String>>,
    pub instances: HashMap<Vec<Type>, MutRc<Function>>,
}

impl FunctionPrototype {
    pub fn build(&mut self, arguments: &Vec<Type>) -> Result<MutRc<Function>, String> {
        if let Some(func) = self.instances.get(arguments) {
            return Ok(Rc::clone(&func));
        }

        check_generic_arguments(&self.generic_args, arguments)?;

        let name = format!("{}-{}", self.name, self.instances.len());
        let function = mutrc_new(Function {
            name,
            parameters: self.parameters.clone(), // TODO
            blocks: self.blocks.clone(),
            variables: self.variables.clone(), // TODO
            ret_type: replace_generic(self.ret_type.clone(), arguments),
        });

        self.instances.insert(arguments.clone(), Rc::clone(&function));
        Ok(function)
    }
}

/// Takes a type and list of generic types and replaces the type
/// should it be a generic one.
fn replace_generic(ty: Type, generics: &Vec<Type>) -> Type {
    if let Type::Generic(index) = ty {
        generics[index].clone()
    } else {
        ty
    }
}

fn check_generic_arguments(parameters: &Vec<Rc<String>>, arguments: &Vec<Type>) -> Result<(), String> {
    if parameters.len() != arguments.len() {
        return Err(format!(
            "Wrong amount of interface generic parameters (expected {}; got {})",
            parameters.len(),
            arguments.len()
        ));
    }
    Ok(())
}