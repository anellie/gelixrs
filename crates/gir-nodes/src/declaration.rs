use crate::{
    types::{ClosureType, TypeArguments, TypeKind, TypeParameters},
    Expr, Instance, Module, Type,
};
use common::{ModPath, MutRc};
use enum_methods::{EnumAsGetters, EnumIntoGetters, EnumIsA};
use indexmap::map::IndexMap;
use smol_str::SmolStr;
use std::{
    cell::RefCell,
    collections::HashMap,
    hash::{Hash, Hasher},
    rc::Rc,
};

/// A declaration is a top-level user-defined
/// item inside a module. This can be
/// either a function or an ADT;
/// interface implementations are an exception and
/// are instead attached to the implementor.
#[derive(Debug, Clone, EnumAsGetters, EnumIntoGetters)]
pub enum Declaration {
    Function(MutRc<Function>),
    Adt(MutRc<ADT>),
}

impl Declaration {
    /// Returns the corresponding type for this declaration
    /// with no type arguments. Not sound for use in
    /// generated code due to this!
    pub fn to_type(&self) -> Type {
        match self {
            Self::Function(f) => Type::Function(Instance::new_(Rc::clone(f))),
            Self::Adt(a) => Type::Adt(Instance::new_(Rc::clone(a))),
        }
    }

    /// Returns the type parameters on the declaration.
    pub fn type_parameters(&self) -> Rc<TypeParameters> {
        match self {
            Self::Function(f) => Rc::clone(&f.borrow().type_parameters),
            Self::Adt(a) => Rc::clone(&a.borrow().type_parameters),
        }
    }

