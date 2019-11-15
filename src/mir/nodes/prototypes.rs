/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 11/15/19 4:47 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use either::Either;
use either::Either::{Left, Right};
use indexmap::IndexMap;

use crate::ast::Literal;
use crate::ir::PtrEqRc;
use crate::mir::{MutRc, mutrc_new};
use crate::mir::generator::MIRGenerator;
use crate::mir::nodes::{
    Block, Class, ClassMember, Expression, Flow, Function, IFaceMethod, Interface, Type, Variable,
};

#[derive(Debug, Default)]
pub struct ClassPrototype {
    pub name: Rc<String>,
    pub members: IndexMap<Rc<String>, Rc<ClassMember>>,
    pub methods: HashMap<Rc<String>, MutRc<FunctionPrototype>>,
    pub interfaces: IndexMap<Rc<String>, Either<MutRc<Interface>, MutRc<InterfacePrototype>>>,
    pub generic_args: Vec<Rc<String>>,
    pub instances: HashMap<Vec<Type>, MutRc<Class>>,
}

impl ClassPrototype {
    pub fn build(
        &mut self,
        gen: &mut MIRGenerator,
        mut arguments: Vec<Type>,
    ) -> Result<MutRc<Class>, String> {
        if let Some(class) = self.instances.get(&arguments) {
            return Ok(Rc::clone(&class));
        }

        let class_rc = mutrc_new(Class {
            name: Rc::clone(&self.name),
            ..Default::default()
        });
        let mut class = class_rc.borrow_mut();

        arguments.push(Type::Class(Rc::clone(&class_rc)));
        check_generic_arguments(&self.generic_args, &arguments)?;

        class.members.extend(self
            .members
            .iter()
            .map(|(name, member)| {
                let mut member = ClassMember::clone(member);
                member.type_ = replace_generic(member.type_, &arguments);
                (Rc::clone(name), Rc::new(member))
            }));
        drop(class);

        let methods = self
            .methods
            .iter()
            .map(|(name, method)| {
                (
                    Rc::clone(name),
                    method.borrow_mut().build(gen, &arguments).unwrap(),
                )
            })
            .collect();

        let interfaces = self
            .interfaces
            .iter()
            .map(|(name, iface)| {
                let iface = match iface {
                    Left(iface) => Ok(iface.clone()),
                    Right(proto) => proto.borrow_mut().build(&arguments),
                };
                iface.map(|iface| (Rc::clone(name), iface))
            })
            .collect::<Result<IndexMap<Rc<String>, MutRc<Interface>>, String>>()?;

        class_rc.borrow_mut().methods = methods;
        class_rc.borrow_mut().interfaces = interfaces;

        arguments.pop();
        self.instances.insert(arguments.clone(), Rc::clone(&class_rc));
        Ok(class_rc)
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

        let methods = self
            .methods
            .iter()
            .map(|(name, method)| {
                (
                    Rc::clone(&name),
                    IFaceMethod {
                        name: Rc::clone(&method.name),
                        parameters: method
                            .parameters
                            .iter()
                            .cloned()
                            .map(|p| replace_generic(p, arguments))
                            .collect(),
                        ret_type: replace_generic(method.ret_type.clone(), arguments),
                        default_impl: method.default_impl.clone(),
                    },
                )
            })
            .collect();

        let interface = mutrc_new(Interface {
            name: Rc::clone(&self.name),
            methods,
        });

        self.instances
            .insert(arguments.clone(), Rc::clone(&interface));
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
    pub instances: HashMap<Vec<Type>, Rc<Variable>>,
}

impl FunctionPrototype {
    pub fn build(
        &mut self,
        gen: &mut MIRGenerator,
        arguments: &Vec<Type>,
    ) -> Result<Rc<Variable>, String> {
        if let Some(func) = self.instances.get(arguments) {
            return Ok(Rc::clone(&func));
        }

        check_generic_arguments(&self.generic_args, arguments)?;

        let name = format!("{}-{}", self.name, self.instances.len());
        let function = Function {
            name: name.clone(),
            parameters: self.parameters.clone(),
            blocks: self.blocks.clone(),
            variables: self.variables.clone(),
            ret_type: replace_generic(self.ret_type.clone(), arguments),
        };
        let function = mutrc_new(remove_generic_variables(function, arguments));

        let global = Rc::new(Variable {
            name: Rc::new(name),
            type_: Type::Function(Rc::clone(&function)),
            mutable: false,
        });

        gen.builder
            .module
            .functions
            .insert(Rc::clone(&global.name), Rc::clone(&global));

        self.instances.insert(arguments.clone(), Rc::clone(&global));
        Ok(global)
    }

