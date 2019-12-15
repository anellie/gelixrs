/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 10:53 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::module::ModulePath;
use crate::ast::Type as ASTType;
use crate::error::Res;
use crate::lexer::token::Token;
use crate::mir::{IFACE_IMPLS, MModule, MutRc};
use crate::mir::nodes::Variable;
use crate::mir::result::ToMIRResult;

use super::super::nodes::Type;

/// The MIR builder is used alongside the module after
/// all types have been declared. It can be used for
/// resolving types and similar tasks.
///
/// It is also used by the [MIRGenerator].
pub struct MIRBuilder {
    /// The path of the current module.
    /// Separate to reduce amount of borrows on the module.
    pub path: Rc<ModulePath>,

    /// The module this builder is linked to.
    module: MutRc<MModule>,

    /// A map of all type aliases, where the key
    /// will be translated to the value when encountered
    /// as a type. Used for instantiating generic stuff
    /// where the key is the parameter name (like T)
    /// and the value the type to use in its place.
    type_aliases: HashMap<Rc<String>, Type>,
}

impl MIRBuilder {
    /// Returns the MIR type of the AST type passed.
    /// Will search for the type in the scope of the module
    /// and return an error if no such type exists.
    /// Note that this function can call borrow_mut() on the module.
    pub fn find_type(&self, ast: &ASTType) -> Res<Type> {
        Ok(match ast {
            ASTType::Ident(tok) => {
                let ty = self.find_type_by_name(&tok);
                let ty = ty.or_else(|| Some(self.type_aliases.get(&tok.lexeme)?.clone()));
                ty.or_type_err(&self.path, ast, "Unknown type.")?
            }

            ASTType::Array(_) => unimplemented!(),

            ASTType::Closure { .. } => unimplemented!(),

            ASTType::Generic { .. } => unimplemented!(),
        })
    }

    fn find_type_by_name(&self, tok: &Token) -> Option<Type> {
        Some(match &tok.lexeme[..] {
            "None" => Type::None,
            "bool" => Type::Bool,

            "i8" => Type::I8,
            "i16" => Type::I16,
            "i32" => Type::I32,
            "i64" => Type::I64,

            "f32" => Type::F32,
            "f64" => Type::F64,

            "String" => Type::String,

            _ => self.module.borrow().find_type(&tok.lexeme)?,
        })
    }

    /// Searches for an associated method on a type. Can be either an interface
    /// method or a class method.
    pub fn find_associated_method(&self, ty: Type, name: &Token) -> Option<Rc<Variable>> {
        let class_method = if let Type::Class(class) = &ty {
            class.borrow().methods.get(&name.lexeme).cloned()
        } else {
            None
        };

        class_method.or_else(|| {
            IFACE_IMPLS
                .with(|impls| impls.borrow().get(&ty).cloned())?
                .borrow()
                .methods
                .get(&name.lexeme)
                .cloned()
        })
    }

    pub fn push_type_aliases(&mut self, params: &[Token], args: &[Type]) {
        self.type_aliases.extend(
            params
                .iter()
                .map(|t| &t.lexeme)
                .cloned()
                .zip(args.iter().cloned()),
        );
    }

    pub fn new(module: &MutRc<MModule>) -> MIRBuilder {
        MIRBuilder {
            path: Rc::clone(&module.borrow().path),
            module: Rc::clone(module),
            type_aliases: HashMap::new(),
        }
    }
}
