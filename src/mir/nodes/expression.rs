/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 2/3/20 3:01 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{
    fmt::{Display, Error, Formatter},
    mem,
    rc::Rc,
};

use crate::{
    ast::{expression::LOGICAL_BINARY, Literal},
    lexer::token::TType,
    mir::{
        generator::intrinsics::INTRINSICS,
        nodes::{Class, ClassMember, Function, Interface, Type, Variable},
        MutRc,
    },
};
use either::Either::Right;
use std::cell::Cell;

/// All expressions in MIR. All of them produce a value.
/// Expressions are in blocks in functions. Gelix does not have statements.
///
/// When compiling MIR expressions into IR, they are not checked for
/// validity. Invalid/Illegal expressions will most likely result in a
/// crash or broken IR code.
#[derive(Debug, Clone)]
pub enum Expr {
    /// Create a class instance.
    /// This will perform the following steps in IR:
    /// - Allocate either an alloca or on the heap with malloc
    /// - Call the class instantiator with this new allocation
    /// - Call the given constructor with the allocation
    /// - Return the now initialized object as the value of this Expr
    AllocClassInst {
        class: MutRc<Class>,
        constructor: Rc<Variable>,
        constructor_args: Vec<Expr>,
        heap: Cell<bool>,
    },

    /// Simply a binary operation between numbers.
    Binary {
        left: Box<Expr>,
        operator: TType,
        right: Box<Expr>,
    },

    /// A static function call.
    /// callee can be both a function or a closure.
    Call {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },

    /// A dynamic function call, where the callee is an interface method.
    /// The index is of the function to be called in the iface's method/vtable field.
    /// Implemented in IR as a struct with pointers to implementor and vtable (fat ptr).
    /// The value/fat ptr is obtained from the arguments list.
    CallDyn {
        callee: MutRc<Interface>,
        index: usize,
        arguments: Vec<Expr>,
    },

    /// A cast to an interface.
    /// Will create a temp alloca in IR to hold the interface struct.
    CastToInterface { object: Box<Expr>, to: Type },

    /// Construct a closure from the given function along with the captured
    /// variables. The function must have an additional first parameter
    /// for all captured variables; similarly to the 'this' param on methods.
    ConstructClosure {
        function: MutRc<Function>,
        global: Rc<Variable>,
        captured: Rc<Vec<Rc<Variable>>>,
    },

    /// A 'flow' expression, which changes control flow. See [Flow] enum
    Flow(Box<Flow>),

    /// Will call libc free on the inner expression.
    Free(Box<Expr>),

    /// Modifies the refcount on a value, either
    /// incrementing or decrementing it.
    /// It returns the value - it essentially wraps it
    ModifyRefCount { object: Box<Expr>, dec: bool },

    /// A Phi node. Returns a different value based on
    /// which block the current block was reached from.
    Phi(Vec<(Expr, Rc<String>)>),

    /// Pop all current locals off the stack.
    /// See comments on the IR generator for more info.
    PopLocals,

    /// Same as PopLocals, but will evalutate the given expression
    /// beforehand and 'lift' the expression into the outer locals
    /// (See ir/gen_expr.rs for the actual code, which
    /// should be enough to explain this.)
    PopLocalsWithReturn(Box<Expr>),

    /// Push a new empty vector onto the locals stack.
    /// See comments on the IR generator for more info.
    PushLocals,

    /// Gets a member of a class struct.
    StructGet { object: Box<Expr>, index: usize },

    /// Sets a member of a class struct.
    StructSet {
        object: Box<Expr>,
        index: usize,
        value: Box<Expr>,
        first_set: bool,
    },

    /// Simply produces the literal as value.
    Literal(Literal),

    /// A unary expression on numbers.
    Unary { operator: TType, right: Box<Expr> },

    /// Returns a variable.
    VarGet(Rc<Variable>),

    /// Stores a value inside a variable.
    VarStore {
        var: Rc<Variable>,
        value: Box<Expr>,
        first_store: bool,
    },
}

impl Expr {
    pub fn alloc_class(
        class: &MutRc<Class>,
        constructor: &Rc<Variable>,
        constructor_args: Vec<Expr>,
    ) -> Expr {
        Expr::AllocClassInst {
            class: Rc::clone(&class),
            constructor: Rc::clone(&constructor),
            constructor_args,
            heap: Cell::new(true),
        }
    }

    pub fn binary(left: Expr, operator: TType, right: Expr) -> Expr {
        Expr::Binary {
            left: Box::new(left),
            operator,
            right: Box::new(right),
        }
    }

    pub fn cast(obj: Expr, ty: &Type) -> Expr {
        Expr::CastToInterface {
            object: Box::new(obj),
            to: ty.clone(),
        }
    }

    pub fn construct_closure(global: &Rc<Variable>, captured: Rc<Vec<Rc<Variable>>>) -> Expr {
        Expr::ConstructClosure {
            function: Rc::clone(global.type_.as_function()),
            global: Rc::clone(global),
            captured,
        }
    }

