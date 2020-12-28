mod parse_stage;

use ir::IRGenerator;

pub use common::{Benches, BENCH};
pub use error::Errors;
pub use gir_generator::{CompiledGIR, GIRFlags};
pub use ir::{jit::JIT, produce_binary, CompiledIR};
pub use parse_stage::{find_std_module, parse_source, stem_to_smol};

use crate::parse_stage::ParsedModules;
use gir_generator::GIRGenerator;

pub fn compile_gir(ast: ParsedModules, flags: GIRFlags) -> Result<CompiledGIR, Vec<Errors>> {
    GIRGenerator::new(ast, flags).consume()
}

pub fn compile_ir(gir: CompiledGIR) -> CompiledIR {
    IRGenerator::new(gir).generate()
}
