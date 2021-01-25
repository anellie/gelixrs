use std::{mem, rc::Rc};

use common::{ModPath, MutRc};
use error::{GErr, Res};
use gir_nodes::{
    module::{Imports, UnresolvedImport},
    Module,
};

use crate::{eatc, result::EmitGIRError, GIRGenerator};
use ast::CSTNode;

impl GIRGenerator {
    pub(super) fn import_stage_1(&mut self, module: MutRc<Module>) {
        let ast_borrow = {
            let mut module = module.borrow_mut();
            module.borrow_ast()
        };
        let ast = &ast_borrow.0;

        for import in ast.imports() {
            let mut path = import.parts().collect::<Vec<_>>();
            let symbol = path.pop().unwrap();
            let path = ModPath::from(path);

            let src_module_rc = eatc!(self, self.find_module(&path, &import));
            let src_module = src_module_rc.borrow();

            if symbol == "+" {
                Self::get_imports(&mut module.borrow_mut(), import.is_export())
                    .modules
                    .push(src_module_rc.clone());
            } else {
                let decl = src_module.find_import(&symbol);
                if let Some(decl) = decl {
                    self.try_reserve_name(&import.cst, &symbol);
                    Self::get_imports(&mut module.borrow_mut(), import.is_export())
                        .decls
                        .insert(symbol, decl);
                    continue;
                }
            }

            module
                .borrow_mut()
                .imports
                .unresolved
                .push(UnresolvedImport {
                    ast: import,
                    module: Rc::clone(&src_module_rc),
                    symbol,
                })
        }

        let mut module = module.borrow_mut();
        if !self.flags.no_prelude && !module.path.is(&["std", "prelude"]) {
            module
                .imports
                .modules
                .push(self.intrinsics.std_prelude.clone().unwrap());
        }
        module.return_ast(ast_borrow);
    }

    pub(super) fn import_stage_2(&mut self, module: MutRc<Module>) {
        let remaining_imports = mem::replace(&mut module.borrow_mut().imports.unresolved, vec![]);
        for import in remaining_imports {
            let src_module = import.module.borrow();

            if import.symbol == "+" {
                for name in src_module.declarations.keys() {
                    self.try_reserve_name(&import.ast.cst, name);
                }
            } else {
                let decl = src_module.find_import(&import.symbol);
                if let Some(decl) = decl {
                    self.try_reserve_name(&import.ast.cst, &import.symbol);
                    Self::get_imports(&mut module.borrow_mut(), import.ast.is_export())
                        .decls
                        .insert(import.symbol, decl);
                } else {
                    self.err(import.ast.cst(), GErr::E103);
                }
            }
        }

        if !self.flags.no_prelude && !module.borrow().path.is(&["std", "prelude"]) {
            let dummy = CSTNode::dummy();
            for name in self
                .intrinsics
                .std_prelude
                .as_ref()
                .unwrap()
                .borrow()
                .declarations
                .keys()
            {
                self.try_reserve_name(&dummy, name);
            }
        }
    }

    fn find_module(&self, path: &ModPath, import: &ast::Import) -> Res<&MutRc<Module>> {
        self.modules
            .iter()
            .find(|m| {
                m.try_borrow()
                    .ok()
                    .map(|m| *m.path == *path)
                    .unwrap_or(false)
            })
            .or_err(&import.cst, GErr::E102)
    }

    fn get_imports(module: &mut Module, is_export: bool) -> &mut Imports {
        if is_export {
            &mut module.exports
        } else {
            &mut module.imports
        }
    }
}
