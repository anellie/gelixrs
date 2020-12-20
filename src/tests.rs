/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 2/3/20 2:47 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{collections::HashSet, env, ffi::CStr, fs::read_to_string, os::raw::c_char, panic, path::PathBuf, sync::Mutex, io};

use crate::{
    error::Errors,
    gir::{
        generator::intrinsics::{Intrinsics, INTRINSICS},
        IFACE_IMPLS,
    },
};
use inkwell::{execution_engine::JitFunction, OptimizationLevel};
use std::collections::HashMap;
use ansi_term::{Style, Color};
use std::io::Write;

type MainFn = unsafe extern "C" fn();

lazy_static! {
    static ref RESULT: Mutex<String> = Mutex::new(String::from(""));
    static ref STD_LIB: Mutex<PathBuf> = {
        let mut std_mod = env::current_dir().expect("Failed to get current directory!");
        std_mod.push("std");
        Mutex::new(std_mod)
    };
    static ref MALLOC_LIST: Mutex<HashSet<i64>> = Mutex::new(HashSet::with_capacity(50));
}

/// All possible ways the compiler can fail compilation.
#[derive(Debug)]
enum Failure {
    Parse,
    Compile(Vec<Errors>),
    IR(String),
    Panic,
    Leak(usize),
}

impl PartialEq for Failure {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[no_mangle]
extern "C" fn test_puts(string: *const c_char) {
    let string = unsafe { CStr::from_ptr(string) };
    RESULT
        .lock()
        .unwrap()
        .push_str(&format!("{}\n", string.to_str().unwrap_or("INVALID_UTF8")));
}

extern "C" {
    fn malloc(x: i64) -> i64;
}

extern "C" fn test_malloc(size: i64) -> i64 {
    let ptr = unsafe { malloc(size) };
    // TODO: Re-add once GC is fixed
    // MALLOC_LIST.lock().unwrap().insert(ptr);
    ptr
}

static ERROR_STR: &str = "USE AFTER FREE";

extern "C" fn test_free(ptr: i64) {
    MALLOC_LIST.lock().unwrap().remove(&ptr);
    // TODO: free seems to cause SIGSEGV on some tests,
    // even though they run fine outside the test runner.
    // This shouldn't be a very big issue unless more tests
    // with big memory requirements are added;
    // not calling free causes the runner to leak less than 1MB of RAM.
    // unsafe { free(ptr) }

    // Write a pointer to an error string to this location -
    // this makes catching use after free bugs much easier
    unsafe {
        let ptr = ptr as *mut &str;
        ptr.write(ERROR_STR)
    }
}

#[test]
fn gelix_tests() -> Result<(), ()> {
    let (mut test_total, mut test_failed) = (0, Vec::new());
    let mut test_path = env::current_dir().expect("Couldn't get current dir.");
    test_path.push("tests");

    let iter = test_path.read_dir().expect("Test path is a file?");
    for file in iter
        .map(|f| f.unwrap().path())
        .filter(|p| p.file_stem().unwrap() != "benchmark")
    {
        match file.read_dir() {
            Ok(iter) => iter
                .map(|file| file.unwrap().path())
                .filter(|file| file.extension() != Some("disabled".as_ref()))
                .for_each(|file| run_test(file, &mut test_total, &mut test_failed)),
            Err(_) => run_test(file, &mut test_total, &mut test_failed),
        }
    }

    for fail in test_failed.iter().rev() {
        println!("{}", fail);
    }
    println!(
        "\n{} out of {} tests succeeded\n",
        (test_total - test_failed.len()),
        test_total
    );
    Ok(())
}

fn run_test(path: PathBuf, total: &mut usize, failed: &mut Vec<String>) {
    *total += 1;
    let expected = get_expected_result(path.clone());
    let result = catch_unwind_silent(|| exec_jit(path.clone())).unwrap_or(Err(Failure::Panic));

    let style = if result == expected {
        GREEN_BOLD
    } else {
        failed.push(format!(
            "{}\n{}\n{}   {:?}\n{} {:?}\n",
            RED_BOLD.paint(format!("Test #{} failed!", total)),
            BOLD.paint(format!("Test: {}", relative_path(&path))),
            BOLD.paint("Result:"),
            result,
            BOLD.paint("Expected:"),
            expected
        ));
        RED_BOLD
    };
    print!("{}", style.paint("."));
    io::stdout().flush().unwrap();
}

fn exec_jit(path: PathBuf) -> Result<String, Failure> {
    clear_state();

    let mut code = super::parse_source(vec![path, STD_LIB.lock().unwrap().clone()])
        .map_err(|_| Failure::Parse)?;
    super::auto_import_prelude(&mut code);
    let gir = super::compile_gir(code).map_err(Failure::Compile)?;
    let module = super::compile_ir(gir);

    let engine = module
        .create_jit_execution_engine(OptimizationLevel::None)
        .map_err(|_| Failure::IR("Failed to create JIT".to_string()))?;
    if let Some(fun) = &module.get_function("puts") {
        engine.add_global_mapping(fun, test_puts as usize);
    }
    engine.add_global_mapping(
        &module.get_function("malloc").unwrap(),
        test_malloc as usize,
    );
    engine.add_global_mapping(&module.get_function("free").unwrap(), test_free as usize);

    unsafe {
        let main_fn: JitFunction<MainFn> = engine
            .get_function("main")
            .map_err(|_| Failure::IR("No main fn".to_string()))?;
        main_fn.call();
    }

    let mut result = RESULT.lock().unwrap();
    let result_copy = result.clone();
    result.clear();

    let leaked = MALLOC_LIST.lock().unwrap().len();
    if leaked > 0 {
        return Err(Failure::Leak(leaked));
    }
    Ok(result_copy)
}

fn clear_state() {
    MALLOC_LIST.lock().unwrap().clear();
    INTRINSICS.with(|i| i.replace(Intrinsics::default()));
    IFACE_IMPLS.with(|i| i.replace(HashMap::default()));
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
        Err(Failure::Compile(vec![]))
    } else if code.starts_with("// LEAK") {
        Err(Failure::Leak(1))
    } else {
        let split = code.split("*/").next().unwrap();
        Ok(split[3..].to_string())
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

// https://stackoverflow.com/a/59211505
fn catch_unwind_silent<F: FnOnce() -> R + panic::UnwindSafe, R>(f: F) -> std::thread::Result<R> {
    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let result = panic::catch_unwind(f);
    panic::set_hook(prev_hook);
    result
}

const GREEN_BOLD: Style = {
    Style {
        foreground: Some(Color::Green),
        background: None,
        is_bold: true,
        is_dimmed: false,
        is_italic: false,
        is_underline: false,
        is_blink: false,
        is_reverse: false,
        is_hidden: false,
        is_strikethrough: false,
    }
};

const BOLD: Style = {
    Style {
        foreground: None,
        background: None,
        is_bold: true,
        is_dimmed: false,
        is_italic: false,
        is_underline: false,
        is_blink: false,
        is_reverse: false,
        is_hidden: false,
        is_strikethrough: false,
    }
};

const RED_BOLD: Style = {
    Style {
        foreground: Some(Color::Red),
        background: None,
        is_bold: true,
        is_dimmed: false,
        is_italic: false,
        is_underline: false,
        is_blink: false,
        is_reverse: false,
        is_hidden: false,
        is_strikethrough: false,
    }
};