    // TODO: The 2 functions below are just copies of the same ones in mir::Function

    /// Appends a new block; will returns the block name.
    /// The name can be different than the given one when it was already in use.
    pub fn append_block(&mut self, name: &str) -> Rc<String> {
        let mut name = name.to_string();
        if self.blocks.contains_key(&name) {
            name = format!("{}-{}", name, self.blocks.len());
        }
        let rc = Rc::new(name);
        self.blocks.insert(Rc::clone(&rc), Vec::with_capacity(5));
        rc
    }

    /// Inserts a variable into the functions allocation table.
    /// Returns the name of it (should be used since a change can be needed due to colliding names).
    pub fn insert_var(&mut self, mut name: Rc<String>, var: Rc<Variable>) -> Rc<String> {
        if self.variables.contains_key(&name) {
            name = Rc::new(format!("{}-{}", name, self.variables.len()));
        }
        self.variables.insert(Rc::clone(&name), var);
        name
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

fn remove_generic_variables(mut function: Function, generics: &Vec<Type>) -> Function {
    function.ret_type = replace_generic(function.ret_type, generics);
    let function_vars = function
        .parameters
        .iter()
        .chain(function.variables.values());
    let variables_map: HashMap<PtrEqRc<Variable>, Rc<Variable>> = function_vars
        .map(|var| {
            let mut clone = Variable::clone(var);
            clone.type_ = replace_generic(clone.type_, generics);
            (PtrEqRc::new(var), Rc::new(clone))
        })
        .collect();

    function.parameters = function
        .parameters
        .iter()
        .map(|p| Rc::clone(&variables_map[&PtrEqRc::new(p)]))
        .collect();

    for block in function.blocks.values_mut() {
        for expr in block.iter_mut() {
            replace_variables(expr, &variables_map)
        }
    }

    function
}

fn replace_variables(expr: &mut Expression, var_map: &HashMap<PtrEqRc<Variable>, Rc<Variable>>) {
    match expr {
        Expression::Binary { left, right, .. } => {
            replace_variables(left, var_map);
            replace_variables(right, var_map);
        }

        Expression::Call { callee, arguments } => {
            replace_variables(callee, var_map);
            for arg in arguments {
                replace_variables(arg, var_map)
            }
        }

        Expression::Flow(flow) => match &mut **flow {
            Flow::Return(expr) => replace_variables(expr, var_map),
            Flow::Branch { condition, .. } => replace_variables(condition, var_map),

            Flow::Switch { cases, .. } => {
                for (expr, _) in cases.iter_mut() {
                    replace_variables(expr, var_map)
                }
            }

            Flow::None => (),
            Flow::Jump(_) => (),
        },

        Expression::Phi(nodes) => {
            for (node, _) in nodes {
                replace_variables(node, var_map)
            }
        }

        Expression::StructGet { object, .. } => replace_variables(object, var_map),

        Expression::StructSet { object, value, .. } => {
            replace_variables(object, var_map);
            replace_variables(value, var_map);
        }

        Expression::Literal(_) => (),

        Expression::Unary { right, .. } => replace_variables(right, var_map),

        Expression::VarGet(ref var) => {
            let var_rc = PtrEqRc::new(var);
            if let Some(var) = var_map.get(&var_rc) {
                std::mem::replace(expr, Expression::VarGet(Rc::clone(&var)));
            }
        }

        Expression::VarStore { ref var, value } => {
            let var_rc = PtrEqRc::new(var);
            if let Some(var) = var_map.get(&var_rc) {
                let mut owned_value =
                    std::mem::replace(value, Box::new(Expression::Literal(Literal::None)));
                replace_variables(&mut owned_value, var_map);

                std::mem::replace(
                    expr,
                    Expression::VarStore {
                        var: Rc::clone(&var),
                        value: owned_value,
                    },
                );
            }
        }
    }
}
