use std::ffi::CStr;

#[no_mangle]
pub extern fn print(string: *const i8) -> bool {
    let string = unsafe { CStr::from_ptr(string) };
    println!("{}", string.to_str().unwrap());
    true
}

#[no_mangle]
pub extern fn printnum(int: i64) -> i64 {
    println!("{}", int);
    int
}