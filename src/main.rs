/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/12/19 5:36 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{env, fs, path::PathBuf, process};

use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "gelixrs")]
struct Opt {
    /// Run in-place instead of compiling
    #[structopt(short, long)]
    run: bool,

    /// Parse to AST and exit
    #[structopt(long = "parse-only")]
    parse_only: bool,

    /// Compile to MIR, print, and exit
    #[structopt(long)]
    mir: bool,

    /// Compile to LLVM IR, print, and exit
    #[structopt(long)]
    ir: bool,

    /// Don't auto-import 'std/prelude' into every module
    #[structopt(long = "no-prelude")]
    no_prelude: bool,

    /// Path of the resulting executable
    #[structopt(short, long)]
    output: Option<PathBuf>,

    /// File to compile
    #[structopt(parse(from_os_str))]
    file: PathBuf,
}

fn main() -> Result<(), &'static str> {
    let args = Opt::from_args();

    if !args.file.exists() {
        return Err("Given path does not exist.");
    }

    let mut std_mod = env::current_dir().expect("Failed to get current directory!");
    std_mod.push("stdlib");
    std_mod.push("std");
    let modules = vec![args.file.clone(), std_mod];

    let mut code = gelixrs::parse_source(modules).or_else(|errors| {
        for file in errors {
            println!(
                "{} error(s) in file {}:\n",
                file.errors.len(),
                file.file_name
            );
            for error in file.errors {
                println!("{}\n", error.to_string(&file.source));
            }
            println!();
        }
        Err("Parser encountered errors. Exiting.")
    })?;

    if args.parse_only {
        println!("{:#?}", code);
        return Ok(());
    }

    if !args.no_prelude {
        gelixrs::auto_import_prelude(&mut code)
    }

    let mir = gelixrs::compile_mir(code).or_else(|errors| {
        for error in errors {
            println!("{}\n", error.to_string(args.file.clone()));
        }
        Err("MIR generator encountered errors. Exiting.")
    })?;

    if args.mir {
        for module in mir {
            println!("{}\n\n", module);
        }
        return Ok(());
    }

    let module = gelixrs::compile_ir(mir);

    if args.ir {
        module.print_to_stderr();
        return Ok(());
    }

    let mut tmp_dir = env::temp_dir();
    tmp_dir.push("gelixrs");
    if !tmp_dir.exists() {
        fs::create_dir(&tmp_dir).expect("Failed to create temporary directory!");
    }

    let mut module_file = tmp_dir.clone();
    module_file.push("out.bc");
    module.write_bitcode_to_path(&module_file);

    let mut asm_file = tmp_dir;
    asm_file.push("out.o");

    let mut stdlib_dir = env::current_dir().expect("Failed to get current directory!");
    stdlib_dir.push("stdlib");
    stdlib_dir.push("target");
    stdlib_dir.push("x86_64-unknown-linux-musl");
    stdlib_dir.push("release");

    // TODO: Invoking clang as a command feels soo wrong, but I don't think there's a stable Rust API for it...
    process::Command::new("clang")
        .arg("-c")
        .arg("-o")
        .arg(&asm_file)
        .arg(module_file)
        .status()
        .expect("Evoking clang failed.");

    let status = process::Command::new("clang")
        .arg("-static")
        .arg("-o")
        .arg(&args.output.ok_or("Output location required.")?)
        .arg(asm_file)
        .arg("-L")
        .arg(stdlib_dir)
        .arg("-lstdlib")
        .status()
        .expect("Evoking clang failed.");

    if status.success() {
        println!("Compilation successful!");
        Ok(())
    } else {
        Err("Compiling to native binary failed. Please file a bug report.")
    }
}
