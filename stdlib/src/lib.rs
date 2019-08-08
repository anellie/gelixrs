#[no_mangle]
pub extern fn print(x: i64) -> bool {
    println!("{}", x);
    true
}