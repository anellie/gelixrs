use std::{
    env::current_dir, 
    ffi::CStr,
    fs::read_to_string, 
    path::PathBuf,
    sync::Mutex,
    os::raw::c_char
};
use inkwell::{
    OptimizationLevel,
    execution_engine::JitFunction
};

type MainFn = unsafe extern "C" fn();

lazy_static! {
    static ref RESULT: Mutex<String> = Mutex::new(String::from(""));
}

#[no_mangle]
extern fn test_print(string: *const c_char) {
    let string = unsafe { CStr::from_ptr(string) };
    RESULT.lock().unwrap().push_str(&format!("{}\n", string.to_str().unwrap()));
}
#[no_mangle]
extern fn test_printnum(num: i64) {
    RESULT.lock().unwrap().push_str(&format!("{}\n", num));
}

#[test]
fn run_all() -> Result<(), ()> {
    let mut test_path = current_dir().expect("Couldn't get current dir.");
    test_path.push("tests");
    for_file(test_path)?;
    Ok(())
}

fn for_file(file: PathBuf) -> Result<(), ()> {
    if file.is_dir() {
        for file in file.read_dir().expect("Couldn't get current dir.") {
            for_file(file.expect("Couldn't get file.").path())?;
        }
    } else {
        println!("Running test: {}", file.to_str().unwrap());
        let source = read_to_string(file).expect("Couldn't get file.");
        let expected = get_expected_result(&source);
        let source = format!("exfn print(String a)\nexfn printnum(i64 a)\n{}", source);
        let result = exec_jit(source);

        if result != expected {
            println!("\n\n\nTest failed!\nResult: {:?}\nExpected: {:?}", result, expected);
        }
        println!("\n", );
    }

    Ok(())
}

fn exec_jit(source: String) -> Result<String, ()> {
    let code = super::parse_source(&source).ok_or(())?;
    let module = super::compile_ir(code).ok_or(())?;

    let engine = module.create_jit_execution_engine(OptimizationLevel::None).ok().ok_or(())?;
    engine.add_global_mapping(&module.get_function("print").ok_or(())?, test_print as usize);
    engine.add_global_mapping(&module.get_function("printnum").ok_or(())?, test_printnum as usize);

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