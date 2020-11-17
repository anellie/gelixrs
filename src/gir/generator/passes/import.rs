use std::{cell::RefMut, rc::Rc};

use crate::{
    ast::{module::ModulePath, Import},
    error::{Error, Res},
    gir::{
        generator::module::GIRModuleGenerator,
        nodes::module::{Imports, Module},
        result::ToGIRResult,
        MutRc,
    },
    lexer::token::TType,
};

impl GIRModuleGenerator {
    pub fn imports(&mut self, second_stage: bool) {
        self.run_mod(|gen, module| {
            gen.drain_mod_imports(module, |this, module, import, is_export| {
                let src_module_rc = gen.find_module(&module.path, &import)?;
                let src_module = src_module_rc.borrow();

                if import.symbol.t_type == TType::Plus {
                    if second_stage {
                        for name in src_module.declarations.keys() {
                            module.try_reserve_name_rc(&this.generator, name, &import.symbol);
                        }
                    } else {
                        Self::get_imports(module, is_export).modules.push(src_module_rc.clone());
                    }
                    Ok(second_stage)
                } else {
                    let decl = src_module.find_import(&import.symbol.lexeme);

                    if let Some(decl) = decl {
                        module.try_reserve_name(&this.generator, &import.symbol);
                        Self::get_imports(module, is_export).decls.insert(import.symbol.lexeme.clone(), decl);
                    } else if second_stage {
                        gen.err(&import.symbol, "Unknown declaration.".to_string())
                    } else {
                        return Ok(false)
                    }

                    Ok(true)
                }
            })
        })
    }

    /// This function runs `drain_filter` on all imports in the given module, using the given function as a filter.
    fn drain_mod_imports<T: FnMut(&Self, &mut RefMut<Module>, &Import, bool) -> Res<bool>>(
        &self,
        target: MutRc<Module>,
        mut cond: T,
    ) {
        let mut module = target.borrow_mut();
        let mut ast = module.borrow_ast();
        ast.imports.drain_filter(|im| {
            cond(self, &mut module, im, false).unwrap_or_else(|e| self.import_err(e, &module))
        });
        ast.exports.drain_filter(|im| {
            cond(self, &mut module, im, true).unwrap_or_else(|e| self.import_err(e, &module))
        });
        module.return_ast(ast);
    }

    fn find_module(&self, path: &Rc<ModulePath>, import: &Import) -> Res<&MutRc<Module>> {
        self.modules
            .iter()
            .find(|m| {
                m.try_borrow().ok().map(|m| Rc::clone(&m.path)) == Some(Rc::clone(&import.path))
            })
            .or_err(path, &import.symbol, "Unknown module.")
    }

    fn get_imports(module: &mut Module, is_export: bool) -> &mut Imports {
        if is_export {
            &mut module.exports
        } else {
            &mut module.imports
        }
    }

    fn import_err(&self, err: Error, module: &Module) -> bool {
        self.generator.error_(err, module);
        true
    }
}
