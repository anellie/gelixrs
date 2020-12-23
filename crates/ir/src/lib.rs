#![feature(box_syntax)]

pub mod jit;

use inkwell::module::Module;
use std::{error::Error, env, fs, process};
use std::ffi::OsStr;

pub type CompiledIR = Module;

pub fn produce_binary(module: Module, location: &OsStr, optimize_level: usize) -> Result<(), Box<dyn Error>> {
    let mut tmp_dir = env::temp_dir();
    tmp_dir.push("gelixrs");
    if !tmp_dir.exists() {
        fs::create_dir(&tmp_dir)?;
    }

    let mut module_file = tmp_dir;
    module_file.push("out.bc");
    module.write_bitcode_to_path(&module_file);

    if optimize_level > 3 {
        return Err("Invalid optimize level.".to_string().into());
    }
    let status = process::Command::new("clang")
        .arg("-o")
        .arg(&location)
        .arg(module_file)
        .arg(format!("-O{}", optimize_level))
        .output()?
        .status;

    if status.success() {
        Ok(())
    } else {
        Err("Compiling to native binary failed. Please file a bug report.".to_string().into())
    }
}