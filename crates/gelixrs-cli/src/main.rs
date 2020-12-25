/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 2/3/20 2:26 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use gelixrs::{stem_to_smol, GIRFlags};
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug, Default)]
#[structopt(name = "gelixrs", about = "A compiler for the gelix language.")]
struct Opt {
    /// Run in-place instead of compiling
    #[structopt(short, long)]
    run: bool,

    /// Parse to AST and exit
    #[structopt(long)]
    parse: bool,

    /// Compile to GIR, print, and exit
    #[structopt(long)]
    gir: bool,

    /// Compile to GIR, print including all libs, and exit
    #[structopt(long = "gir-all")]
    gir_all: bool,

    /// Compile to LLVM IR, print, and exit
    #[structopt(long)]
    ir: bool,

    /// Do not include the standard library (very buggy, use for debugging only!)
    #[structopt(long = "no-std")]
    no_std: bool,

    /// Path of the resulting executable
    #[structopt(short, long)]
    output: Option<PathBuf>,

    /// The level of optimization to use with clang
    #[structopt(short = "O", default_value = "3")]
    optimize_level: usize,

    /// File to compile
    #[structopt(parse(from_os_str))]
    file: PathBuf,
}

fn main() {
    run(Opt::from_args()).map_err(|e| println!("{}", e)).ok();
}

fn run(args: Opt) -> Result<(), &'static str> {
    if !args.file.exists() {
        return Err("Given path does not exist.");
    }

    let modules = if !args.no_std {
        let std_mod = gelixrs::find_std_module()?;
        vec![args.file.clone(), std_mod]
    } else {
        vec![args.file.clone()]
    };

    let code = gelixrs::parse_source(modules).map_err(|errors| {
        for file in errors {
            println!("{} error(s):\n{}", file.errors.len(), file);
            println!();
        }
        "Parser encountered errors. Exiting."
    })?;

    if args.parse {
        let stem = stem_to_smol(&args.file);
        for module in code.iter().filter(|m| m.path.index(0).unwrap() == &stem) {
            println!("{:#?}\n\n", module);
        }
        return Ok(());
    }

    let gir_flags = GIRFlags {
        no_std: args.no_std,
        no_prelude: args.no_std,
    };
    let gir = gelixrs::compile_gir(code, gir_flags).map_err(|errors| {
        for error in errors {
            println!("{}\n", error);
        }
        "GIR generator encountered errors. Exiting."
    })?;

    if args.gir || args.gir_all {
        let stem = stem_to_smol(&args.file);
        for module in gir
            .modules
            .iter()
            .filter(|m| (m.borrow().path.index(0).unwrap() == &stem) || args.gir_all)
        {
            println!("{}", module.borrow())
        }
        return Ok(());
    }

    let module = gelixrs::compile_ir(gir);

    if args.ir {
        match args.output {
            Some(file) => {
                module.print_to_file(file).ok();
            }
            None => module.print_to_stderr(),
        };
        return Ok(());
    }

    if args.run {
        let mut engine = gelixrs::JIT::new(module);
        unsafe {
            engine.call("main");
        }
        return Ok(());
    }

    let result = gelixrs::produce_binary(
        module,
        args.output.ok_or("Missing output location.")?.as_os_str(),
        args.optimize_level,
    );

    if let Err(err) = result {
        println!("Error: {}", err);
    } else {
        println!("Compilation successful!");
    }
    Ok(())
}
