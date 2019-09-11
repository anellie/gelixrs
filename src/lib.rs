/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/11/19, 7:49 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

// https://github.com/rust-lang/rust/pull/63059
#![feature(bind_by_move_pattern_guards)]
// https://github.com/rust-lang/rust/pull/60256
#![feature(option_flattening)]

#[macro_use]
#[cfg(test)]
extern crate lazy_static;

pub mod ast;
pub mod error;
pub mod ir;
pub mod lexer;
pub mod mir;
pub mod parser;
#[cfg(test)]
pub mod tests;

use ast::module::Module;
use error::Error;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use crate::mir::generator::module::MIRModuleGenerator;
use std::ffi::OsStr;
use crate::parser::ParserErrors;
use crate::mir::generator::MIRError;

type ModulePath = Vec<Rc<String>>;
type SrcParseErrors = Vec<ParserErrors>;

pub fn parse_source(input: PathBuf) -> Result<Vec<Module>, SrcParseErrors> {
    let mut modules = Vec::new();
    make_modules(input, &mut vec![], &mut modules)?;
    Ok(modules)
}

fn make_modules(input: PathBuf, path: &mut ModulePath, modules: &mut Vec<Module>) -> Result<(), SrcParseErrors> {
    path.push(to_rc_str(input.file_stem().unwrap()));

    let ret = match input.read_dir() {
        Ok(dir) => {
            let mut errors = Vec::new();
            for file in dir {
                let file = file.expect("Failed to read file").path();
                let submodule = make_modules(file, path, modules);

                if let Err(mut errs) = submodule {
                    errors.append(&mut errs);
                }
            }

            if errors.is_empty() {
                Ok(())
            } else {
                Err(errors)
            }
        }

        Err(_) => {
            if *input.extension().map(|ext| ext == "gel").get_or_insert(false) {
                // Its a .gel file; parse it
                let code = fs::read_to_string(&input).expect("Failed to read file.");
                let mut module = Module::new(path);

                fill_module(&code, &mut module).map_err(|err| {
                    vec![ParserErrors::new(err, &code, path)]
                })?;

                modules.push(module);
                Ok(())
            } else {
                // Not a .gel file; ignore
                Ok(())
            }
        }
    };

    path.pop();
    ret
}

fn fill_module(code: &str, module: &mut Module) -> Result<(), Vec<Error>> {
    let lexer = lexer::Lexer::new(code);
    let parser = parser::Parser::new(lexer);
    parser.parse(module)
}

pub fn compile_ir(modules: Vec<Module>) -> Result<inkwell::module::Module, Vec<MIRError>> {
    let pool = MIRModuleGenerator::new(modules);
    let modules = pool.execute()?;
    unimplemented!()
}

fn to_rc_str(os_str: &OsStr) -> Rc<String> {
    Rc::new(os_str.to_str().unwrap().to_string())
}