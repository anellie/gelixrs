/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/24/19 4:12 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{Display, Error, Formatter};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast::expression::{Expression as ASTExpr, LOGICAL_BINARY};
use crate::ast::literal::Literal;
use crate::lexer::token::TType;
use crate::mir::MutRc;

/// All types in Gelix.
#[derive(Debug, Clone)]
pub enum Type {
    Any,
    None,
    Bool,

    I8,
    I16,
    I32,
    I64,

    F32,
    F64,

    String,

    Array(Box<Type>),

    Function(MutRc<Function>),

    Class(MutRc<Class>),

    Interface(MutRc<Interface>),

    /// A generic type. Only found in interface methods.
    /// Appearing anywhere else is undefined behavior; panicking is appropriate in that case.
    Generic(Rc<String>),
}

impl Type {
    /// Is this type an integer?
    pub fn is_int(&self) -> bool {
        match self {
            Type::I8 | Type::I16 | Type::I32 | Type::I64 => true,
            _ => false
        }
    }

    /// Is this type a floating-point number?
    pub fn is_float(&self) -> bool {
        match self {
            Type::F32 | Type::F64 => true,
            _ => false
        }
    }
}

impl PartialEq for Type {
    fn eq(&self, o: &Self) -> bool {
        if let Type::Any = o {
            return true;
        }

        // TODO: Is there really no other way than whatever the heck this is?
        match self {
            Type::Function(f) => {
                if let Type::Function(o) = o { f == o } else { false }
            }

            Type::Class(c) => {
                if let Type::Class(o) = o { c == o } else { false }
            }

            Type::Interface(i) => {
                if let Type::Interface(o) = o { i == o } else { false }
            }

            Type::Array(a) => {
                if let Type::Array(o) = o { a == o } else { false }
            }

            Type::Generic(g) => {
                if let Type::Generic(o) = o { g == o } else { false }
            }

            Type::Any => true,

            _ => std::mem::discriminant(self) == std::mem::discriminant(o),
        }
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            Type::Array(arr) => write!(f, "[{}]", arr),
            Type::Function(func) => write!(f, "<func {}>", func.borrow().name),
            Type::Class(class) => write!(f, "{}", class.borrow().name),
            Type::Interface(iface) => write!(f, "{}", iface.borrow().name),
            _ => write!(f, "{:?}", self),
        }
    }
}

#[derive(Debug)]
pub struct Class {
    pub name: Rc<String>,
    /// All class members.
    pub members: IndexMap<Rc<String>, Rc<ClassMember>>,
    /// All class methods. Inserted as "doThing", not "Name-doThing".
    pub methods: HashMap<Rc<String>, Rc<Variable>>,
    /// All interfaces implemented by this class.
    pub interfaces: IndexMap<Rc<String>, MutRc<Interface>>,
}

impl PartialEq for Class {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Display for Class {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "class {} {{\n", self.name)?;
        for (name, member) in self.members.iter() {
            write!(f, "    {} {}: {}\n", if member.mutable { "var" } else { "val" }, name, member.type_)?;
        }
        write!(f, "}}\n")
    }
}

/// A member of a class.
#[derive(Debug)]
pub struct ClassMember {
    pub mutable: bool,
    pub type_: Type,
    pub index: u32,
}

/// An abstract interface defining all its methods.
#[derive(Debug)]
pub struct Interface {
    pub name: Rc<String>,
    /// A map of all methods.
    pub methods: IndexMap<Rc<String>, IFaceMethod>,
    /// All generic parameters that are type-aliased on concrete implementations.
    pub generics: Vec<Rc<String>>,
}

impl PartialEq for Interface {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

/// A method inside an interface.
/// The default implementation is left in AST state so that it can be compiled
/// individually on concrete implementations if needed.
#[derive(Debug)]
pub struct IFaceMethod {
    pub name: Rc<String>,
    pub parameters: Vec<Type>,
    pub ret_type: Type,
    pub default_impl: Option<ASTExpr>,
}

/// A function in MIR. Consists of blocks.
#[derive(Debug)]
pub struct Function {
    pub name: Rc<String>,
    pub parameters: Vec<Rc<Variable>>,
    pub blocks: HashMap<Rc<String>, Block>,
    pub variables: HashMap<Rc<String>, Rc<Variable>>,
    pub ret_type: Type,
}

impl PartialEq for Function {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Display for Function {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "func {}(", self.name)?;

