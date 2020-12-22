use inkwell::{
    execution_engine::{ExecutionEngine, JitFunction},
    module::Module,
    OptimizationLevel,
};

type SimpleFn = JitFunction<unsafe extern "C" fn()>;

pub struct JIT {
    module: Module,
    engine: ExecutionEngine,
}

impl JIT {
    /// Calls a function inside the module this JIT is inside.
    /// # Safety
    /// Since the called function can perform unsafe behavior, calling
    /// it is unsafe.
    pub unsafe fn call(&mut self, name: &str) {
        let func: SimpleFn = self
            .engine
            .get_function(name)
            .expect("Unknown JIT function");
        func.call();
    }

    pub fn link_fn(&mut self, name: &str, address: usize) {
        if let Some(fun) = &self.module.get_function(name) {
            self.engine.add_global_mapping(fun, address);
        }
    }

    pub fn new(module: Module) -> JIT {
        JIT {
            engine: module
                .create_jit_execution_engine(OptimizationLevel::Default)
                .expect("Failed to create JIT"),
            module,
        }
    }
}
