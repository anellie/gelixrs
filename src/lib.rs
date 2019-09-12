/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/12/19, 2:19 PM.
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

use crate::mir::generator::module::MIRModuleGenerator;
use crate::mir::generator::MIRError;
use crate::parser::ParserErrors;
use ast::module::Module;
use error::Error;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use crate::mir::MIRModule;

type ModulePath = Vec<Rc<String>>;
type SrcParseErrors = Vec<ParserErrors>;

pub fn parse_source(input: PathBuf) -> Result<Vec<Module>, SrcParseErrors> {
    let mut modules = Vec::new();
    make_modules(input, &mut vec![], &mut modules)?;
    Ok(modules)
}

fn make_modules(
    input: PathBuf,
    path: &mut ModulePath,
    modules: &mut Vec<Module>,
) -> Result<(), SrcParseErrors> {
    path.push(stem_to_rc_str(&input));

    if let Ok(dir) = input.read_dir() {
        let mut errors = Vec::new();
        for file in dir {
            let file = file.expect("Failed to read file").path();
            let submodule = make_modules(file, path, modules);

            if let Err(mut errs) = submodule {
                errors.append(&mut errs);
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }
    } else if *input
        .extension()
        .map(|ext| ext == "gel")
        .get_or_insert(false)
    {
        // If 'input' is a .gel file; parse it if true
        let code = fs::read_to_string(&input).expect("Failed to read file.");
        let mut module = Module::new(path);

        fill_module(&code, &mut module).map_err(|err| vec![ParserErrors::new(err, &code, path)])?;

        modules.push(module);
    }

    path.pop();
    Ok(())
}

fn fill_module(code: &str, module: &mut Module) -> Result<(), Vec<Error>> {
    let lexer = lexer::Lexer::new(code);
    let parser = parser::Parser::new(lexer);
    parser.parse(module)
}

pub fn compile_mir(modules: Vec<Module>) -> Result<Vec<MIRModule>, Vec<MIRError>> {
    let pool = MIRModuleGenerator::new(modules);
    pool.execute()
}

pub fn compile_ir(modules: Vec<MIRModule>) -> inkwell::module::Module {
    unimplemented!()
}

fn stem_to_rc_str(path: &PathBuf) -> Rc<String> {
    Rc::new(path.file_stem().unwrap().to_str().unwrap().to_string())
}
