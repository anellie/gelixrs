/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/20/19 7:18 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::io;
use std::ffi::CStr;

#[no_mangle]
pub extern fn printnum(int: i64) {
    println!("{}", int);
}
