/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 6:51 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

#![feature(drain_filter)]
#![feature(refcell_take)]

#[macro_use]
extern crate enum_methods;
#[macro_use]
#[cfg(test)]
extern crate lazy_static;

use std::{env, fs, path::PathBuf, rc::Rc};

use crate::{
    ast::module::{Import, Module, ModulePath},
    error::{Error, Errors},
    gir::{generator::module::GIRModuleGenerator, MutRc},
    ir::IRGenerator,
    lexer::token::Token,
};
use smol_str::SmolStr;

pub mod ast;
//#[cfg(test)]
//pub mod bench;
pub mod error;
pub mod gir;
pub mod ir;
pub mod lexer;
pub mod parser;
#[cfg(test)]
pub mod tests;

pub fn parse_source(input: Vec<PathBuf>) -> Result<Vec<Module>, Vec<Errors>> {
    let mut modules = Vec::new();
    for path in input {
        make_modules(path, &mut ModulePath(vec![]), &mut modules)?;
    }
    Ok(modules)
}

fn make_modules(
    input: PathBuf,
    path: &mut ModulePath,
    modules: &mut Vec<Module>,
) -> Result<(), Vec<Errors>> {
    path.0.push(stem_to_smol(&input));

    if let Ok(dir) = input.read_dir() {
        let mut errors = Vec::new();
        for file in dir {
            let file = file.expect("Failed to read file").path();

            // If the file is named 'module.gel', it should have the
            // containing directory as its module path.
            let result = if file.file_name().unwrap() == "module.gel" {
                parse_module(file, path).map(|m| modules.push(m))
            } else {
                make_modules(file, path, modules)
            };

            if let Err(mut errs) = result {
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
        modules.push(parse_module(input, path)?);
    }

    path.0.pop();
    Ok(())
}

fn parse_module(input: PathBuf, path: &mut ModulePath) -> Result<Module, Vec<Errors>> {
    let code = Rc::new(fs::read_to_string(&input).expect("Failed to read file."));
    let mut module = Module::new(path, &code);

    fill_module(code, &mut module).map_err(|e| vec![e])?;
    Ok(module)
}

fn fill_module(code: Rc<String>, module: &mut Module) -> Result<(), Errors> {
    let lexer = lexer::Lexer::new(&code, &module.path);
    let tokens = lexer
        .consume()
        .map_err(|e| Errors(vec![e], Rc::clone(&code)))?;

    let parser = parser::Parser::new(tokens, Rc::clone(&module.path));
    parser.parse(module).map_err(|errs| Errors(errs, code))
}

pub fn auto_import_prelude(modules: &mut Vec<Module>) {
    let prelude_import = Import {
        path: Rc::new(ModulePath(vec![
            SmolStr::new_inline("std"),
            SmolStr::new_inline("prelude"),
        ])),
        symbol: Token::generic_identifier("+"),
    };

    for module in modules
        .iter_mut()
        .filter(|module| module.path != prelude_import.path)
    {
        module.imports.push(prelude_import.clone())
    }
}

pub fn compile_gir(modules: Vec<Module>) -> Result<Vec<MutRc<gir::Module>>, Vec<Errors>> {
    GIRModuleGenerator::new(modules).consume()
}

pub fn compile_ir(modules: Vec<MutRc<gir::Module>>) -> inkwell::module::Module {
    IRGenerator::new().generate(modules)
}

pub fn stem_to_smol(path: &PathBuf) -> SmolStr {
    SmolStr::new(path.file_stem().unwrap().to_str().unwrap())
}

pub fn find_std_module() -> Result<PathBuf, &'static str> {
    let mut local_std = env::current_dir().expect("Failed to get current directory!");
    local_std.push("std");
    if local_std.exists() {
        return Ok(local_std);
    }

    let mut user_std = dirs::data_dir().expect("Failed to get home directory!");
    user_std.push("gelix");
    user_std.push("std");
    if user_std.exists() {
        return Ok(user_std);
    }

    let system_std = PathBuf::from("/usr/local/lib/gelix/std");
    if system_std.exists() {
        return Ok(system_std);
    }

    Err("Failed to find standard library. Please make sure to follow the installation instructions.")
}
