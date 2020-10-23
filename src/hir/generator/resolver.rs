use std::rc::Rc;

use crate::{
    ast,
    ast::module::ModulePath,
    error::Res,
    hir::{
        hir_err,
        nodes::{
            declaration::ADTType,
            expression::{CastType, CastType::Bitcast, Expr},
            module::Module,
            types::{ClosureType, Instance, Type, TypeParameters, VariableIndex},
        },
        result::EmitHIRError,
    },
    lexer::token::Token,
    mir::MutRc,
};
use std::mem;

/// A resolver for types inside HIR.
/// Responsible for resolving all types and casting them,
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
                        "Functions cannot be used as types".to_string(),
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
                        &self.path,
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
                        &self.path,
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
                let args = ty.type_args().on_err(
                    &self.path,
                    token,
                    "Type does not take type arguments.",
                )?;
                *args = types
                    .iter()
                    .map(|p| self.find_type(p))
                    .collect::<Res<Vec<_>>>()?;
                Ok(ty)
            }
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

            _ => self
                .module
                .borrow()
                .find_decl(&tok.lexeme)
                .map(|d| d.to_type())?,
        })
    }

    fn search_type_param(&self, name: &String) -> Option<Type> {
        for (i, context) in self.contexts.iter().enumerate() {
            for param in context.iter() {
                if *param.name.lexeme == *name {
                    return Some(Type::Variable(
                        VariableIndex {
                            context: i,
                            index: param.index,
                        },
                        param.bound.clone(),
                    ));
                }
            }
        }
        None
    }

    /// Will cast value to ty, if needed.
    /// If the cast is not possible, returns None.
    pub fn cast_or_none(&self, value: Expr, ty: &Type) -> Option<Expr> {
        let (value, success) = self.try_cast(value, ty);
        if success {
            Some(value)
        } else {
            None
        }
    }

    /// Checks if the value is of the given type ty.
    /// Will do casts if needed to make the types match;
    /// returns the new expression that should be used in case a cast happened.
    /// Boolean indicates if the cast was successful.
    pub fn try_cast(&self, value: Expr, ty: &Type) -> (Expr, bool) {
        let val_ty = value.get_type();
        if val_ty == *ty {
            return (value, true);
        }

        (
            match val_ty.can_cast_to(ty) {
                None => return (value, false),

                // The case of "to value, then bitcast" needs special handling to reverse order
                // since values cannot be bitcast
                Some((CastType::Bitcast, Some((CastType::ToValue, _)))) => Expr::cast(
                    Expr::cast(value, ty.to_weak(), CastType::Bitcast),
                    ty.clone(),
                    CastType::ToValue,
                ),

                Some((outer, Some((inner, inner_ty)))) => {
                    Expr::cast(Expr::cast(value, inner_ty, inner), ty.clone(), outer)
                }

                Some((outer, None)) => Expr::cast(value, ty.clone(), outer),
            },
            true,
        )
    }

    /// Same as above but utilizing `std::mem::replace` to only
    /// require a mutable reference at the cost of a slight performance penalty.
    pub fn try_cast_in_place(&self, value_ref: &mut Expr, ty: &Type) {
        let value = mem::replace(value_ref, Expr::none_const_());
        *value_ref = self.try_cast(value, ty).0; // todo use success
    }

    /// Will try to make left and right be of the same type.
    /// Return value is `(NewType, left, right)`.
    /// If both are already the same type, this will just return the original type.
    /// If they cannot be made to match, it returns None as type.
    pub fn try_unify_type(&self, left: Expr, right: Expr) -> (Option<Type>, Expr, Expr) {
        let left_ty = left.get_type();
        let right_ty = right.get_type();

        if left_ty == right_ty {
            return (Some(left_ty), left, right); // Nothing to do here
        }

        // If both are enum cases, they need special handling to cast to their supertype
        let (left_adt, right_adt) = (left_ty.try_adt(), right_ty.try_adt());
        if let (
            Some(ADTType::EnumCase { parent: p1, .. }),
            Some(ADTType::EnumCase { parent: p2, .. }),
        ) = (
            left_adt.map(|a| a.ty.borrow().ty.clone()),
            right_adt.map(|a| a.ty.borrow().ty.clone()),
        ) {
            if Rc::ptr_eq(&p1, &p2) && left_adt.unwrap().args == right_adt.unwrap().args {
                let inst = Instance {
                    ty: p1,
                    args: left_adt.unwrap().args.clone(),
                };
                let ty = match (left_ty, right_ty) {
                    (Type::StrongRef(_), Type::StrongRef(_)) => Type::StrongRef(inst),
                    _ => Type::WeakRef(inst),
                };

                // Run this function a second time to convert any
                // value/weak/strong mismatches
                return self.try_unify_type(
                    Expr::cast(left, ty.clone(), Bitcast),
                    Expr::cast(right, ty, Bitcast),
                );
            }
        }

        // Simply trying to cast one into the other is enough for all other cases
        let (left, success) = self.try_cast(left, &right_ty);
        if success {
            return (Some(right_ty), left, right);
        }

        let (right, success) = self.try_cast(right, &left_ty);
        if success {
            return (Some(left_ty), left, right);
        }

        (None, left, right)
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
