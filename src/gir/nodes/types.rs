use std::{
    collections::HashMap,
    fmt,
    fmt::{Display, Formatter},
    hash::{Hash, Hasher},
    rc::Rc,
};

use crate::{
    ast,
    error::Res,
    gir::{
        generator::resolver::Resolver,
        get_or_create_iface_impls,
        nodes::{
            declaration::{ADTType, Function, LocalVariable, ADT},
            expression::CastType,
            module::Module,
        },
        MutRc,
    },
    ir::adapter::IRClosure,
    lexer::token::Token,
};
use smol_str::SmolStr;
use std::cell::Cell;

pub type TypeArguments = Vec<Type>;
pub type TypeParameters = Vec<TypeParameter>;

/// A type in GIR.
/// This *can* include type arguments for declarations,
/// but does not have to - types can be unresolved.
/// Type parameters are a separate type, monomorthised later in GIR.
#[derive(Debug, Clone, EnumAsGetters, EnumIsA, EnumIntoGetters)]
pub enum Type {
    /// Any type that can cast to anything; used by
    /// control flow branching away to allow phi usage with them
    Any,
    /// None singleton type used for expressions that do not produce a value
    None,
    /// Simple boolean/i1 type.
    Bool,

    /// Signed integer types from 8 to 64 bit width.
    I8,
    I16,
    I32,
    I64,

    /// Unsigned integer types from 8 to 64 bit width.
    U8,
    U16,
    U32,
    U64,

    /// Floating-point numbers with 32 and 64 bit width.
    F32,
    F64,

    /// A function instance. This is a function itself, not a signature.
    Function(Instance<Function>),
    /// A closure signature.
    Closure(Rc<ClosureType>),
    /// The first parameter on a closure function
    ClosureCaptured(Rc<Vec<Rc<LocalVariable>>>),

    /// An ADT used as a value.
    Value(Instance<ADT>),
    /// A strong ADT reference.
    StrongRef(Instance<ADT>),
    /// A weak ADT reference.
    WeakRef(Instance<ADT>),

    /// A raw pointer of a type.
    /// Can only be interacted with using special
    /// intrinsic functions; here for FFI and unsafe
    /// memory operations
    RawPtr(Box<Type>),

    /// An unresolved type parameter, resolved at IR.
    Variable(TypeVariable),
    /// A type itself. This is used for static fields,
    /// currently only enum cases.
    Type(Box<Type>),
}

