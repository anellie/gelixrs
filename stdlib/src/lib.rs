use std::io;
use std::ffi::CStr;

#[no_mangle]
pub extern fn print(string: *const i8) {
    let string = unsafe { CStr::from_ptr(string) };
    println!("{}", string.to_str().unwrap());
}

#[no_mangle]
pub extern fn printnum(int: i64) {
    println!("{}", int);
}

// TODO: This does not work.
// I think its because Rust deallocates the pointer before returning?
#[no_mangle]
pub extern fn read_line() -> *const u8 {
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.as_ptr()
}
