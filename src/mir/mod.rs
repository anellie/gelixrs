/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/3/19 6:25 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use nodes::{MIRClass, MIRVariable};

use crate::ast::declaration::ASTType;
use crate::lexer::token::Token;
use crate::mir::generator::{MIRError, MIRGenerator};
use crate::mir::nodes::MIRInterface;
use crate::ModulePath;

pub mod generator;
pub mod nodes;

type MutRc<T> = Rc<RefCell<T>>;

fn mutrc_new<T>(value: T) -> MutRc<T> {
    Rc::new(RefCell::new(value))
}

/// A struct produced by a generator. It contains the full MIR representation of a file/module.
#[derive(Debug, Default)]
pub struct MIRModule {
    pub path: Rc<ModulePath>,
    pub classes: HashMap<Rc<String>, MutRc<MIRClass>>,
    pub interfaces: HashMap<Rc<String>, MutRc<MIRInterface>>,
    pub functions: HashMap<Rc<String>, Rc<MIRVariable>>,
    /// Imported functions will only be declared; not defined.
    /// They are defined when modules are linked/combined at the end of IR generation.
    pub imported_func: HashMap<Rc<String>, Rc<MIRVariable>>,
}

impl MIRModule {
    pub fn new(path: Rc<ModulePath>) -> MIRModule {
        Self {
            path,
            ..Default::default()
        }
    }
}

pub trait ToMIRResult<T> {
    #[inline(always)]
    fn or_err(self, gen: &MIRGenerator, error_token: &Token, msg: &str) -> Result<T, MIRError>;

    #[inline(always)]
    fn or_anon_err(
        self,
        gen: &MIRGenerator,
        error_token: Option<&Token>,
        msg: &str,
    ) -> Result<T, MIRError>;

    #[inline(always)]
    fn or_type_err(
        self,
        gen: &MIRGenerator,
        error_ty: &Option<ASTType>,
        msg: &str,
    ) -> Result<T, MIRError>;
}

impl<T> ToMIRResult<T> for Option<T> {
    fn or_err(self, gen: &MIRGenerator, error_token: &Token, msg: &str) -> Result<T, MIRError> {
        self.ok_or_else(|| gen.error(error_token, error_token, msg))
    }

    fn or_anon_err(
        self,
        gen: &MIRGenerator,
        error_token: Option<&Token>,
        msg: &str,
    ) -> Result<T, MIRError> {
        self.ok_or_else(|| gen.anon_err(error_token, msg))
    }

    fn or_type_err(
        self,
        gen: &MIRGenerator,
        error_ty: &Option<ASTType>,
        msg: &str,
    ) -> Result<T, MIRError> {
        self.ok_or_else(|| gen.anon_err(error_ty.as_ref().map(|t| t.get_token()).flatten(), msg))
    }
}