    pub fn visible(&self, from: &ModPath) -> bool {
        match self {
            Self::Function(f) => f.borrow().visible(from),
            Self::Adt(a) => a.borrow().visible(from),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Visibility {
    Private = 0,
    Module = 1,
    Public = 2,
}

impl Visibility {
    /// Is this visibility visible from the given module?
    pub fn from(&self, own: &ModPath, from: &ModPath) -> bool {
        match self {
            Visibility::Private => from.is(&own.parts()),
            Visibility::Module => from.parts()[0] == own.parts()[0],
            Visibility::Public => true,
        }
    }
}

/// A general purpose struct used for all user-defined data structures.
/// The ty field inside is used for further specialization.
pub struct ADT {
    /// The name of the ADT.
    pub name: SmolStr,
    /// All fields on the ADT.
    pub fields: IndexMap<SmolStr, Rc<Field>>,
    /// The visibility of this ADT, determining its ability to be imported.
    pub visibility: Visibility,
    /// The type kind of this ADT, default to Ref, Value with `value` modifier
    pub type_kind: TypeKind,

    /// All methods of this ADT.
    /// Some ADTs have a few more special methods:
    /// - "new-instance(&ADT) -> &ADT": Initializes an empty allocation of the ADT with default members, called before constructor.
    /// - "free-wr(&ADT)": Frees a WR by decrementing the refcount of all fields
    /// - "free-sr(&ADT, act)": Frees a SR by decrementing the refcount of all fields and calling free if act == true
    pub methods: IndexMap<SmolStr, MutRc<Function>>,
    /// All constructors of the ADT, if any. They are simply methods
    /// with special constraints to enforce safety.
    pub constructors: Vec<MutRc<Function>>,

    /// Type parameters on this ADT, if any.
    pub type_parameters: Rc<TypeParameters>,

    /// The exact type of this ADT; used for holding specific info.
    pub ty: ADTType,
    /// The AST of this ADT
    pub ast: ast::Adt,
    /// The module this ADT was declared in
    pub module: MutRc<Module>,
    /// IR-level information of this ADT
    pub ir: IRAdt,
}

impl ADT {
    pub fn is_ptr(&self) -> bool {
        self.type_kind == TypeKind::Reference && !matches!(self.ty, ADTType::Interface)
    }

    pub fn refcounted(&self) -> bool {
        self.is_ptr()
    }

    pub fn visible(&self, from: &ModPath) -> bool {
        self.visibility.from(&self.module.borrow().path, from)
    }

    pub fn get_singleton_inst(inst: &MutRc<ADT>, args: &Rc<TypeArguments>) -> Option<Expr> {
        if let ADTType::EnumCase { ty, .. } = &inst.borrow().ty {
            if *ty == CaseType::Simple {
                Some(Expr::Allocate {
                    ty: Type::Adt(Instance::new(Rc::clone(inst), Rc::clone(args))),
                    constructor: Rc::clone(&inst.borrow().constructors[0]),
                    args: vec![],
                })
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// The exact type of ADT.
/// Can also contain type-specific data.
#[derive(Debug, Clone, EnumIsA)]
pub enum ADTType {
    /// A class definition.
    Class {
        // If this class is external (see gelix docs for more info)
        external: bool,
    },

    /// An interface definition.
    Interface,

    /// An enum, with unknown case.
    Enum {
        /// All cases.
        cases: Rc<HashMap<SmolStr, MutRc<ADT>>>,
    },

    /// An enum with known case.
    EnumCase { parent: MutRc<ADT>, ty: CaseType },
}

impl ADTType {
    /// Returns the cases of an enum type.
    /// Use on any other type will result in a panic.
    pub fn cases(&self) -> &HashMap<SmolStr, MutRc<ADT>> {
        if let ADTType::Enum { cases } = self {
            cases
        } else {
            unreachable!();
        }
    }

    /// Is this an extern class?
    pub fn is_extern_class(&self) -> bool {
        match self {
            ADTType::Class { external } => *external,
            _ => false,
        }
    }

    /// Does this type allow the default constructor with no arguments?
    pub fn allow_default_constructor(&self) -> bool {
        matches!(self, ADTType::Class {.. } | ADTType::EnumCase { .. })
    }
}

/// Kind of an enum case.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CaseType {
    /// Simply an identifier, 'simple' java-style case
    Simple,
    /// A 'data case', a la `Name(val a: String, val b: i64)`
    Data,
    /// A full case that behaves like a regular ADT.
    Adt,
}

/// Field on an ADT.
#[derive(Debug)]
pub struct Field {
    /// The name of the field.
    pub name: SmolStr,
    /// The visibility of the field.
    pub visibility: Visibility,
    /// If this field is mutable by user code. ("val" vs "var")
    pub mutable: bool,
    /// The type of this field, either specified or inferred by initializer
    pub ty: Type,
    /// The initializer for this field, if any.
    pub initializer: RefCell<Option<Expr>>,
    /// If this type had an initializer (it is captured and removed by the generator, so this is needed)
    pub initialized: bool,
    /// The index of the field inside the ADT.
    pub index: usize,
}

impl PartialEq for Field {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for Field {}

impl Hash for Field {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state)
    }
}

/// A function.
pub struct Function {
    /// The name of the function, with its module before it ($mod:$func)
    /// The only functions with no name change are external functions
    pub name: SmolStr,
    /// The visibility of the function.
    pub visibility: Visibility,
    /// All parameters needed to call this function.
    pub parameters: Vec<Rc<LocalVariable>>,
    /// If this function is variadic and accepts additional parameters.
    pub variadic: bool,
    /// Type parameters on this function, if any.
    pub type_parameters: Rc<TypeParameters>,
    /// A list of expressions that make up the func, executed in order.
    pub exprs: Vec<Expr>,
    /// All variables declared inside the function.
    pub variables: HashMap<SmolStr, Rc<LocalVariable>>,
    /// The return type of the function; Type::None if omitted.
    pub ret_type: Type,
    /// The AST for this function, if it is a user function
    /// and not compiler-generated.
    pub ast: Option<ast::Function>,
    /// The module this was declared in.
    pub module: MutRc<Module>,
    /// IR data for this function, used by IR generator
    pub ir: RefCell<IRFunction>,
}

impl Function {
    pub fn visible(&self, from: &ModPath) -> bool {
        self.visibility.from(&self.module.borrow().path, from)
    }

    /// Inserts a variable into the functions allocation table.
    /// Returns the name of it (should be used since a change can be needed due to colliding names).
    pub fn insert_var(&mut self, mut name: SmolStr, var: Rc<LocalVariable>) -> SmolStr {
        if self.variables.contains_key(&name) {
            name = SmolStr::new(format!("{}-{}", name, self.variables.len()));
        }
        self.variables.insert(name.clone(), var);
        name
    }

    /// Returns the corresponding closure type for this function.
    /// Will not include the first parameter containing captures.
    pub fn to_closure_type(&self) -> Type {
        Type::Closure(Rc::new(ClosureType {
            // Skip the first parameter, which is the parameter for captured variables.
            parameters: self
                .parameters
                .iter()
                .skip(1)
                .map(|p| p.ty.clone())
                .collect(),
            ret_type: self.ret_type.clone(),
            ..Default::default()
        }))
    }
}

/// A variable that can be loaded to produce a value by user code.
/// Can be either a global or local variable.
#[derive(Debug, Clone)]
pub enum Variable {
    /// This is a global function variable
    Function(Instance<Function>),
    /// This is a local function-scoped variable
    Local(Rc<LocalVariable>),
}

impl Variable {
    pub fn get_name(&self) -> SmolStr {
        match self {
            Self::Function(func) => func.ty.borrow().name.clone(),
            Self::Local(local) => local.name.clone(),
        }
    }

    /// Returns the type of the variable when loading it.
    pub fn get_type(&self) -> Type {
        match self {
            Self::Function(func) => Type::Function(func.clone()),
            Self::Local(local) => local.ty.clone(),
        }
    }

    /// Can this variable be assigned to?
    pub fn assignable(&self) -> bool {
        match self {
            Variable::Function(_) => false,
            Variable::Local(var) => var.mutable,
        }
    }
}

impl Hash for Variable {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Function(func) => func.ty.borrow().name.hash(state),
            Self::Local(local) => local.name.hash(state),
        }
    }
}

impl PartialEq for Variable {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Variable::Function(f), Variable::Function(o)) => f == o,
            (Variable::Local(f), Variable::Local(o)) => Rc::ptr_eq(f, o),
            _ => false,
        }
    }
}

impl Eq for Variable {}

/// A local variable scoped to a function, can be
/// function parameters or user-defined variables.
#[derive(Debug, Clone)]
pub struct LocalVariable {
    /// The name of the variable.
    pub name: SmolStr,
    /// Type of the variable.
    pub ty: Type,
    /// If it is mutable; user-decided on variables, false on fn arguments
    pub mutable: bool,
}

pub type IRFunction = gir_ir_adapter::IRFunction<TypeArguments>;
pub type IRAdt = gir_ir_adapter::IRAdt<TypeArguments>;