        let mut params = self.parameters.iter();
        params.next().map(|param| write!(f, "{}: {}", param.name, param.type_));
        for param in params {
            write!(f, ", {}: {}", param.name, param.type_)?;
        }

        write!(f, ") {{\n")?;
        for (name, block) in self.blocks.iter() {
            write!(f, "{}:\n", name)?;
            for inst in block.expressions.iter() {
                write!(f, "    {}\n", inst)?;
            }
            write!(f, "    {}\n\n", block.last)?;
        }
        write!(f, "}}\n")
    }
}

impl Function {
    /// Appends a new block; will returns the block name.
    /// The name can be different than the given one when it was already in use.
    pub fn append_block(&mut self, name: &str) -> Rc<String> {
        let mut name = name.to_string();
        if self.blocks.contains_key(&name) {
            name = format!("{}-{}", name, self.blocks.len());
        }
        let rc = Rc::new(name);
        self.blocks.insert(
            Rc::clone(&rc),
            Block {
                expressions: Vec::with_capacity(5),
                last: Flow::None,
            },
        );
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

/// A variable inside a function.
#[derive(Debug, Clone)]
pub struct Variable {
    pub mutable: bool,
    pub type_: Type,
    pub name: Rc<String>,
}

impl Hash for Variable {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state)
    }
}

/// A block inside a function.
#[derive(Debug)]
pub struct Block {
    pub expressions: Vec<Expression>,
    pub last: Flow,
}

/// The last part of a block.
#[derive(Debug)]
pub enum Flow {
    /// Return void
    None,

    /// Jump to another block
    Jump(Rc<String>),

    /// Jump to another block conditionally
    Branch {
        condition: Expression,
        then_b: Rc<String>,
        else_b: Rc<String>,
    },

    /// Same as branch, but with a list of conditions.
    /// Jumps to the first that matches.
    Switch {
        cases: Vec<(Expression, Rc<String>)>,
        default: Rc<String>,
    },

    /// Return a value
    Return(Expression),
}

impl Display for Flow {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            Flow::None => write!(f, "return"),

            Flow::Jump(goal) => write!(f, "jump {}", goal),

            Flow::Branch { condition, then_b, else_b } => write!(f, "jump {} if ({}) else {}", then_b, condition, else_b),

            Flow::Switch { cases, default } => {
                write!(f, "switch {{ ")?;
                for (expr, block) in cases.iter() {
                    write!(f, "{}: ({}), ", block, expr)?;
                }
                write!(f, "else {}", default)?;
                write!(f, "}}")
            },

            Flow::Return(expr) => write!(f, "return ({})", expr),
        }
    }
}

/// All expressions in MIR. All of them produce a value.
#[derive(Debug, Clone)]
pub enum Expression {
    /// Simply a binary operation between numbers.
    Binary {
        left: Box<Expression>,
        operator: TType,
        right: Box<Expression>,
    },

    /// Casts a class to another class type.
    Bitcast {
        object: Box<Expression>,
        goal: MutRc<Class>,
    },

    /// A function call.
    Call {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
    },

    /// An expression indicating that any code after it should be discarded,
    /// and that the next statement in the block should be the terminator ([MIRFlow]).
    DoRet,

    /// Simply produces the function as a value.
    Function(MutRc<Function>),

    /// A Phi node. Returns a different value based on
    /// which block the current block was reached from.
    Phi(Vec<(Expression, Rc<String>)>),

    /// Gets a member of a class struct.
    StructGet {
        object: Box<Expression>,
        index: u32,
    },

    /// Sets a member of a class struct.
    StructSet {
        object: Box<Expression>,
        index: u32,
        value: Box<Expression>,
    },

    /// Simply produces the literal as value.
    Literal(Literal),

    /// A unary expression on numbers.
    Unary {
        operator: TType,
        right: Box<Expression>,
    },

    /// Returns a variable.
    VarGet(Rc<Variable>),

