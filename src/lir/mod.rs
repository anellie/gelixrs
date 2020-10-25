use crate::lir::types::{Function, ADT};
use crate::mir::MutRc;

pub mod expr;
pub mod types;

pub struct LModule {
    pub functions: Vec<MutRc<Function>>,
    pub adts: Vec<MutRc<ADT>>
}
