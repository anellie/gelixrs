use std::rc::Rc;

use error::{GErr, Res};
use gir_nodes::{
    declaration::ADTType,
    expression::CastType,
    types::{Bound, TypeParameterBound},
    Type,
};

use crate::GIRGenerator;
use ast::CSTNode;
use gir_nodes::types::{TypeArguments, TypeParameter};

impl GIRGenerator {
    /// Returns casts to get the first type to [goal].
    /// None = Not possible
    /// Some(Cast, None) = Single cast
    /// Some(Cast, ...) = Double cast with middle step
    pub(crate) fn can_cast_type(
        &mut self,
        ty: &Type,
        goal: &Type,
    ) -> Option<(CastType, Option<(CastType, Type)>)> {
        if let Some(cast) = self.find_cast_ty(ty, goal) {
            Some((cast, None))
        } else {
            match ty {
                Type::WeakRef(inst) => self
                    .find_cast_ty(&Type::Value(inst.clone()), goal)
                    .map(|c| (c, Some((CastType::ToValue, Type::Value(inst.clone()))))),

                // TODO: Value to WR
                Type::StrongRef(inst) => self
                    .find_cast_ty(&Type::Value(inst.clone()), goal)
                    .map(|c| (c, Some((CastType::ToValue, Type::Value(inst.clone())))))
                    .or_else(|| {
                        self.find_cast_ty(&Type::WeakRef(inst.clone()), goal)
                            .map(|c| (c, Some((CastType::StrongToWeak, Type::Value(inst.clone())))))
                    }),

                _ => None,
            }
        }
    }

    /// Tries finding a possible cast to [goal].
    /// Does not account for the types being identical.
    fn find_cast_ty(&mut self, ty: &Type, goal: &Type) -> Option<CastType> {
        match (ty, goal) {
            // Any, just return a no-op cast
            (Type::Any, _) | (_, Type::Any) => Some(CastType::Bitcast),

            // Interface cast
            _ if self
                .get_iface_impls(&ty)
                .borrow()
                .interfaces
                .get(goal)
                .is_some() =>
            {
                Some(CastType::ToInterface(ty.clone()))
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
                    if Rc::ptr_eq(parent, &other.ty) && other.args() == adt.args() =>
                {
                    Some(CastType::Bitcast)
                }

                _ => None,
            },

            // Number cast
            _ if ty.is_int() && goal.is_int() => Some(CastType::Number),
            _ if ty.is_float() && goal.is_float() => Some(CastType::Number),

            _ => None,
        }
    }

    pub(crate) fn validate_type_args(
        &self,
        args: &TypeArguments,
        params: &[TypeParameter],
        cst: &CSTNode,
    ) {
        for (i, _) in args
            .iter()
            .zip(params.iter())
            .filter(|(a, p)| !self.matches_bound(a, &p.bound))
            .enumerate()
        {
            self.err(cst.clone(), GErr::E239(i))
        }
    }

    /// Returns if the type matches this bound and can be used.
    pub(crate) fn matches_bound(&self, ty: &Type, bound: &TypeParameterBound) -> bool {
        match bound {
            TypeParameterBound::Interface(i) => {
                let impls = self.maybe_get_iface_impls(ty);
                if let Some(impls) = impls {
                    let impls = impls.borrow();
                    impls.interfaces.contains_key(&i)
                } else {
                    false
                }
            }

            TypeParameterBound::Bound(bound) => match bound {
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
    pub(crate) fn bound_from_ast(&mut self, ast: Option<&ast::Type>) -> Res<TypeParameterBound> {
        Ok(if let Some(ast) = ast {
            match ast.get() {
                ast::TypeE::Ident(tok) => match &tok[..] {
                    "Primitive" => TypeParameterBound::Bound(Bound::Primitive),
                    "Number" => TypeParameterBound::Bound(Bound::Number),
                    "Integer" => TypeParameterBound::Bound(Bound::Integer),
                    "SignedInt" => TypeParameterBound::Bound(Bound::SignedInt),
                    "UnsignedInt" => TypeParameterBound::Bound(Bound::UnsignedInt),
                    "Float" => TypeParameterBound::Bound(Bound::Float),
                    "Value" => TypeParameterBound::Bound(Bound::Value),
                    "StrongRef" => TypeParameterBound::Bound(Bound::StrongRef),
                    "WeakRef" => TypeParameterBound::Bound(Bound::WeakRef),
                    _ => TypeParameterBound::Interface(Box::new(self.find_type(ast)?)),
                },

                _ => TypeParameterBound::Interface(Box::new(self.find_type(ast)?)),
            }
        } else {
            TypeParameterBound::default()
        })
    }
}
