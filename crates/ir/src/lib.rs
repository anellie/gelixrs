#![feature(box_syntax)]
#![feature(box_patterns)]

mod generator;
pub mod jit;

use inkwell::module::Module;
use std::{env, error::Error, ffi::OsStr, fs, process};

pub use generator::IRGenerator;
use inkwell::context;

pub type CompiledIR = Module;

#[derive(Clone)]
pub struct Context(context::Context);

pub fn ir_context() -> Context {
    Context(context::Context::create())
}

pub fn produce_binary(
    module: Module,
    location: &OsStr,
    optimize_level: usize,
) -> Result<(), Box<dyn Error>> {
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
        Err(
            "Compiling to native binary failed. Please file a bug report."
                .to_string()
                .into(),
        )
    }
}
