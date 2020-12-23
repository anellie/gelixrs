mod parse_stage;

pub use error::Errors;
pub use gir_generator::CompiledGIR;
pub use ir::{jit::JIT, CompiledIR, produce_binary};
pub use parse_stage::{find_std_module, parse_source, stem_to_smol};

use crate::parse_stage::ParsedModules;

pub fn clear_compiler_state() {
    // INTRINSICS.with(|i| i.replace(Intrinsics::default()));
    // IFACE_IMPLS.with(|i| i.replace(HashMap::default()));
}

pub fn compile_gir(_ast: ParsedModules) -> Result<CompiledGIR, Vec<Errors>> {
    Err(vec![]) // TODO
}

pub fn compile_ir(_gir: CompiledGIR) -> CompiledIR {
    todo!()
}
