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
use gir_nodes::{
    declaration::Visibility,
    types::{TypeArguments, TypeParameter},
};
use syntax::kind::SyntaxKind;

impl GIRGenerator {
    /// Tries finding a possible cast to [goal].
    /// Does not account for the types being identical.
    pub(crate) fn can_cast_type(&mut self, ty: &Type, goal: &Type) -> Option<CastType> {
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

            // Type to nullable cast
            (_, Type::Nullable(inner)) if ty.equal(inner, true) => Some(CastType::ToNullable),

            // Enum case to enum cast
            (Type::Adt(adt), Type::Adt(other)) => match &adt.ty.borrow().ty {
                ADTType::EnumCase { parent, .. }
                    if Rc::ptr_eq(parent, &other.ty) && other.args() == adt.args() =>
                {
                    Some(CastType::Bitcast)
                }

                _ => None,
            },

            // Enum case to nullable parent cast
            (Type::Adt(adt), Type::Nullable(box Type::Adt(other))) => match &adt.ty.borrow().ty {
                ADTType::EnumCase { parent, .. }
                    if Rc::ptr_eq(parent, &other.ty) && other.args() == adt.args() =>
                {
                    // This is fine, case to parent cast is performed implicitly
                    Some(CastType::ToNullable)
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
                Bound::Adt => ty.is_adt(),
                Bound::Nullable => ty.is_nullable(),
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
                    "Adt" => TypeParameterBound::Bound(Bound::Adt),
                    "Nullable" => TypeParameterBound::Bound(Bound::Nullable),
                    _ => TypeParameterBound::Interface(Box::new(self.find_type(ast)?)),
                },

                _ => TypeParameterBound::Interface(Box::new(self.find_type(ast)?)),
            }
        } else {
            TypeParameterBound::default()
        })
    }

    pub(crate) fn visibility_from_modifiers(
        &self,
        mods: impl Iterator<Item = SyntaxKind>,
        cst: &CSTNode,
    ) -> Visibility {
        let mut vis = None;
        for m in mods {
            let double = match m {
                SyntaxKind::Priv => vis.replace(Visibility::Private),
                SyntaxKind::Mod => vis.replace(Visibility::Module),
                _ => None,
            }
            .is_some();
            if double {
                self.err(cst.clone(), GErr::E318);
            }
        }

        vis.unwrap_or(Visibility::Public)
    }
}