    pub fn unary(right: Expr, op: TType) -> Expr {
        Expr::Unary {
            operator: op,
            right: Box::new(right),
        }
    }

    pub fn call(callee: Expr, arguments: Vec<Expr>) -> Expr {
        Expr::Call {
            callee: Box::new(callee),
            arguments,
        }
    }

    pub fn call_dyn(callee: &MutRc<Interface>, index: usize, arguments: Vec<Expr>) -> Expr {
        Expr::CallDyn {
            callee: Rc::clone(callee),
            index,
            arguments,
        }
    }

    pub fn phi(nodes: Vec<(Expr, Rc<String>)>) -> Expr {
        // Filter all nodes that return Any.
        // A node might return Any if it does not produce a value;
        // but instead branches away from the phi.
        let filtered_nodes = nodes
            .into_iter()
            .filter(|node| {
                let type_ = node.0.get_type();
                mem::discriminant(&Type::Any) != mem::discriminant(&type_)
            })
            .collect();

        Expr::Phi(filtered_nodes)
    }

    pub fn struct_get(object: Expr, field: &Rc<ClassMember>) -> Expr {
        Expr::StructGet {
            object: Box::new(object),
            index: field.index,
        }
    }

    pub fn struct_set(object: Expr, field: Rc<ClassMember>, value: Expr) -> Expr {
        Expr::StructSet {
            object: Box::new(object),
            index: field.index,
            value: Box::new(value),
            first_set: false,
        }
    }

    pub fn struct_set_init(
        object: Expr,
        field: Rc<ClassMember>,
        value: Expr,
        first_set: bool,
    ) -> Expr {
        Expr::StructSet {
            object: Box::new(object),
            index: field.index,
            value: Box::new(value),
            first_set,
        }
    }

    pub fn struct_set_index(object: Expr, index: usize, value: Expr) -> Expr {
        Expr::StructSet {
            object: Box::new(object),
            index,
            value: Box::new(value),
            first_set: true,
        }
    }

    pub fn store(var: &Rc<Variable>, value: Expr) -> Expr {
        Expr::VarStore {
            var: Rc::clone(var),
            value: Box::new(value),
            first_store: false,
        }
    }

    pub fn store_init(var: &Rc<Variable>, value: Expr) -> Expr {
        Expr::VarStore {
            var: Rc::clone(var),
            value: Box::new(value),
            first_store: true,
        }
    }

    pub fn load(var: &Rc<Variable>) -> Expr {
        Expr::VarGet(Rc::clone(var))
    }

    pub fn branch(cond: Expr, then: &Rc<String>, else_: &Rc<String>) -> Expr {
        Expr::Flow(Box::new(Flow::Branch {
            condition: cond,
            then_b: Rc::clone(&then),
            else_b: Rc::clone(&else_),
        }))
    }

    pub fn jump(to: &Rc<String>) -> Expr {
        Expr::Flow(Box::new(Flow::Jump(Rc::clone(to))))
    }

    pub fn ret(val: Expr) -> Expr {
        Expr::Flow(Box::new(Flow::Return(val)))
    }

    pub fn mod_rc(val: Expr, dec: bool) -> Expr {
        Expr::ModifyRefCount {
            object: Box::new(val),
            dec,
        }
    }

    pub fn any_const() -> Expr {
        Expr::Literal(Literal::Any)
    }

    pub fn none_const() -> Expr {
        Expr::Literal(Literal::None)
    }

    /// Returns the type of this MIRExpression.
    /// Note that this function does not do type validation, and calling this function
    /// on malformed expressions is undefined behavior that can lead to panics.
    pub fn get_type(&self) -> Type {
        match self {
            Expr::AllocClassInst { class, .. } => Type::Class(Rc::clone(class)),

            Expr::Binary { right, operator, .. } | Expr::Unary { operator, right } => {
                if LOGICAL_BINARY.contains(&operator) {
                    Type::Bool
                } else {
                    right.get_type()
                }
            }

            Expr::Call { callee, .. } => match callee.get_type() {
                Type::Function(func) => func.borrow().ret_type.clone(),
                Type::Closure(closure) => closure.ret_type.clone(),
                _ => panic!("Invalid callee"),
            },

            Expr::CallDyn { callee, index, .. } => callee
                .borrow()
                .methods
                .get_index(*index)
                .unwrap()
                .1
                .ret_type
                .clone(),

            Expr::CastToInterface { to, .. } => to.clone(),

            Expr::ConstructClosure { function, .. } => function.borrow().to_closure_type(),

            Expr::ModifyRefCount { object, .. } => object.get_type(),

            Expr::Phi(branches) => branches.first().unwrap().0.get_type(),

            Expr::PopLocalsWithReturn(expr) => expr.get_type(),

            Expr::StructGet { object, index } | Expr::StructSet { object, index, .. } => {
                let object = object.get_type();
                if let Type::Class(class) = object {
                    class
                        .borrow()
                        .members
                        .iter()
                        .find(|(_, mem)| mem.index == *index)
                        .unwrap()
                        .1
                        .type_
                        .clone()
                } else {
                    panic!("non-class struct get")
                }
            },

            Expr::Literal(literal) => match literal {
                Literal::Any => Type::Any,
                Literal::None => Type::None,
                Literal::Bool(_) => Type::Bool,
                Literal::I8(_) => Type::I8,
                Literal::I16(_) => Type::I16,
                Literal::I32(_) => Type::I32,
                Literal::I64(_) => Type::I64,
                Literal::F32(_) => Type::F32,
                Literal::F64(_) => Type::F64,
                Literal::String(_) => INTRINSICS.with(|i| i.borrow().string_type.clone().unwrap()),
                Literal::Array(Right(arr)) => INTRINSICS
                    .with(|i| i.borrow().get_array_type(arr.type_.clone(), None))
                    .ok()
                    .unwrap(),
                _ => panic!("unknown literal"),
            },

            Expr::VarGet(var) | Expr::VarStore { var, .. } => var.type_.clone(),

            Expr::Flow(_) | Expr::PushLocals | Expr::PopLocals | Expr::Free(_) => Type::None,
        }
    }
}

