mod parse_stage;

use ir::IRGenerator;

pub use error::Errors;
pub use gir_generator::CompiledGIR;
pub use ir::{jit::JIT, produce_binary, CompiledIR};
pub use parse_stage::{find_std_module, parse_source, stem_to_smol};

use crate::parse_stage::ParsedModules;

pub fn compile_gir(_ast: ParsedModules) -> Result<CompiledGIR, Vec<Errors>> {
    Err(vec![]) // TODO
}

pub fn compile_ir(gir: CompiledGIR) -> CompiledIR {
    IRGenerator::new(gir).generate()
}
