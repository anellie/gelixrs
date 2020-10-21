use std::rc::Rc;

use crate::{
    ast,
    ast::module::ModulePath,
    error::Res,
    hir::nodes::{
        expression::Expr,
        module::Module,
        types::{Type, TypeParameters},
    },
    mir::MutRc,
};
use crate::hir::hir_err;
use crate::hir::nodes::types::{ClosureType, VariableIndex};
use crate::hir::result::EmitHIRError;
use crate::lexer::token::Token;
use crate::hir::nodes::declaration::Declaration;

/// A resolver for types inside HIR.
/// Responsible for resolving all types,
/// and managing type parameters/arguments.
#[derive(Default)]
pub struct Resolver {
    /// Module currently compiling in
    pub module: MutRc<Module>,
    /// Path of [module]
    pub path: Rc<ModulePath>,
    // TODO: Isn't 2 enough in all cases?
    pub contexts: Vec<Rc<TypeParameters>>,
}

impl Resolver {
    /// Resolves the given AST type to its HIR equivalent.
    pub fn find_type(&self, ast: &ast::Type) -> Res<Type> {
        match ast {
            ast::Type::Ident(tok) => {
                let ty = self.find_type_by_name(&tok);
                let ty = ty.or_else(|| self.search_type_param(&tok.lexeme));
                let ty = ty.on_err(&self.path, ast.token(), "Unknown type.")?;
                // TODO generics validation

                if !ty.is_function() {
                    Ok(ty)
                } else {
                    Err(hir_err(
                        ast.token(),
                        format!("Functions cannot be used as types"),
                        &self.path,
                    ))
                }
            }

            ast::Type::Weak(inner) => {
                let inner = self.find_type(inner)?;
                if let Type::Value(adt) = inner {
                    Ok(Type::WeakRef(adt))
                } else {
                    Err(hir_err(
                        ast.token(),
                        format!("Weak is only applicable to ADTs, not {}.", inner),
                        &self.path
                    ))
                }
            }

            ast::Type::Strong(inner) => {
                let inner = self.find_type(inner)?;
                if let Type::Value(adt) = inner {
                    Ok(Type::StrongRef(adt))
                } else {
                    Err(hir_err(
                        ast.token(),
                        format!("Strong is only applicable to ADTs, not {}.", inner),
                        &self.path
                    ))
                }
            }

            ast::Type::Closure {
                params, ret_type, ..
            } => {
                let parameters = params
                    .iter()
                    .map(|p| self.find_type(p))
                    .collect::<Res<Vec<_>>>()?;
                let ret_type = ret_type
                    .as_ref()
                    .map_or(Ok(Type::None), |t| self.find_type(t))?;
                Ok(Type::Closure(Rc::new(ClosureType {
                    parameters,
                    ret_type,
                })))
            }

            ast::Type::Generic { token, types } => {
                // TODO: more validation
                let mut ty = self.find_type(&ast::Type::Ident(token.clone()))?;
                let mut args = ty.type_args().on_err(&self.path, token, "Type does not take type arguments.")?;
                *args = types
                    .iter()
                    .map(|p| self.find_type(p))
                    .collect::<Res<Vec<_>>>()?;
                Ok(ty)
            }

            _ => panic!(),
        }
    }

    fn find_type_by_name(&self, tok: &Token) -> Option<Type> {
        Some(match &tok.lexeme[..] {
            "None" => Type::None,
            "bool" => Type::Bool,

            "i8" => Type::I8,
            "i16" => Type::I16,
            "i32" => Type::I32,
            "i64" => Type::I64,
            #[cfg(target_pointer_width = "64")]
            "isize" => Type::I64,
            #[cfg(not(target_pointer_width = "64"))]
            "isize" => Type::I32,

            "u8" => Type::U8,
            "u16" => Type::U16,
            "u32" => Type::U32,
            "u64" => Type::U64,
            #[cfg(target_pointer_width = "64")]
            "usize" => Type::U64,
            #[cfg(not(target_pointer_width = "64"))]
            "usize" => Type::U32,

            "f32" => Type::F32,
            "f64" => Type::F64,

            _ => self.module.borrow().find_decl(&tok.lexeme).map(|d| d.to_type())?,
        })
    }

    fn search_type_param(&self, name: &String) -> Option<Type> {
        for (i, context) in self.contexts.iter().enumerate() {
            for param in context.iter() {
                if *param.name.lexeme == *name {
                    return Some(Type::Variable(
                        VariableIndex {
                            context: i,
                            index: param.index
                        },
                        param.bound.clone()
                    ))
                }
            }
        }
        None
    }

    /// Try casting a value to the given type.
    /// Returns the cast expression to use along with success.
    pub fn try_cast(&self, value: Expr, to: &Type) -> (Expr, bool) {
        todo!()
    }

    /// Switch to compiling a different module, resetting module state.
    pub fn switch_module(&mut self, new: MutRc<Module>) {
        self.module = new;
        self.path = Rc::clone(&self.module.borrow().path);
        self.contexts.clear();
    }

    /// Sets the current type parameters.
    pub fn set_context(&mut self, ctx: Rc<TypeParameters>) {
        self.contexts.clear();
        self.contexts.push(ctx);
    }
}