impl Display for Expr {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            Expr::AllocClassInst { class, heap, .. } => {
                write!(f, "alloc {} (heap: {})", class.borrow().name, heap.get())
            }

            Expr::Binary {
                left,
                operator,
                right,
            } => write!(f, "({}) {:?} ({})", left, operator, right),

            Expr::Call { callee, arguments } => {
                write!(f, "call {}", callee)?;
                if !arguments.is_empty() {
                    write!(f, " with ")?;
                }
                for arg in arguments.iter() {
                    write!(f, "({}) ", arg)?;
                }
                Ok(())
            }

            Expr::CallDyn {
                callee,
                index,
                arguments,
            } => {
                let method_name = Rc::clone(callee.borrow().methods.get_index(*index).unwrap().0);
                write!(f, "call method {}", method_name)?;
                write!(f, " with ")?;
                for arg in arguments.iter() {
                    write!(f, "({}) ", arg)?;
                }
                Ok(())
            }

            Expr::CastToInterface { object, to, .. } => write!(f, "cast {} to {}", object, to),

            Expr::ConstructClosure {
                global, captured, ..
            } => {
                write!(f, "closure fn({}) capture", global.name)?;
                for capture in captured.iter() {
                    write!(f, " {}", capture.name)?;
                }
                Ok(())
            }

            Expr::Flow(flow) => write!(f, "{}", flow),

            Expr::Free(expr) => write!(f, "free({})", expr),

            Expr::ModifyRefCount { object, dec } => write!(f, "rc+{} on {}", !dec, object),

            Expr::Phi(nodes) => {
                write!(f, "phi {{ ")?;
                for (expr, block) in nodes.iter() {
                    write!(f, "{}: ({}), ", block, expr)?;
                }
                write!(f, "}}")
            }

            Expr::PopLocals => write!(f, "pop context"),

            Expr::PopLocalsWithReturn(expr) => write!(f, "pop context with ({})", expr),

            Expr::PushLocals => write!(f, "push context"),

            Expr::StructGet { object, index } => write!(f, "get {} from ({})", index, object),

            Expr::StructSet {
                object,
                index,
                value,
                ..
            } => write!(f, "set {} of ({}) to ({})", index, object, value),

            Expr::Literal(literal) => write!(f, "{}", literal),

            Expr::Unary { right, .. } => write!(f, "neg ({})", right),

            Expr::VarGet(var) => write!(f, "{}", var.name),

            Expr::VarStore { var, value, .. } => write!(f, "store ({}) in {}", value, var.name),
        }
    }
}

/// An 'expression' that always yields None, and changes control flow.
#[derive(Clone, Debug)]
pub enum Flow {
    /// Return void from function
    None,

    /// Return a value from function
    Return(Expr),

    /// Jump to another block
    Jump(Rc<String>),

    /// Jump to another block conditionally
    Branch {
        condition: Expr,
        then_b: Rc<String>,
        else_b: Rc<String>,
    },

    /// Same as branch, but with a list of conditions.
    /// Jumps to the first that matches.
    Switch {
        cases: Vec<(Expr, Rc<String>)>,
        default: Rc<String>,
    },
}

impl Display for Flow {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            Flow::None => write!(f, "return"),

            Flow::Return(expr) => write!(f, "return ({})", expr),

            Flow::Jump(goal) => write!(f, "jump {}", goal),

            Flow::Branch {
                condition,
                then_b,
                else_b,
            } => write!(f, "jump {} if ({}) else {}", then_b, condition, else_b),

            Flow::Switch { cases, default } => {
                write!(f, "switch {{ ")?;
                for (expr, block) in cases.iter() {
                    write!(f, "{}: ({}), ", block, expr)?;
                }
                write!(f, "else {}", default)?;
                write!(f, "}}")
            }
        }
    }
}

/// An array literal in MIR. See ast/literal.rs for usage.
#[derive(Debug, Clone)]
pub struct ArrayLiteral {
    pub values: Vec<Expr>,
    pub type_: Type,
}
