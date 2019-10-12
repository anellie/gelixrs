/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/12/19 5:46 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{Display, Error, Formatter};
use std::rc::Rc;

use nodes::{MIRClass, MIRVariable};

use crate::{module_path_to_string, ModulePath};
use crate::ast::declaration::ASTType;
use crate::lexer::token::Token;
use crate::mir::generator::{MIRError, MIRGenerator};
use crate::mir::nodes::MIRInterface;

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

impl Display for MIRModule {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "----------------------------------------\n")?;
        write!(f, "Module {}\n", module_path_to_string(&self.path))?;
        write!(f, "----------------------------------------\n\n")?;

        if !self.functions.is_empty() {
            write!(f, "---------- Functions ----------\n")?;
        }
        for func in self.functions.iter().map(|f| MIRGenerator::var_to_function(f.1)) {
            write!(f, "{}\n", func.borrow())?;
        }

        if !self.classes.is_empty() {
            write!(f, "---------- Classes ----------\n")?;
        }
        for (_, class) in self.classes.iter() {
            write!(f, "{}\n", class.borrow())?;
        }

        Ok(())
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
