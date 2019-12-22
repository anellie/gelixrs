/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/22/19 8:35 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{env, ffi::CStr, fs::read_to_string, os::raw::c_char, panic, path::PathBuf, sync::Mutex};

use inkwell::{execution_engine::JitFunction, OptimizationLevel};

type MainFn = unsafe extern "C" fn();

lazy_static! {
    static ref RESULT: Mutex<String> = Mutex::new(String::from(""));
    static ref STD_LIB: Mutex<PathBuf> = {
        let mut std_mod = env::current_dir().expect("Failed to get current directory!");
        std_mod.push("std");
        Mutex::new(std_mod)
    };
}

/// All possible ways the compiler can fail compilation.
#[derive(Debug, PartialEq)]
enum Failure {
    Parse,
    Compile,
    IR(String),
    Panic,
}

#[no_mangle]
extern "C" fn test_puts(string: *const c_char) {
    let string = unsafe { CStr::from_ptr(string) };
    RESULT
        .lock()
        .unwrap()
        .push_str(&format!("{}\n", string.to_str().unwrap_or("INVALID_UTF8")));
}

#[test]
fn gelix_tests() -> Result<(), ()> {
    let (mut test_total, mut test_failed) = (0, 0);
    let mut test_path = env::current_dir().expect("Couldn't get current dir.");
    test_path.push("tests");

    let iter = test_path.read_dir().expect("Test path is a file?");
    for file in iter
        .map(|f| f.unwrap().path())
        .filter(|p| p.file_stem().unwrap() != "benchmark")
    {
        match file.read_dir() {
            Ok(iter) => iter.for_each(|file| {
                run_test(
                    file.expect("Couldn't get file.").path(),
                    &mut test_total,
                    &mut test_failed,
                )
            }),
            Err(_) => run_test(file, &mut test_total, &mut test_failed),
        }
    }

    println!(
        "\n\nResult: {} out of {} tests succeeded.\n\n",
        (test_total - test_failed),
        test_total
    );
    Ok(())
}

fn run_test(path: PathBuf, total: &mut usize, failed: &mut usize) {
    *total += 1;
    println!("Running test #{}: {}", total, relative_path(&path));
    let expected = get_expected_result(path.clone());
    let result = panic::catch_unwind(|| exec_jit(path)).unwrap_or(Err(Failure::Panic));

    if result != expected {
        *failed += 1;
        println!(
            "Test failed!\nResult:   {:?}\nExpected: {:?}\n",
            result, expected
        );
    }
}

fn exec_jit(path: PathBuf) -> Result<String, Failure> {
    let mut code = super::parse_source(vec![path, STD_LIB.lock().unwrap().clone()]).map_err(|_| Failure::Parse)?;
    super::auto_import_prelude(&mut code);
    let mir = super::compile_mir(code).map_err(|_| Failure::Compile)?;
    let module = super::compile_ir(mir);

    let engine = module
        .create_jit_execution_engine(OptimizationLevel::None)
        .map_err(|_| Failure::IR("Failed to create JIT".to_string()))?;
    if let Some(fun) = &module.get_function("puts") {
        engine.add_global_mapping(fun, test_puts as usize);
    }

    unsafe {
        let main_fn: JitFunction<MainFn> = engine
            .get_function("main")
            .map_err(|_| Failure::IR("No main fn".to_string()))?;
        main_fn.call();
    }

    let mut result = RESULT.lock().unwrap();
    let result_copy = result.clone();
    result.clear();
    Ok(result_copy)
}

fn get_expected_result(mut path: PathBuf) -> Result<String, Failure> {
    // If the test is a directory, the wanted result is in a file 'expected' in the dir
    if path.is_dir() {
        path.push("expected");
    }

    let code = read_to_string(path).expect("Couldn't get wanted result");
    if code.starts_with("// P-ERR") {
        Err(Failure::Parse)
    } else if code.starts_with("// C-ERR") {
        Err(Failure::Compile)
    } else {
        let split: Vec<&str> = code.split("*/").collect();
        Ok(split[0][3..].to_string())
    }
}

fn relative_path(path: &PathBuf) -> String {
    // Unwrap it all!
    format!(
        "{}/{}",
        path.parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap(),
        path.file_name().unwrap().to_str().unwrap()
    )
}
