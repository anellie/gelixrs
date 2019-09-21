/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/21/19 4:25 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{env, ffi::CStr, fs::read_to_string, os::raw::c_char, path::PathBuf, sync::Mutex};

use inkwell::{execution_engine::JitFunction, OptimizationLevel};

type MainFn = unsafe extern "C" fn();

lazy_static! {
    static ref RESULT: Mutex<String> = Mutex::new(String::from(""));
    static ref STD_LIB: Mutex<PathBuf> = {
        let mut std_mod = env::current_dir().expect("Failed to get current directory!");
        std_mod.push("stdlib");
        std_mod.push("std");
        Mutex::new(std_mod)
    };
}

#[no_mangle]
extern "C" fn test_print(string: *const c_char) {
    let string = unsafe { CStr::from_ptr(string) };
    RESULT
        .lock()
        .unwrap()
        .push_str(&format!("{}\n", string.to_str().unwrap()));
}
#[no_mangle]
extern "C" fn test_printnum(num: i64) {
    RESULT.lock().unwrap().push_str(&format!("{}\n", num));
}

#[test]
fn run_all() -> Result<(), ()> {
    let mut test_path = env::current_dir().expect("Couldn't get current dir.");
    test_path.push("tests");
    for_file(test_path);
    Ok(())
}

fn for_file(file: PathBuf) {
    if file.is_dir() {
        for file in file.read_dir().expect("Couldn't get current dir.") {
            for_file(file.expect("Couldn't get file.").path());
        }
    } else {
        println!("Running test: {}", file.to_str().unwrap());
        let source = read_to_string(file.clone()).expect("Couldn't get file.");
        let expected = get_expected_result(&source);
        let result = exec_jit(file);

        if result != expected {
            println!(
                "Test failed!\nResult:   {:?}\nExpected: {:?}\n",
                result, expected
            );
        }
    }
}

fn exec_jit(file: PathBuf) -> Result<String, ()> {
    let mut code = super::parse_source(vec![file, STD_LIB.lock().unwrap().clone()])
        .ok()
        .ok_or(())?;
    super::auto_import_prelude(&mut code);
    let mir = super::compile_mir(code).ok().ok_or(())?;
    let module = super::compile_ir(mir);

    let engine = module
        .create_jit_execution_engine(OptimizationLevel::None)
        .ok()
        .ok_or(())?;
    engine.add_global_mapping(
        &module.get_function("print").ok_or(())?,
        test_print as usize,
    );
    engine.add_global_mapping(
        &module.get_function("printnum").ok_or(())?,
        test_printnum as usize,
    );

    unsafe {
        let main_fn: JitFunction<MainFn> = engine.get_function("main").ok().ok_or(())?;
        main_fn.call();
    }

    let mut result = RESULT.lock().unwrap();
    let result_copy = result.clone();
    result.clear();
    Ok(result_copy)
}

fn get_expected_result(code: &String) -> Result<String, ()> {
    if code.starts_with("// ERR") {
        Err(())
    } else {
        let split: Vec<&str> = code.split("*/").collect();
        Ok(split[0][3..].to_string())
    }
}
