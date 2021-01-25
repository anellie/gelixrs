/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 2/3/20 2:47 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{
    collections::HashSet, env, ffi::CStr, fs, fs::read_to_string, io, mem, os::raw::c_char, panic,
    path::PathBuf, process, sync::Mutex,
};

use ansi_term::{Color, Style};
use common::bench;
use gelixrs::{ir_context, CompiledGIR, CompiledIR, Context, Errors, GIRFlags, BENCH};
use lazy_static::lazy_static;
use std::{io::Write, panic::AssertUnwindSafe};
use structopt::StructOpt;

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
    Parse(Vec<Errors>),
    Compile(Vec<Errors>),
    IR,
    Panic,
    Subprocess,
    Segfault,
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

#[allow(clippy::let_and_return)] // Remove once todo is fixed.
extern "C" fn test_malloc(size: i64) -> i64 {
    let ptr = unsafe { malloc(size) };
    // TODO reenable once IR is fixed
    // MALLOC_LIST.lock().unwrap().insert(ptr);
    ptr
}

static ERROR_STR: &str = "USE AFTER FREE";

extern "C" fn test_free(ptr: i64) {
    MALLOC_LIST.lock().unwrap().remove(&ptr);

    // Write a pointer to an error string to this location -
    // this makes catching use after free bugs much easier
    // We don't actually free the allocation, so the test runner
    // leaks a little - this is fine
    unsafe {
        let ptr = ptr as *mut &str;
        ptr.write(ERROR_STR)
    }
}

#[derive(StructOpt, Debug, Default)]
#[structopt(name = "gelixrs-e2e", about = "A E2E test suite for gelix.")]
struct Opt {
    /// Use snapshot failure display
    #[structopt(long)]
    snapshot: bool,

    /// Run tests as subprocess instead of JIT, slower
    /// but segfault-safe
    #[structopt(long = "no-jit")]
    no_jit: bool,

    /// Don't cache the stdlib and recompile it for every test
    #[structopt(long = "no-cache")]
    no_cache: bool,

    /// Print each test name before running it
    #[structopt(long)]
    verbose: bool,
}

type TestRes = Result<String, Failure>;

struct TestRun {
    total: usize,
    failed: Vec<FailedTest>,
    gir_stdlib: Option<CompiledGIR>,
    ir_context: Context,
    options: Opt,
}

struct FailedTest {
    rel_path: String,
    count: usize,
    result: TestRes,
    expected: TestRes,
}

fn main() {
    let mut run = TestRun {
        total: 0,
        failed: vec![],
        gir_stdlib: None,
        ir_context: ir_context(),
        options: Opt::from_args(),
    };

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
                .for_each(|file| run_test(file, &mut run)),
            Err(_) => run_test(file, &mut run),
        }
    }

    let mut snapshot = cwd;
    snapshot.push("test-snapshot");
    print_failed(snapshot, &run);

    println!(
        "\n{} out of {} tests succeeded\n",
        (run.total - run.failed.len()),
        run.total
    );
}

fn print_failed(path: PathBuf, run: &TestRun) {
    println!();
    let print_test = |test: &FailedTest| {
        format!(
            "{}\n{}\n{}   {:?}\n{} {:?}\n",
            RED_BOLD.paint(format!("Test #{} failed!", test.count)),
            BOLD.paint(format!("Test: {}", test.rel_path)),
            BOLD.paint("Result:"),
            test.result,
            BOLD.paint("Expected:"),
            test.expected
        )
    };

    let snapshot = read_to_string(&path).unwrap_or_else(|_| "".to_string());
    let mut prev_failed = snapshot.lines().collect::<HashSet<&str>>();
    for fail in &run.failed {
        if !prev_failed.remove(fail.rel_path.as_str()) && run.options.snapshot {
            println!("{} {}", RED_BOLD.paint("New failure:"), print_test(fail));
        }
    }
    if !run.options.snapshot {
        for fail in run.failed.iter().rev() {
            println!("{}", print_test(fail));
        }
    }
    for fixed in prev_failed {
        println!("{} {}", GREEN_BOLD.paint("Test fixed:"), fixed);
    }

    println!("\n{}", BENCH.lock().unwrap());

    let mut snapshots_file =
        fs::File::create(path.as_path()).expect("Failed to write snapshots file");
    for fail in &run.failed {
        snapshots_file
            .write_all(format!("{}\n", fail.rel_path).as_bytes())
            .expect("Write failed");
    }
}

