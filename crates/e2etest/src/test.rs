/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 2/3/20 2:47 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{
    collections::HashSet, env, ffi::CStr, fs, fs::read_to_string, io, os::raw::c_char, panic,
    path::PathBuf, sync::Mutex,
};

use ansi_term::{Color, Style};
use gelixrs::Errors;
use lazy_static::lazy_static;
use std::{collections::HashMap, io::Write};

type MainFn = unsafe extern "C" fn();

lazy_static! {
    static ref RESULT: Mutex<String> = Mutex::new(String::from(""));
    static ref STD_LIB: Mutex<PathBuf> = {
        let mut std_mod = PathBuf::from(
            env::current_dir()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap(),
        );
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

fn main() -> Result<(), ()> {
    let (mut test_total, mut test_failed) = (0, Vec::new());

    let cwd = env::current_dir().expect("Couldn't get current dir.");
    let mut test_path = PathBuf::from(cwd.parent().unwrap().parent().unwrap());
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

    let mut snapshot = cwd.clone();
    snapshot.push("test-snapshot");
    print_failed(snapshot, &test_failed);

    println!(
        "\n{} out of {} tests succeeded\n",
        (test_total - test_failed.len()),
        test_total
    );
    Ok(())
}

const SNAPSHOT_OUTPUT: bool = true;

fn print_failed(path: PathBuf, failed: &[(String, String)]) {
    println!();
    if SNAPSHOT_OUTPUT {
        let snapshot = read_to_string(&path).unwrap_or("".to_string());
        let mut prev_failed = snapshot.lines().collect::<HashSet<&str>>();
        for (fail, msg) in failed {
            if !prev_failed.remove(fail.as_str()) {
                println!("{} {}", RED_BOLD.paint("New failure:"), msg);
            }
        }
        for fixed in prev_failed {
            println!("{} {}", GREEN_BOLD.paint("Test fixed:"), fixed);
        }
    } else {
        for (_, msg) in failed.into_iter().rev() {
            println!("{}", msg);
        }
    }

    let mut snapshots_file =
        fs::File::create(path.as_path()).expect("Failed to write snapshots file");
    for (fail, _) in failed {
        snapshots_file
            .write_all(format!("{}\n", fail).as_bytes())
            .expect("Write failed");
    }
}

fn run_test(path: PathBuf, total: &mut usize, failed: &mut Vec<(String, String)>) {
    *total += 1;
    let expected = get_expected_result(path.clone());
    let result = catch_unwind_silent(|| exec_jit(path.clone())).unwrap_or(Err(Failure::Panic));

    let style = if result == expected {
        GREEN_BOLD
    } else {
        let rel_path = relative_path(&path);
        failed.push((
            rel_path,
            format!(
                "{}\n{}\n{}   {:?}\n{} {:?}\n",
                RED_BOLD.paint(format!("Test #{} failed!", total)),
                BOLD.paint(format!("Test: {}", relative_path(&path))),
                BOLD.paint("Result:"),
                result,
                BOLD.paint("Expected:"),
                expected
            ),
        ));
        RED_BOLD
    };
    print!("{}", style.paint("."));
    io::stdout().flush().unwrap();
}

fn exec_jit(path: PathBuf) -> Result<String, Failure> {
    clear_state();

    let mut _code = gelixrs::parse_source(vec![path, STD_LIB.lock().unwrap().clone()])
        .map_err(|_| Failure::Parse)?;

    Err(Failure::Compile(vec![]))
    /*
    gelixrs::auto_import_prelude(&mut code);
    let gir = gelixrs::compile_gir(code).map_err(Failure::Compile)?;
    let module = gelixrs::compile_ir(gir);

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
    */
}

fn clear_state() {
    MALLOC_LIST.lock().unwrap().clear();
    gelixrs::clear_compiler_state();
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
