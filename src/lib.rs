/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/8/19, 6:09 PM.
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

use crate::ast::module::{FileModule, ModuleContent};
use ast::module::Module;
use error::Error;
use error::FileErrors;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

type Res<T> = Result<T, Error>;
type ModuleErrors = Vec<FileErrors>;

pub fn parse_source(input: PathBuf) -> Result<Vec<Module>, ModuleErrors> {
    let mut modules = Vec::new();
    modules.push(make_module(input)?);
    Ok(modules)
}

fn make_module(input: PathBuf) -> Result<Module, ModuleErrors> {
    let content = match input.read_dir() {
        Ok(dir) => {
            let mut errors = Vec::new();
            let mut content = HashMap::with_capacity(dir.size_hint().0);
            for file in dir {
                let file = file.expect("Failed to read file").path();
                let name = file.file_stem().unwrap().to_str().unwrap().to_string();
                let module = make_module(file);

                match module {
                    Ok(module) => {
                        content.insert(Rc::new(name), module);
                    }
                    Err(mut errs) => errors.append(&mut errs),
                }
            }

            if !errors.is_empty() {
                return Err(errors);
            }
            ModuleContent::Submodules(content)
        }

        Err(_) => {
            let mut file_mod = FileModule::default();
            let code = fs::read_to_string(&input).expect("Failed to read file.");

            fill_module(&code, &mut file_mod).map_err(|err| {
                let file_name = input.file_name().unwrap().to_str().unwrap().to_string();
                vec![FileErrors::new(err, &code, file_name)]
            })?;

            ModuleContent::File(file_mod)
        }
    };

    let module = Module {
        name: Rc::new(input.file_stem().unwrap().to_str().unwrap().to_string()),
        content,
    };
    Ok(module)
}

fn fill_module(code: &str, module: &mut FileModule) -> Result<(), Vec<Error>> {
    let lexer = lexer::Lexer::new(code);
    let parser = parser::Parser::new(lexer);
    parser.parse(module)
}

pub fn compile_ir(_ast_modules: Vec<Module>) -> Res<inkwell::module::Module> {
    unimplemented!()
    /*
    let mir = mir::generator::MIRGenerator::new().generate(declarations)?;
    Ok(ir::IRGenerator::new().generate(mir))
    */
}