    /// Stores a value inside a variable.
    VarStore {
        var: Rc<Variable>,
        value: Box<Expression>,
    },
}

impl Expression {
    /// Returns the type of this MIRExpression.
    /// Note that this function does not do type validation, and calling this function
    /// on malformed expressions is undefined behavior that can lead to panics.
    pub(super) fn get_type(&self) -> Type {
        match self {
            Expression::Binary { left, operator, .. } => {
                if LOGICAL_BINARY.contains(&operator) {
                    Type::Bool
                } else {
                    left.get_type()
                }
            }

            Expression::Bitcast { goal, .. } => Type::Class(Rc::clone(goal)),

            Expression::Call { callee, .. } => {
                if let Type::Function(func) = callee.get_type() {
                    RefCell::borrow(&func).ret_type.clone()
                } else {
                    panic!("non-function call type")
                }
            }

            Expression::DoRet => Type::None,

            Expression::Function(func) => Type::Function(func.clone()),

            Expression::Phi(branches) => branches.first().unwrap().0.get_type(),

            Expression::StructGet { object, index } => {
                Expression::type_from_struct_get(object, *index)
            }

            Expression::StructSet { object, index, .. } => {
                Expression::type_from_struct_get(object, *index)
            }

            Expression::Literal(literal) => match literal {
                Literal::Any => Type::Any,
                Literal::None => Type::None,
                Literal::Bool(_) => Type::Bool,
                Literal::I8(_) => Type::I8,
                Literal::I16(_) => Type::I16,
                Literal::I32(_) => Type::I32,
                Literal::I64(_) => Type::I64,
                Literal::F32(_) => Type::F32,
                Literal::F64(_) => Type::F64,
                Literal::String(_) => Type::String,
                Literal::Array(arr) => {
                    Type::Array(Box::new(arr.as_ref().right().unwrap().type_.clone()))
                }
                _ => panic!("unknown literal"),
            },

            Expression::Unary { operator, right } => match operator {
                TType::Bang => Type::Bool,
                TType::Minus => right.get_type(),
                _ => panic!("invalid unary"),
            },

            Expression::VarGet(var) => var.type_.clone(),

            Expression::VarStore { var, .. } => var.type_.clone(),
        }
    }

    /// Returns the type of a struct member.
    fn type_from_struct_get(object: &Expression, index: u32) -> Type {
        let object = object.get_type();
        if let Type::Class(class) = object {
            class
                .borrow()
                .members
                .iter()
                .find(|(_, mem)| mem.index == index)
                .unwrap()
                .1
                .type_
                .clone()
        } else {
            panic!("non-class struct get")
        }
    }
}

impl Display for Expression {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            Expression::Binary { left, operator, right } => write!(f, "({}) {:?} ({})", left, operator, right),

            Expression::Bitcast { object, goal } => write!(f, "cast ({}) to {}", object, goal.borrow().name),

            Expression::Call { callee, arguments } => {
                write!(f, "call ({}) with ", callee)?;
                for arg in arguments.iter() {
                    write!(f, "({})", arg)?;
                }
                Ok(())
            },

            Expression::DoRet => write!(f, "endblock"),

            Expression::Function(func) => write!(f, "{}", func.borrow().name),

            Expression::Phi(nodes) => {
                write!(f, "phi {{ ")?;
                for (expr, block) in nodes.iter() {
                    write!(f, "{}: ({}), ", block, expr)?;
                }
                write!(f, "}}")
            },

            Expression::StructGet { object, index } => write!(f, "get {} from ({})", index, object),

            Expression::StructSet { object, index, value } => write!(f, "set {} of ({}) to ({})", index, object, value),

            Expression::Literal(literal) => write!(f, "{}", literal),

            Expression::Unary { right, .. } => write!(f, "neg ({})", right),

            Expression::VarGet(var) => write!(f, "load {}", var.name),

            Expression::VarStore { var, value } => write!(f, "store ({}) in {}", value, var.name),
        }
    }
}

/// An array literal in MIR. See ast/literal.rs for usage.
#[derive(Debug, Clone)]
pub struct MIRArray {
    pub values: Vec<Expression>,
    pub type_: Type,
}
