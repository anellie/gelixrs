use std::rc::Rc;

use crate::{result::EmitGIRError, GIRGenerator};
use ast::CSTNode;
use common::{mutrc_new, MutRc};
use error::{GErr, Res};
use gir_nodes::{
    declaration::ADTType,
    expression::{CastType, CastType::Bitcast},
    gir_err,
    types::{ClosureType, TypeParameters, TypeVariable, VariableModifier},
    Expr, IFaceImpls, Instance, Type,
};
use smol_str::SmolStr;
use std::{collections::HashMap, mem};

/// Resolver part of the GIR generator.
/// Responsible for resolving all types and casting them,
/// and managing type parameters/arguments.
impl GIRGenerator {
    /// Resolves the given AST type to its GIR equivalent.
    pub(crate) fn find_type(&self, ast: &ast::Type) -> Res<Type> {
        self.find_type_(ast, false)
    }

    pub(crate) fn find_type_(&self, ast: &ast::Type, allow_fn: bool) -> Res<Type> {
        match ast.get() {
            ast::TypeE::Ident(tok) => {
                let ty = self.search_type_param(&tok);
                let ty = ty.or_else(|| self.symbol(&tok));
                let ty = ty.or_err(&ast.cst, GErr::E300(tok.to_string()))?;
                // TODO generics validation

                if !ty.is_function() || allow_fn {
                    Ok(ty)
                } else {
                    Err(gir_err(ast.cst(), GErr::E301))
                }
            }

            ast::TypeE::Weak(inner) => {
                let inner = self.find_type(&inner)?;
                match inner {
                    Type::StrongRef(adt) | Type::Value(adt) | Type::WeakRef(adt) => {
                        Ok(Type::WeakRef(adt))
                    }
                    Type::Variable(mut var) => {
                        var.modifier = VariableModifier::Weak;
                        Ok(Type::Variable(var))
                    }
                    _ => Err(gir_err(ast.cst(), GErr::E302)),
                }
            }

            ast::TypeE::Value(inner) => {
                let inner = self.find_type(&inner)?;
                match inner {
                    Type::StrongRef(adt) | Type::Value(adt) | Type::WeakRef(adt) => {
                        Ok(Type::Value(adt))
                    }
                    Type::Variable(mut var) => {
                        var.modifier = VariableModifier::Value;
                        Ok(Type::Variable(var))
                    }
                    _ => Err(gir_err(ast.cst(), GErr::E303)),
                }
            }

            ast::TypeE::RawPtr(inner) => Ok(Type::RawPtr(Box::new(self.find_type(&inner)?))),

            ast::TypeE::Closure {
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

            ast::TypeE::Generic { ident, types } => {
                self.symbol_with_type_args(&ident, types.into_iter(), &ast.cst)
            }
        }
    }

    pub(crate) fn symbol(&self, name: &SmolStr) -> Option<Type> {
        Some(match &name[..] {
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

            _ => self.module.borrow().find_decl(name).map(|d| d.to_type())?,
        })
    }

    pub(crate) fn symbol_with_type_args<T: Iterator<Item = ast::Type>>(
        &self,
        ident: &SmolStr,
        args: T,
        cst: &CSTNode,
    ) -> Res<Type> {
        // TODO: more validation
        let mut ty = self
            .symbol(ident)
            .or_err(cst, GErr::E300(ident.to_string()))?;
        let args = args.map(|p| self.find_type(&p)).collect::<Res<Vec<_>>>()?;
        if !args.is_empty() {
            let args = Rc::new(args);
            let success = ty.set_type_args(Rc::clone(&args));
            if !success {
                return Err(gir_err(cst.clone(), GErr::E304));
            }
            self.validate_type_args(&args, &ty.type_params().unwrap(), cst);
        }
        Ok(ty)
    }

    fn search_type_param(&self, name: &str) -> Option<Type> {
        if let Some(params) = &self.type_params {
            for param in params.iter() {
                if *param.name == *name {
                    return Some(Type::Variable(TypeVariable::from_param(param)));
                }
            }
        }
        None
    }

    /// Will cast value to ty, if needed.
    /// If the cast is not possible, returns None.
    pub(crate) fn cast_or_none(&mut self, value: Expr, ty: &Type) -> Option<Expr> {
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
    pub(crate) fn try_cast(&mut self, value: Expr, ty: &Type) -> (Expr, bool) {
        let val_ty = value.get_type();
        if val_ty.equal(ty, false) {
            return (value, true);
        }

        (
            match self.can_cast_type(&val_ty, ty) {
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
    pub(crate) fn try_cast_in_place(&mut self, value_ref: &mut Expr, ty: &Type) -> bool {
        let value = mem::replace(value_ref, Expr::none_const());
        let (expr, success) = self.try_cast(value, ty);
        *value_ref = expr;
        success
    }

    /// Will try to make left and right be of the same type.
    /// Return value is `(NewType, left, right)`.
    /// If both are already the same type, this will just return the original type.
    /// If they cannot be made to match, it returns None as type.
    pub(crate) fn try_unify_type(&mut self, left: Expr, right: Expr) -> (Option<Type>, Expr, Expr) {
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

    /// Gets the interfaces implemented by a type.
    pub(crate) fn get_iface_impls(&mut self, ty: &Type) -> MutRc<IFaceImpls> {
        let impls = self.maybe_get_iface_impls(ty);
        match impls {
            Some(impls) => impls,
            None => {
                let iface_impls = mutrc_new(IFaceImpls {
                    implementor: ty.clone(),
                    interfaces: HashMap::with_capacity(2),
                    methods: HashMap::with_capacity(2),
                });
                self.iface_impls.insert(ty.clone(), Rc::clone(&iface_impls));
                iface_impls
            }
        }
    }

    /// Gets the interfaces implemented by a type.
    pub(crate) fn maybe_get_iface_impls(&self, ty: &Type) -> Option<MutRc<IFaceImpls>> {
        self.iface_impls.get(ty).cloned()
    }

    /// Sets the current type parameters.
    pub(crate) fn set_context(&mut self, ctx: &Rc<TypeParameters>) {
        self.type_params = Some(Rc::clone(ctx))
    }
}