impl Type {
    /// Compares equality between types.
    /// [strict] decides if Type::Any always equals
    /// other types or not.
    pub fn equal(&self, other: &Self, strict: bool) -> bool {
        match (self, other) {
            (Self::Any, _) | (_, Self::Any) => !strict,

            (Self::Function(f), Self::Function(o)) => f == o,
            (Self::Closure(f), Self::Closure(o)) => f == o,
            (Self::Value(v), Self::Value(o)) => v == o,
            (Self::StrongRef(v), Self::StrongRef(o)) => v == o,
            (Self::WeakRef(v), Self::WeakRef(o)) => v == o,
            (Self::Variable(i), Self::Variable(o)) => i.index == o.index,
            (Self::RawPtr(p), Self::RawPtr(o)) => p == o,

            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }

    /// Returns type arguments of this type, if applicable.
    pub fn type_args(&self) -> Option<&Rc<TypeArguments>> {
        match self {
            Self::Function(inst) => Some(&inst.args),
            Self::Value(inst) | Self::WeakRef(inst) | Self::StrongRef(inst) => Some(&inst.args),
            Self::Type(ty) => ty.type_args(),
            _ => None,
        }
    }

    /// Returns type parameters of this type's prototype, if applicable.
    pub fn type_params(&self) -> Option<Rc<TypeParameters>> {
        Some(match self {
            Self::Function(inst) => Rc::clone(&inst.ty.borrow().type_parameters),
            Self::Value(inst) | Self::WeakRef(inst) | Self::StrongRef(inst) => {
                Rc::clone(&inst.ty.borrow().type_parameters)
            }
            Self::Type(ty) => return ty.type_params(),
            _ => return None,
        })
    }

    /// Sets type arguments of this type, if applicable.
    /// Returns success.
    pub fn set_type_args(&mut self, args: Rc<TypeArguments>) -> bool {
        match self {
            Self::Function(inst) => {
                inst.args = args;
                true
            }
            Self::Value(inst) | Self::WeakRef(inst) | Self::StrongRef(inst) => {
                inst.args = args;
                true
            }
            Self::Type(ty) => ty.set_type_args(args),
            _ => false,
        }
    }

    /// A list of all primitive types that are not defined in any gelix code,
    /// but are instead indirectly globally defined.
    pub fn primitives() -> [Type; 12] {
        [
            Type::None,
            Type::Bool,
            Type::I8,
            Type::I16,
            Type::I32,
            Type::I64,
            Type::U8,
            Type::U16,
            Type::U32,
            Type::U64,
            Type::F32,
            Type::F64,
        ]
    }

    /// Is this a primitive?
    pub fn is_primitive(&self) -> bool {
        self.is_none() || self.is_number()
    }

    /// Is this type a number?
    pub fn is_number(&self) -> bool {
        self.is_int() || self.is_float() || self.is_var_with_marker(Bound::Number)
    }

    /// Is this type an integer?
    pub fn is_int(&self) -> bool {
        self.is_signed_int()
            || self.is_unsigned_int()
            || self.is_bool()
            || self.is_var_with_marker(Bound::Integer)
    }

    /// Is this type a signed integer?
    pub fn is_signed_int(&self) -> bool {
        matches!(self, Type::I8 | Type::I16 | Type::I32 | Type::I64)
            || self.is_var_with_marker(Bound::SignedInt)
    }

    /// Is this type an unsigned integer?
    pub fn is_unsigned_int(&self) -> bool {
        matches!(self, Type::U8 | Type::U16 | Type::U32 | Type::U64)
            || self.is_var_with_marker(Bound::UnsignedInt)
    }

    /// Is this type a floating-point number?
    pub fn is_float(&self) -> bool {
        matches!(self, Type::F32 | Type::F64) || self.is_var_with_marker(Bound::Float)
    }

    /// Is this type a pointer at machine level?
    pub fn is_ptr(&self) -> bool {
        self.is_strong_ref()
            || self.is_weak_ref()
            || self.is_var_with_marker(Bound::StrongRef)
            || self.is_var_with_marker(Bound::WeakRef)
    }

    /// Can this type be assigned to variables?
    /// True for everything but static ADTs.
    pub fn is_assignable(&self) -> bool {
        !self.is_type()
    }

    /// Can this type 'escape' the function it is in?
    /// True for everything except weak references.
    pub fn can_escape(&self) -> bool {
        !self.is_weak_ref()
    }

    pub fn is_var_with_marker(&self, marker: Bound) -> bool {
        if let Type::Variable(var) = self {
            if let TypeParameterBound::Bound(bound) = &var.bound {
                marker == *bound
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Try turning this type into an ADT,
    /// if it contains one.
    pub fn try_adt(&self) -> Option<&Instance<ADT>> {
        match self {
            Type::Value(adt) | Type::WeakRef(adt) | Type::StrongRef(adt) => Some(adt),
            _ => None,
        }
    }

    pub fn to_strong(&self) -> Type {
        self.try_adt()
            .cloned()
            .map(Type::StrongRef)
            .unwrap_or_else(|| self.clone())
    }

    pub fn to_weak(&self) -> Type {
        self.try_adt()
            .cloned()
            .map(Type::WeakRef)
            .unwrap_or_else(|| self.clone())
    }

    pub fn to_value(&self) -> Type {
        self.try_adt()
            .cloned()
            .map(Type::Value)
            .unwrap_or_else(|| self.clone())
    }

    pub fn type_or_none(self) -> Option<Type> {
        match self {
            Type::None | Type::Any => None,
            _ => Some(self),
        }
    }

    /// Returns a list of available constructors, should self be a
    /// static type access.
    /// TODO: Copying the list of constructors is not great for performance
    pub fn get_constructors(&self) -> Option<Vec<MutRc<Function>>> {
        // Thanks, no box pattern matching!
        if let Type::Type(ty) = self {
            if let Some(ty) = ty.try_adt() {
                Some(ty.ty.borrow().constructors.clone())
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Returns casts to get this type to [goal].
    /// None = Not possible
    /// Some(Cast, None) = Single cast
    /// Some(Cast, ...) = Double cast with middle step
    pub fn can_cast_to(&self, goal: &Type) -> Option<(CastType, Option<(CastType, Type)>)> {
        if let Some(cast) = self.find_cast_ty(goal) {
            Some((cast, None))
        } else {
            match self {
                Type::WeakRef(inst) => Type::Value(inst.clone())
                    .find_cast_ty(&goal)
                    .map(|c| (c, Some((CastType::ToValue, Type::Value(inst.clone()))))),

                // TODO: Value to WR
                Type::StrongRef(inst) => Type::Value(inst.clone())
                    .find_cast_ty(&goal)
                    .map(|c| (c, Some((CastType::ToValue, Type::Value(inst.clone())))))
                    .or_else(|| {
                        Type::WeakRef(inst.clone())
                            .find_cast_ty(&goal)
                            .map(|c| (c, Some((CastType::StrongToWeak, Type::Value(inst.clone())))))
                    }),

                _ => None,
            }
        }
    }

    /// Tries finding a possible cast to [goal].
    /// Does not account for the types being identical.
    fn find_cast_ty(&self, goal: &Type) -> Option<CastType> {
        match (self, goal) {
            // Any, just return a no-op cast
            (Type::Any, _) | (_, Type::Any) => Some(CastType::Bitcast),

            // Interface cast
            _ if get_or_create_iface_impls(&self)
                .borrow()
                .interfaces
                .get(goal)
                .is_some() =>
            {
                Some(CastType::ToInterface(self.clone()))
            }

            // Strong reference to weak reference cast
            (Type::StrongRef(adt), Type::WeakRef(weak)) if adt == weak => {
                Some(CastType::StrongToWeak)
            }

            // Reference to value cast
            (Type::StrongRef(adt), Type::Value(value))
            | (Type::WeakRef(adt), Type::Value(value))
                if adt == value =>
            {
                Some(CastType::ToValue)
            }

            // Enum case to enum cast
            (Type::StrongRef(adt), Type::StrongRef(other))
            | (Type::WeakRef(adt), Type::WeakRef(other))
            | (Type::Value(adt), Type::Value(other)) => match &adt.ty.borrow().ty {
                ADTType::EnumCase { parent, .. }
                    if Rc::ptr_eq(parent, &other.ty) && other.args == adt.args =>
                {
                    Some(CastType::Bitcast)
                }

                _ => None,
            },

            // Number cast
            _ if self.is_int() && goal.is_int() => Some(CastType::Number),
            _ if self.is_float() && goal.is_float() => Some(CastType::Number),

            _ => None,
        }
    }

    pub fn resolve(&self, args: &Rc<TypeArguments>) -> Type {
        // Start by replacing any type variables with their concrete type
        let mut ty = match self {
            Type::Variable(var) => match var.modifier {
                VariableModifier::Value => args[var.index].clone(),
                VariableModifier::Weak => args[var.index].to_weak(),
                VariableModifier::Strong => args[var.index].to_strong(),
            },
            Type::RawPtr(box Type::Variable(var)) => Type::RawPtr(box match var.modifier {
                VariableModifier::Value => args[var.index].clone(),
                VariableModifier::Weak => args[var.index].to_weak(),
                VariableModifier::Strong => args[var.index].to_strong(),
            }),
            _ => self.clone(),
        };

        // Resolve any type args on itself if present,
        // for example resolving SomeAdt[T] to SomeAdt[ActualType]
        if let Some(a) = ty.type_args() {
            let new = Rc::new(a.iter().map(|a| a.resolve(args)).collect::<Vec<_>>());
            ty.set_type_args(new);
        }

        // If the type has empty type args but needs some, attach given ones
        // Done after arg resolution to prevent resolving given ones when that is not needed
        if self.type_args().map(|a| a.is_empty()).unwrap_or(false)
            && self.type_params().map(|a| !a.is_empty()).unwrap_or(false)
        {
            ty.set_type_args(Rc::clone(args));
        }

        ty
    }
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        self.equal(other, true)
    }
}

impl Eq for Type {}

impl Hash for Type {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // todo bad!!
        match self {
            Self::Function(v) => v.ty.borrow().name.lexeme.hash(state),
            Self::Value(v) | Self::StrongRef(v) | Self::WeakRef(v) => {
                v.ty.borrow().name.lexeme.hash(state)
            }
            _ => std::mem::discriminant(self).hash(state),
        }
    }
}

impl Default for Type {
    fn default() -> Self {
        Type::None
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Type::Function(_) => write!(f, "<function>"),
            Type::Closure(closure) => write!(f, "{}", closure),
            Type::Value(adt) => write!(f, "~{}", adt),
            Type::WeakRef(adt) => write!(f, "&{}", adt),
            Type::RawPtr(inner) => write!(f, "*{}", inner),
            Type::StrongRef(adt) => write!(f, "@{}", adt),
            Type::Variable(var) => write!(f, "{}: {}", var.name, var.bound),
            Type::Type(ty) => match **ty {
                Type::Function(_) => write!(f, "<function>"),
                Type::Closure(_) => write!(f, "<closure>"),
                Type::Value(_) => write!(f, "<ADT>"),
                Type::WeakRef(_) => write!(f, "<weak ref>"),
                Type::StrongRef(_) => write!(f, "<strong ref>"),
                _ => write!(f, "<{:?}>", self),
            },
            _ => write!(f, "{:?}", self),
        }
    }
}

/// An "instance" of a declaration, with type arguments.
/// Arguments can be absent from the type if it is to be used
/// generically; should not be absent in final GIR produced.
#[derive(Debug)]
pub struct Instance<T> {
    pub ty: MutRc<T>,
    args: Rc<TypeArguments>,
}

impl<T> Instance<T> {
    /// Create a new instance. Will register with inner type.
    pub fn new(ty: MutRc<T>, args: Rc<TypeArguments>) -> Instance<T> {
        Instance { ty, args }
    }

    /// Create a new instance with no type arguments.
    pub fn new_(ty: MutRc<T>) -> Instance<T> {
        Instance {
            ty,
            args: Rc::new(vec![]),
        }
    }

    pub fn args(&self) -> &Rc<TypeArguments> {
        &self.args
    }

    pub fn set_args(&mut self, args: Rc<TypeArguments>) {
        self.args = args;
    }
}

impl Instance<ADT> {
    pub fn get_method(&self, name: &str) -> Instance<Function> {
        Instance::new(
            Rc::clone(self.ty.borrow().methods.get(name).unwrap()),
            Rc::clone(&self.args),
        )
    }

    pub fn try_get_method(&self, name: &str) -> Option<Instance<Function>> {
        Some(Instance::new(
            Rc::clone(self.ty.borrow().methods.get(name)?),
            Rc::clone(&self.args),
        ))
    }
}

impl Display for Instance<ADT> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.ty.borrow().name.lexeme)?;
        print_type_args(f, &self.args)
    }
}

pub fn print_type_args(f: &mut Formatter, args: &TypeArguments) -> fmt::Result {
    if !args.is_empty() {
        let mut args = args.iter();
        args.next().map(|arg| write!(f, "[{}", arg));
        for arg in args {
            write!(f, ", {}", arg)?;
        }
        write!(f, "]")?;
    }
    Ok(())
}

impl<T> Clone for Instance<T> {
    /// Clone this instance; does 2 Rc clones
    fn clone(&self) -> Self {
        Self {
            ty: Rc::clone(&self.ty),
            args: Rc::clone(&self.args),
        }
    }
}

impl<T> PartialEq for Instance<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.ty, &other.ty) && self.args == other.args
    }
}

