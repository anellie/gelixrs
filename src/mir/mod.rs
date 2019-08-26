/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/26/19 5:53 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/24/19 5:49 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use mir::{MIRStruct, MIRFunction};
use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;

pub mod generator;
pub mod mir;

type MutRc<T> = Rc<RefCell<T>>;

fn mutrc_new<T>(value: T) -> MutRc<T> {
    Rc::new(RefCell::new(value))
}

/// A struct produced by the generator. It contains the full MIR representation of the source.
pub struct MIR {
    pub types: Vec<MutRc<MIRStruct>>,
    pub functions: HashMap<Rc<String>, MutRc<MIRFunction>>,
}