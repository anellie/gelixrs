mod parse_stage;

pub use error::Errors;
pub use ir::jit::JIT;
pub use parser::ParseResult;

pub use parse_stage::{parse_source, find_std_module};

pub fn clear_compiler_state() {
    // INTRINSICS.with(|i| i.replace(Intrinsics::default()));
    // IFACE_IMPLS.with(|i| i.replace(HashMap::default()));
}
