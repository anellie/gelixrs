use crate::{
    gir::{
        generator::{intrinsics::INTRINSICS, module::HIRModuleGenerator},
        nodes::{
            module::Module,
            types::{Instance, Type},
        },
    },
    gir::MutRc,
};

impl HIRModuleGenerator {
    pub fn populate_intrinsics(&mut self, module: MutRc<Module>) {
        let module = module.borrow();
        if **module.path.0[0] == *"std" && **module.path.0[1] == *"ops" {
            // This is the std/ops module, containing all operator interfaces
            INTRINSICS.with(|i| i.borrow_mut().fill_ops_table(module))
        } else if **module.path.0[0] == *"std" && **module.path.0[1] == *"string" {
            // This is std/string, containing the string class
            INTRINSICS.with(|i| {
                i.borrow_mut().string_type = module
                    .find_decl(&"String".to_string())
                    .map(|d| d.into_adt())
                    .map(Instance::new)
                    .map(Type::Value);
            })
        } else if **module.path.0[0] == *"std" && **module.path.0[1] == *"memory" {
            INTRINSICS.with(|i| {
                i.borrow_mut().free_iface =
                    module.find_decl(&"Free".to_string()).map(|d| d.into_adt());
            })
        } else if **module.path.0[0] == *"std" && **module.path.0[1] == *"iter" {
            INTRINSICS.with(|i| {
                i.borrow_mut().iter_proto =
                    module.find_decl(&"Iter".to_string()).map(|d| d.into_adt());
                i.borrow_mut().to_iter_proto = module
                    .find_decl(&"ToIter".to_string())
                    .map(|d| d.into_adt());
            })
        }
    }

    pub fn populate_intrinsics_fn(&mut self, module: MutRc<Module>) {
        let module = module.borrow();
        if **module.path.0[0] == *"std" && **module.path.0[1] == *"intrinsics" {
            INTRINSICS.with(|i| {
                i.borrow_mut().libc_free = module
                    .find_decl(&"free".to_string())
                    .map(|d| d.into_function());
            })
        }
    }
}