fn run_test(path: PathBuf, run: &mut TestRun) {
    run.total += 1;
    if run.options.verbose {
        println!("Running test: {}", relative_path(&path))
    }

    let expected = get_expected_result(path.clone());
    let result = catch_unwind_silent(|| exec(path.clone(), run)).unwrap_or(Err(Failure::Panic));

    let style = if result == expected {
        GREEN_BOLD
    } else {
        let rel_path = relative_path(&path);
        run.failed.push(FailedTest {
            rel_path,
            count: run.total,
            result,
            expected,
        });
        RED_BOLD
    };
    print!("{}", style.paint("."));
    io::stdout().flush().unwrap();
}

fn exec(path: PathBuf, run: &mut TestRun) -> TestRes {
    clear_state();

    let gir = if run.options.no_cache {
        let code = gelixrs::parse_source(vec![path, std_mod()]).map_err(Failure::Parse)?;
        gelixrs::compile_gir(code, GIRFlags::default())
    } else {
        maybe_compile_stdlib(run)?;
        let std = run.gir_stdlib.as_ref().unwrap();

        let code = gelixrs::parse_source(vec![path]).map_err(Failure::Parse)?;
        gelixrs::compile_gir_cached_std(code, std, GIRFlags::default())
    }
    .map_err(Failure::Compile)?;
    let module = gelixrs::compile_ir(run.ir_context.clone(), gir);

    if !run.options.no_jit {
        bench!("jit", exec_jit(module))
    } else {
        bench!("bin", exec_bin(module))
    }
}

fn exec_jit(module: CompiledIR) -> TestRes {
    let mut jit = gelixrs::JIT::new(module);
    jit.link_fn("puts", test_puts as usize);
    jit.link_fn("malloc", test_malloc as usize);
    jit.link_fn("free", test_free as usize);
    unsafe { jit.call("main").ok_or(Failure::IR)? }

    let result = mem::replace(&mut *RESULT.lock().unwrap(), String::with_capacity(100));
    let leaked = MALLOC_LIST.lock().unwrap().len();

    if leaked == 0 {
        Ok(result)
    } else {
        Err(Failure::Leak(leaked))
    }
}

fn exec_bin(module: CompiledIR) -> TestRes {
    let mut tmp_file = env::temp_dir();
    tmp_file.push("gelixrs");
    tmp_file.push("test");
    gelixrs::produce_binary(module, tmp_file.as_os_str(), 1).map_err(|_| Failure::IR)?;

    let output = process::Command::new(tmp_file.as_os_str())
        .output()
        .map_err(|_| Failure::Subprocess)?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into())
    } else {
        Err(Failure::Segfault)
    }
}

fn clear_state() {
    MALLOC_LIST.lock().unwrap().clear();
}

fn get_expected_result(mut path: PathBuf) -> TestRes {
    // If the test is a directory, the wanted result is in a file 'expected' in the dir
    if path.is_dir() {
        path.push("expected");
    }

    let code = read_to_string(path).expect("Couldn't get wanted result");
    if code.starts_with("// P-ERR") {
        Err(Failure::Parse(vec![]))
    } else if code.starts_with("// C-ERR") {
        Err(Failure::Compile(vec![]))
    } else if code.starts_with("// LEAK") {
        Err(Failure::Leak(1))
    } else {
        let split = code.split("*/").next().unwrap();
        Ok(split[3..].to_string())
    }
}

fn maybe_compile_stdlib(run: &mut TestRun) -> Result<(), Failure> {
    if run.gir_stdlib.is_none() {
        let code = gelixrs::parse_source(vec![std_mod()]).map_err(Failure::Parse)?;
        let flags = GIRFlags {
            library: true,
            ..GIRFlags::default()
        };
        let gir = gelixrs::compile_gir(code, flags).map_err(Failure::Compile)?;
        run.gir_stdlib = Some(gir);
    }
    Ok(())
}

fn std_mod() -> PathBuf {
    let mut std_mod = PathBuf::from(
        env::current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap(),
    );
    std_mod.push("std");
    std_mod
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
fn catch_unwind_silent<F: FnOnce() -> R, R>(f: F) -> std::thread::Result<R> {
    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let result = panic::catch_unwind(AssertUnwindSafe(f));
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
