mod parse_stage;

use ir::IRGenerator;

pub use error::Errors;
pub use gir_generator::CompiledGIR;
pub use ir::{jit::JIT, produce_binary, CompiledIR};
pub use parse_stage::{find_std_module, parse_source, stem_to_smol};

use crate::parse_stage::ParsedModules;
use gir_generator::GIRGenerator;

pub fn compile_gir(ast: ParsedModules) -> Result<CompiledGIR, Vec<Errors>> {
    GIRGenerator::new(ast).consume()
}

pub fn compile_ir(gir: CompiledGIR) -> CompiledIR {
    IRGenerator::new(gir).generate()
}
