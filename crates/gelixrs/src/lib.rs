mod parse_stage;

use ir::IRGenerator;

pub use common::{Benches, BENCH};
pub use error::Errors;
pub use gir_generator::{CompiledGIR, GIRFlags};
pub use ir::{ir_context, jit::JIT, produce_binary, CompiledIR, Context};
pub use parse_stage::{find_std_module, parse_source, stem_to_smol};

use crate::parse_stage::ParsedModules;
use gir_generator::GIRGenerator;

pub fn compile_gir(ast: ParsedModules, flags: GIRFlags) -> Result<CompiledGIR, Vec<Errors>> {
    GIRGenerator::new(ast, flags).consume()
}

pub fn compile_gir_cached_std(
    ast: ParsedModules,
    std: &CompiledGIR,
    flags: GIRFlags,
) -> Result<CompiledGIR, Vec<Errors>> {
    GIRGenerator::with_cached_std(ast, std, flags).consume()
}

pub fn compile_ir(context: Context, gir: CompiledGIR) -> CompiledIR {
    IRGenerator::new(context, gir).generate()
}