impl<T> Eq for Instance<T> {}

impl<T: Hash> Hash for Instance<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ty.borrow().hash(state);
        if !self.args.is_empty() {
            self.args.hash(state)
        }
    }
}

pub trait ToInstance<T> {
    fn to_inst(&self) -> Instance<T>;
    fn to_type(&self) -> Type;
}

impl ToInstance<ADT> for MutRc<ADT> {
    fn to_inst(&self) -> Instance<ADT> {
        Instance::new_(Rc::clone(self))
    }

    fn to_type(&self) -> Type {
        Type::StrongRef(self.to_inst())
    }
}

impl ToInstance<Function> for MutRc<Function> {
    fn to_inst(&self) -> Instance<Function> {
        Instance::new_(Rc::clone(self))
    }

    fn to_type(&self) -> Type {
        Type::Function(self.to_inst())
    }
}

/// Type parameter to be used when monomorphising.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TypeVariable {
    pub index: usize,
    pub name: SmolStr,
    pub modifier: VariableModifier,
    pub bound: TypeParameterBound,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum VariableModifier {
    Value,
    Weak,
    Strong,
}

/// A closure signature.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct ClosureType {
    pub parameters: Vec<Type>,
    pub ret_type: Type,
    pub ir: Cell<Option<IRClosure>>,
}

