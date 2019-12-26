/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/26/19 8:59 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::HashMap;
use std::fmt::{Display, Error, Formatter};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast;
use crate::mir::generator::builder::Context;
use crate::mir::nodes::{ClosureType, Expr, Prototype, Type};
use crate::mir::{mutrc_new, MModule, MutRc};

/// A full class including all members and methods.
/// Members are ordered, as the class is represented as a struct in IR;
/// structs in IR only have indices for members, not names.
#[derive(Debug)]
pub struct Class {
    /// The name of the class.
    pub name: Rc<String>,
    /// All class members.
    pub members: IndexMap<Rc<String>, Rc<ClassMember>>,
    /// All class methods. Inserted as "doThing", not "Class-doThing".
    pub methods: HashMap<Rc<String>, Rc<Variable>>,
    /// An internal function that creates an instance of the class
    /// and populates all fields with a user-given default value.
    /// When the user wants to create an instance by calling a constructor,
    /// this function is called first, followed by one of the constructor methods.
    pub instantiator: Rc<Variable>,
    /// All constructors of the class. They are simply methods
    /// with special constraints to enforce safety.
    /// Only call on instances produced by the instantiator function.
    pub constructors: Vec<Rc<Variable>>,
    /// The context to be used inside this declaration.
    pub context: Context,
    /// The AST this was compiled from.
    pub ast: Rc<ast::Class>,
}

impl Class {
    pub fn from_ast(ast: ast::Class, context: Context) -> MutRc<Class> {
        mutrc_new(Class {
            name: Rc::clone(&ast.name.lexeme),
            members: IndexMap::with_capacity(ast.variables.len()),
            methods: HashMap::with_capacity(ast.methods.len()),
            instantiator: Rc::new(Default::default()),
            constructors: Vec::with_capacity(ast.constructors.len()),
            context,
            ast: Rc::new(ast),
        })
    }
}

impl PartialEq for Class {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Display for Class {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        writeln!(f, "class {} {{\n", self.name)?;
        for (name, member) in self.members.iter() {
            writeln!(
                f,
                "    {} {}: {}",
                if member.mutable { "var" } else { "val" },
                name,
                member.type_
            )?;
        }
        writeln!(f)?;
        for func in self.methods.values() {
            func.type_.as_function().borrow().display(f, "    ")?;
        }
        writeln!(f, "}}")
    }
}

impl Hash for Class {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state)
    }
}

/// A member of a class.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ClassMember {
    pub mutable: bool,
    pub type_: Type,
    /// This is the index used for StructGet and StructSet
    /// expressions. This is needed due to the IR only using indices for members
    pub index: u32,
    /// If this member has a default value set before constructors run.
    /// Used to determine if a constructor needs to initialize a member.
    pub has_default_value: bool,
}

/// An interface consisting of methods a type can implement.
#[derive(Debug)]
pub struct Interface {
    /// The name of the interface. Generally the name given by the user;
    /// should the interface have been created from a prototype with generic parameters,
    /// its name is $Name<$Param1,$Param2>
    pub name: Rc<String>,
    /// A map of all methods.
    /// Indexed due to the vtable in IR being a struct.
    pub methods: IndexMap<Rc<String>, IFaceMethod>,
    /// The prototype this interface was built from, if any.
    /// Only used for some intrinsics.
    pub proto: Option<Rc<Prototype>>,
    /// The context to be used inside this declaration.
    pub context: Context,
    /// The AST this was compiled from.
    pub ast: Rc<ast::Interface>,
}

impl Interface {
    pub fn from_ast(
        ast: ast::Interface,
        proto: Option<Rc<Prototype>>,
        context: Context,
    ) -> MutRc<Interface> {
        mutrc_new(Interface {
            name: Rc::clone(&ast.name.lexeme),
            methods: IndexMap::with_capacity(ast.methods.len()),
            proto,
            context,
            ast: Rc::new(ast),
        })
    }
}

impl Hash for Interface {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state)
    }
}

impl PartialEq for Interface {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Display for Interface {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        writeln!(f, "interface {} {{\n", self.name)?;
        for (_, func) in self.methods.iter() {
            writeln!(f, "    {}", func)?;
        }
        writeln!(f, "}}")
    }
}

/// An implementation of an interface.
#[derive(Debug)]
pub struct IFaceImpl {
    pub implementor: Type,
    pub iface: MutRc<Interface>,
    pub methods: IndexMap<Rc<String>, Rc<Variable>>,
    pub module: MutRc<MModule>,
    pub ast: Rc<ast::IFaceImpl>,
}

impl PartialEq for IFaceImpl {
    fn eq(&self, other: &Self) -> bool {
        self.implementor == other.implementor && Rc::ptr_eq(&self.iface, &other.iface)
    }
}

impl Eq for IFaceImpl {}

