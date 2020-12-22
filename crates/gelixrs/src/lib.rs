use ast::Module;
use common::ModPath;
pub use error::Errors;
pub use parser::ParseResult;
use smol_str::SmolStr;
use std::{env, fs, path::PathBuf, rc::Rc};

pub type ParsedModules = Vec<Module>;

pub fn parse_source(input: Vec<PathBuf>) -> Result<ParsedModules, Vec<Errors>> {
    let mut modules = Vec::new();
    for path in input {
        make_modules(path, &mut ModPath::new(), &mut modules)?;
    }
    Ok(modules)
}

pub fn clear_compiler_state() {
    // INTRINSICS.with(|i| i.replace(Intrinsics::default()));
    // IFACE_IMPLS.with(|i| i.replace(HashMap::default()));
}

fn make_modules(
    input: PathBuf,
    path: &mut ModPath,
    modules: &mut ParsedModules,
) -> Result<(), Vec<Errors>> {
    path.push(stem_to_smol(&input));

    if let Ok(dir) = input.read_dir() {
        let mut errors = Vec::new();
        for file in dir {
            let file = file.expect("Failed to read file").path();

            // If the file is named 'module.gel', it should have the
            // containing directory as its module path.
            let result = if file.file_name().unwrap() == "module.gel" {
                parse_module(file, path)
                    .map(|m| modules.push(m))
                    .map_err(|e| vec![e])
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
        modules.push(parse_module(input, path).map_err(|e| vec![e])?);
    }

    path.pop();
    Ok(())
}

fn parse_module(input: PathBuf, path: &mut ModPath) -> Result<Module, Errors> {
    let code = Rc::new(fs::read_to_string(&input).expect("Failed to read file."));
    let parse = parser::parse(&code);
    let cst = parse.map_err(|errors| Errors {
        errors,
        src: Some(Rc::clone(&code)),
        origin: format!("{}", path),
    })?;
    Ok(Module::new(&path, &code, cst))
}

fn stem_to_smol(path: &PathBuf) -> SmolStr {
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