impl Display for ClosureType {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        write!(f, "(")?;
        if !self.parameters.is_empty() {
            let mut p_iter = self.parameters.iter();
            write!(f, "{}", p_iter.next().unwrap())?;
            for ty in p_iter {
                write!(f, ", {}", ty)?;
            }
        }
        write!(f, "): {}", self.ret_type)
    }
}

/// A single type parameter on a declaration.
#[derive(Debug, Clone)]
pub struct TypeParameter {
    /// Name of the parameter to use
    pub name: Token,
    /// Index in list of parameters
    pub index: usize,
    /// The bound to use for arguments
    pub bound: TypeParameterBound,
}

/// Bound for a type parameter.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TypeParameterBound {
    /// Bound on an interface; argument must implement it
    Interface(Box<Type>),
    /// Bound on some builtin bound marker
    Bound(Bound),
}

impl TypeParameterBound {
    /// Returns if the type matches this bound and can be used.
    pub fn matches(&self, ty: &Type) -> bool {
        match self {
            Self::Interface(i) => {
                let impls = get_or_create_iface_impls(ty);
                let impls = impls.borrow();
                impls.interfaces.contains_key(i)
            }

            Self::Bound(bound) => match bound {
                Bound::Unbounded => true,
                Bound::Primitive => ty.is_primitive(),
                Bound::Number => ty.is_number(),
                Bound::Integer => ty.is_int(),
                Bound::SignedInt => ty.is_signed_int(),
                Bound::UnsignedInt => ty.is_unsigned_int(),
                Bound::Float => ty.is_float(),
                Bound::Value => ty.is_value(),
                Bound::StrongRef => ty.is_strong_ref(),
                Bound::WeakRef => ty.is_weak_ref(),
            },
        }
    }