impl Hash for IFaceImpl {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.iface.borrow().hash(state)
    }
}

/// A struct representing all interfaces implemented by a type.
/// A simple map of interfaces is not enough, as it does not
/// prevent naming collisions.
#[derive(Debug)]
pub struct IFaceImpls {
    pub implementor: Type,
    /// Key is the implemented interface, value the impl.
    /// Key isn't an interface directly due to needed
    /// Hash and Eq traits that only [Type] implements.
    pub interfaces: HashMap<Type, IFaceImpl>,
    pub methods: HashMap<Rc<String>, Rc<Variable>>,
}

impl PartialEq for IFaceImpls {
    fn eq(&self, other: &Self) -> bool {
        self.implementor == other.implementor
    }
}

impl Hash for IFaceImpls {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.implementor.hash(state)
    }
}

/// A method inside an interface.
/// The default implementation is left in AST state so that it can be compiled
/// individually on concrete implementations if needed; it can be found
/// in the 'ast' field of the interface this function is contained in.
#[derive(Debug)]
pub struct IFaceMethod {
    pub name: Rc<String>,
    pub parameters: Vec<Type>,
    pub ret_type: Type,
    pub has_default_impl: bool,
}

impl Display for IFaceMethod {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "func {}(", self.name)?;

        let mut params = self.parameters.iter();
        params.next().map(|param| write!(f, "{}", param));
        for param in params {
            write!(f, ", {}", param)?;
        }

        writeln!(f, ")")
    }
}

/// A function.
#[derive(Debug)]
pub struct Function {
    /// The name of the function, with its module before it ($mod:$func)
    /// The only functions with no name change are external functions
    pub name: String,
    /// All parameters needed to call this function.
    pub parameters: Vec<Rc<Variable>>,
    /// All blocks of this function, which contain the expressions making up the func.
    pub blocks: HashMap<Rc<String>, Block>,
    /// All variables declared inside that need alloca in IR.
    pub variables: HashMap<Rc<String>, Rc<Variable>>,
    /// The return type of the function; Type::None if omitted.
    pub ret_type: Type,
    /// The context to be used inside this declaration.
    pub context: Context,
    /// The AST the function was compiled from.
    /// This is only present on functions that were
    /// compiled from AST functions;
    /// things like methods have None here.
    pub ast: Option<Rc<ast::Function>>,
}

impl Function {
    /// Either appends a new block or returns the one already present with the given name.
    /// When force_new is true, a new block is always created.
    /// Because of this, the name can be different than the given one when it was already in use.
    pub fn append_block(&mut self, name: &str, force_new: bool) -> Rc<String> {
        let mut name = name.to_string();
        if self.blocks.contains_key(&name) {
            if force_new {
                name = format!("{}-{}", name, self.blocks.len());
            } else {
                return Rc::new(name);
            }
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

    pub fn to_closure_type(&self) -> Type {
        Type::Closure(Rc::new(ClosureType {
            // Skip the first parameter, which is the parameter for captured variables.
            parameters: self
                .parameters
                .iter()
                .skip(1)
                .map(|p| p.type_.clone())
                .collect(),
            ret_type: self.ret_type.clone(),
        }))
    }

    fn display(&self, f: &mut Formatter, space: &'static str) -> Result<(), Error> {
        write!(f, "{}func {}(", space, self.name)?;

        let mut params = self.parameters.iter();
        params
            .next()
            .map(|param| write!(f, "{}: {}", param.name, param.type_));
        for param in params {
            write!(f, ", {}: {}", param.name, param.type_)?;
        }

        writeln!(f, ") {{")?;
        for (name, var) in &self.variables {
            writeln!(
                f,
                "{}{} {}: {}",
                space,
                if var.mutable { "var" } else { "val" },
                name,
                var.type_
            )?;
        }
        if self.variables.len() != 0 {
            writeln!(f)?;
        }
        for (name, block) in self.blocks.iter() {
            writeln!(f, "{}{}:", space, name)?;
            for inst in block.iter() {
                writeln!(f, "{}    {}", space, inst)?;
            }
        }
        writeln!(f, "{}}}", space)
    }
}

impl PartialEq for Function {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Hash for Function {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state)
    }
}

impl Display for Function {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.display(f, "")
    }
}

/// A block inside a function.
pub type Block = Vec<Expr>;

/// A variable. Used for function variables as well as for referencing functions.
#[derive(Debug, Default, Clone)]
pub struct Variable {
    pub mutable: bool,
    pub type_: Type,
    pub name: Rc<String>,
}

impl Variable {
    pub fn new(mutable: bool, type_: Type, name: &Rc<String>) -> Rc<Variable> {
        Rc::new(Variable {
            mutable,
            type_,
            name: Rc::clone(name),
        })
    }
}

impl Hash for Variable {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state)
    }
}
