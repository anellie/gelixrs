use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    rc::Rc,
};

use crate::{
    ast,
    ast::declaration::GenericParam,
    gir::{
        generator::HIRGenerator,
        nodes::{
            expression::Expr,
            types::{
                ClosureType, Instance, Type, TypeParameter, TypeParameterBound, TypeParameters,
            },
        },
        Module,
    },
    lexer::token::Token,
    gir::{mutrc_new, MutRc},
};

/// A declaration is a top-level user-defined
/// item inside a module. This can be
/// either a function or an ADT;
/// interface implementations are an exception and
/// are instead attached to the implementor.
#[derive(Debug, EnumAsGetters, EnumIntoGetters)]
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
            Self::Function(f) => Type::Function(Instance::new(Rc::clone(f))),
            Self::Adt(a) => Type::Value(Instance::new(Rc::clone(a))),
        }
    }

    /// Returns the type parameters on the declaration.
    pub fn type_parameters(&self) -> Rc<TypeParameters> {
        match self {
            Self::Function(f) => Rc::clone(&f.borrow().type_parameters),
            Self::Adt(a) => Rc::clone(&a.borrow().type_parameters),
        }
    }
}

impl Clone for Declaration {
    /// Clones the declaration; only an Rc clone and therefore cheap.
    fn clone(&self) -> Self {
        match self {
            Self::Function(f) => Self::Function(Rc::clone(f)),
            Self::Adt(a) => Self::Adt(Rc::clone(a)),
        }
    }
}

/// A general purpose struct used for all user-defined data structures.
/// The ty field inside is used for further specialization.
#[derive(Debug)]
pub struct ADT {
    /// The name of the ADT.
    pub name: Token,
    /// All fields on the ADT.
    pub fields: HashMap<Rc<String>, Rc<Field>>,
    /// All methods of this ADT.
    pub methods: HashMap<Rc<String>, MutRc<Function>>,

    /// All constructors of the ADT, if any. They are simply methods
    /// with special constraints to enforce safety.
    ///
    /// Note that not all ADT have this function since not all are intended to be
    /// user-instantiated (for example interfaces or closure capture ADTs)
    pub constructors: Vec<MutRc<Function>>,

    /// Type parameters on this ADT, if any.
    pub type_parameters: Rc<TypeParameters>,

    pub ty: ADTType,
    pub ast: MutRc<ast::ADT>,
    pub module: MutRc<Module>,
}

impl ADT {
    /// TODO: Enum edge case is rather ugly
    pub fn from_ast(generator: &HIRGenerator, mut ast: ast::ADT) -> MutRc<ADT> {
        let mut enum_cases: Option<Vec<ast::ADT>> = None;
        let (mem_size, method_size, const_size, ty) = match &mut ast.ty {
            ast::ADTType::Class {
                variables,
                constructors,
                external,
            } => (
                variables.len(),
                ast.methods.len(),
                constructors.len(),
                ADTType::Class {
                    external: *external,
                },
            ),

            ast::ADTType::Interface => (0, ast.methods.len(), 0, ADTType::Interface),

            ast::ADTType::Enum {
                variables,
                ref mut cases,
            } => {
                enum_cases = Some(std::mem::replace(cases, vec![]));
                (
                    variables.len(),
                    ast.methods.len(),
                    0,
                    ADTType::Enum {
                        cases: HashMap::new(),
                    },
                )
            }

            _ => panic!("unknown ADT"),
        };

        let adt = mutrc_new(ADT {
            name: ast.name.clone(),
            fields: HashMap::with_capacity(mem_size),
            methods: HashMap::with_capacity(method_size),
            constructors: Vec::with_capacity(const_size),
            type_parameters: ast_generics_to_hir(&generator, &ast.generics, None),
            ty,
            ast: mutrc_new(ast),
            module: Rc::clone(&generator.module),
        });

        if let Some(mut cases) = enum_cases {
            let cases = cases
                .drain(..)
                .map(|c| Self::enum_case(generator, &adt, c))
                .collect();
            adt.borrow_mut().ty = ADTType::Enum { cases };
        }

        adt
    }

    fn enum_case(
        generator: &HIRGenerator,
        parent_rc: &MutRc<ADT>,
        ast: ast::ADT,
    ) -> (Rc<String>, MutRc<ADT>) {
        let parent = parent_rc.borrow();
        let ty = ADTType::EnumCase {
            parent: Rc::clone(&parent_rc),
            simple: ast.is_simple_enum(),
        };
        (
            ast.case_name(),
            mutrc_new(ADT {
                name: ast.name.clone(),
                fields: HashMap::with_capacity(ast.members().unwrap().len() + parent.fields.len()),
                methods: HashMap::with_capacity(ast.methods.len() + parent.methods.len()),
                constructors: Vec::with_capacity(ast.constructors().unwrap().len()),
                type_parameters: Rc::clone(&parent.type_parameters),
                ty,
                ast: mutrc_new(ast),
                module: Rc::clone(&generator.module),
            }),
        )
    }