    /// Returns proper type parameter bound from AST.
    /// Can error if bound cannot be resolved.
    pub fn from_ast(resolver: &Resolver, ast: Option<&ast::Type>) -> Res<TypeParameterBound> {
        Ok(if let Some(ast) = ast {
            match ast {
                ast::Type::Ident(tok) => match &tok.lexeme[..] {
                    "Primitive" => Self::Bound(Bound::Primitive),
                    "Number" => Self::Bound(Bound::Number),
                    "Integer" => Self::Bound(Bound::Integer),
                    "SignedInt" => Self::Bound(Bound::SignedInt),
                    "UnsignedInt" => Self::Bound(Bound::UnsignedInt),
                    "Float" => Self::Bound(Bound::Float),
                    "Value" => Self::Bound(Bound::Value),
                    "StrongRef" => Self::Bound(Bound::StrongRef),
                    "WeakRef" => Self::Bound(Bound::WeakRef),
                    _ => Self::Interface(Box::new(resolver.find_type(ast)?)),
                },

                _ => Self::Interface(Box::new(resolver.find_type(ast)?)),
            }
        } else {
            Self::default()
        })
    }
}

impl Default for TypeParameterBound {
    fn default() -> Self {
        TypeParameterBound::Bound(Bound::Unbounded)
    }
}

impl Display for TypeParameterBound {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TypeParameterBound::Interface(iface) => write!(f, "{}", iface),
            TypeParameterBound::Bound(b) => write!(f, "{:?}", b),
        }
    }
}

/// A bound marker that is built into gelix.
/// See gelix docs for details on them.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Bound {
    Unbounded,
    Primitive,
    Number,
    Integer,
    SignedInt,
    UnsignedInt,
    Float,
    Value,
    StrongRef,
    WeakRef,
}

/// An implementation of an interface.
#[derive(Debug)]
pub struct IFaceImpl {
    pub implementor: Type,
    pub iface: Instance<ADT>,
    pub methods: HashMap<SmolStr, MutRc<Function>>,
    /// Module that the impl block is in.
    pub module: MutRc<Module>,
    pub ast: MutRc<ast::IFaceImpl>,
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
    /// Interface is always a strong reference.
    pub interfaces: HashMap<Type, IFaceImpl>,
    pub methods: HashMap<SmolStr, MutRc<Function>>,
}
