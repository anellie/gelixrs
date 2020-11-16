use std::rc::Rc;

use crate::{
    ast,
    ast::module::ModulePath,
    error::Res,
    gir::{
        gir_err,
        nodes::{
            declaration::ADTType,
            expression::{CastType, CastType::Bitcast, Expr},
            module::Module,
            types::{ClosureType, Instance, Type, TypeParameters, TypeVariable},
        },
        result::EmitGIRError,
        MutRc,
    },
    lexer::token::Token,
};
use std::mem;

/// A resolver for types inside GIR.
/// Responsible for resolving all types and casting them,
/// and managing type parameters/arguments.
#[derive(Default)]
pub struct Resolver {
    /// Module currently compiling in
    pub module: MutRc<Module>,
    /// Path of [module]
    pub path: Rc<ModulePath>,
    pub type_params: Option<Rc<TypeParameters>>,
}

impl Resolver {
    /// Resolves the given AST type to its GIR equivalent.
    pub fn find_type(&self, ast: &ast::Type) -> Res<Type> {
        self.find_type_(ast, false)
    }

    pub fn find_type_(&self, ast: &ast::Type, allow_fn: bool) -> Res<Type> {
        match ast {
            ast::Type::Ident(tok) => {
                let ty = self.search_type_param(&tok.lexeme);
                let ty = ty.or_else(|| self.find_type_by_name(&tok));
                let ty = ty.on_err(&self.path, ast.token(), "Unknown type.")?;
                // TODO generics validation

                if !ty.is_function() || allow_fn {
                    Ok(ty)
                } else {
                    Err(gir_err(
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
                    Err(gir_err(
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
                    Err(gir_err(
                        ast.token(),
                        format!("Strong is only applicable to ADTs, not {}.", inner),
                        &self.path,
                    ))
                }
            }

            ast::Type::RawPtr(inner) => Ok(Type::RawPtr(Box::new(self.find_type(inner)?))),

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
                    ..Default::default()
                })))
            }

            ast::Type::Generic { token, types } => {
                // TODO: more validation
                let mut ty = self.find_type_(&ast::Type::Ident(token.clone()), true)?;
                let args = types
                    .iter()
                    .map(|p| self.find_type(p))
                    .collect::<Res<Vec<_>>>()?;
                let success = ty.set_type_args(Rc::new(args));
                if !success {
                    None.on_err(&self.path, token, "Type does not take type arguments.")?;
                }
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

    fn search_type_param(&self, name: &str) -> Option<Type> {
        if let Some(params) = &self.type_params {
            for param in params.iter() {
                if *param.name.lexeme == *name {
                    return Some(Type::Variable(TypeVariable {
                        index: param.index,
                        name: param.name.lexeme.clone(),
                    }));
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
            println!("{} {}", value.get_type(), ty);
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
    /// Returns success.
    pub fn try_cast_in_place(&self, value_ref: &mut Expr, ty: &Type) -> bool {
        let value = mem::replace(value_ref, Expr::none_const_());
        let (expr, success) = self.try_cast(value, ty);
        *value_ref = expr;
        success
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
            if Rc::ptr_eq(&p1, &p2) && left_adt.unwrap().args() == right_adt.unwrap().args() {
                let inst = Instance::new(p1, Rc::clone(left_adt.unwrap().args()));
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
        self.type_params = None;
    }

    /// Sets the current type parameters.
    pub fn set_context(&mut self, ctx: &Rc<TypeParameters>) {
        self.type_params = Some(Rc::clone(ctx))
    }
}