    pub fn get_singleton_inst(inst: &MutRc<ADT>) -> Option<Expr> {
        if let ADTType::EnumCase {
            simple: no_body, ..
        } = &inst.borrow().ty
        {
            if *no_body {
                Some(Expr::Allocate {
                    // TODO: generics, how do they work
                    ty: Type::WeakRef(Instance::new(Rc::clone(inst))),
                    constructor: Rc::clone(&inst.borrow().constructors[0]),
                    args: vec![],
                    tok: Token::eof_token(1),
                })
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Takes a list of generics parameters of an AST node and
/// returns it's HIR representation. Can log an error
/// if type bound cannot be resolved.
pub fn ast_generics_to_hir(
    generator: &HIRGenerator,
    generics: &Option<Vec<GenericParam>>,
    parent_generics: Option<&TypeParameters>,
) -> Rc<TypeParameters> {
    let gen_iter = generics.as_ref().map(|g| {
        g.iter().enumerate().map(|elem| {
            TypeParameter {
                name: elem.1.name.clone(),
                index: elem.0,
                bound: TypeParameterBound::from_ast(&generator.resolver, elem.1.bound.as_ref())
                    .unwrap_or_else(|e| {
                        generator.error(e);
                        TypeParameterBound::default() // doesn't matter anymore, compilation failed anyway
                    }),
            }
        })
    });

    Rc::new(match (gen_iter, parent_generics) {
        (Some(gen), Some(parent)) => parent.iter().cloned().chain(gen).collect(),
        (Some(gen), None) => gen.collect(),
        (None, Some(parent)) => parent.clone(),
        (None, None) => vec![],
    })
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
        cases: HashMap<Rc<String>, MutRc<ADT>>,
    },

    /// An enum with known case.
    EnumCase { parent: MutRc<ADT>, simple: bool },
}

impl ADTType {
    /// Does this type has members that need to populated?
    pub fn has_members(&self) -> bool {
        self.is_class() || self.is_enum_case() || self.is_enum()
    }

    /// Returns the cases of an enum type.
    /// Use on any other type will result in a panic.
    pub fn cases(&self) -> &HashMap<Rc<String>, MutRc<ADT>> {
        if let ADTType::Enum { cases } = self {
            cases
        } else {
            unreachable!();
        }
    }
}

/// Field on an ADT.
#[derive(Clone, Debug)]
pub struct Field {
    /// The name of the field.
    pub name: Rc<String>,
    /// If this field is mutable by user code. ("val" vs "var")
    pub mutable: bool,
    /// The type of this field, either specified or inferred by initializer
    pub ty: Type,
    /// The initializer for this field, if any.
    pub initializer: Option<Box<Expr>>,
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
#[derive(Debug)]
pub struct Function {
    /// The name of the function, with its module before it ($mod:$func)
    /// The only functions with no name change are external functions
    pub name: Token,
    /// All parameters needed to call this function.
    pub parameters: Vec<Rc<LocalVariable>>,
    /// Type parameters on this function, if any.
    pub type_parameters: Rc<TypeParameters>,
    /// A list of expressions that make up the func, executed in order.
    pub exprs: Vec<Expr>,
    /// All variables declared inside the function.
    pub variables: HashMap<Rc<String>, Rc<LocalVariable>>,
    /// The return type of the function; Type::None if omitted.
    pub ret_type: Type,
    pub ast: MutRc<ast::Function>,
    pub module: MutRc<Module>,
}

impl Function {
    /// Inserts a variable into the functions allocation table.
    /// Returns the name of it (should be used since a change can be needed due to colliding names).
    pub fn insert_var(&mut self, mut name: Rc<String>, var: Rc<LocalVariable>) -> Rc<String> {
        if self.variables.contains_key(&name) {
            name = Rc::new(format!("{}-{}", name, self.variables.len()));
        }
        self.variables.insert(Rc::clone(&name), var);
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
    /// Returns a token for error reporting.
    pub fn get_token(&self) -> Token {
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
}

impl Hash for Variable {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Function(func) => func.ty.borrow().name.lexeme.hash(state),
            Self::Local(local) => local.name.lexeme.hash(state),
        }
    }
}

/// A local variable scoped to a function, can be
/// function parameters or user-defined variables.
#[derive(Debug, Clone)]
pub struct LocalVariable {
    /// The name of the variable.
    pub name: Token,
    /// Type of the variable.
    pub ty: Type,
    /// If it is mutable; user-decided on variables, false on fn arguments
    pub mutable: bool,
}
