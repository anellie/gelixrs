use std::rc::Rc;

use crate::{
    hir::{
        generator::HIRGenerator,
        nodes::declaration::{Declaration, Field, ADT},
    },
    mir::MutRc,
};

impl HIRGenerator {
    pub fn insert_adt_fields(&mut self, decl: Declaration) {
        match decl {
            Declaration::Adt(adt) if adt.borrow().ty.has_members() => self.fill_adt(adt),
            _ => (),
        }
    }

    fn fill_adt(&mut self, adt: MutRc<ADT>) {
        self.build_adt(&adt);
        self.check_duplicate(&adt);
    }

    /// This function will fill the ADT with its members while also generating the init method.
    fn build_adt(&mut self, adt: &MutRc<ADT>) {
        let ast = Rc::clone(&adt.borrow().ast);
        let ast = ast.borrow();

        for field in ast.members().unwrap().iter() {
            let initializer = field.initializer.as_ref().map(|e| self.expression(e));
            let ty = eat!(initializer.as_ref().map_or_else(
                || self.resolver.find_type(field.ty.as_ref().unwrap()),
                |i| Ok(i.get_type()),
            ));

            if !ty.can_escape() {
                self.err(
                    &field.name,
                    "ADT field may not be a weak reference".to_string(),
                );
            }

            let member = Rc::new(Field {
                name: Rc::clone(&field.name.lexeme),
                mutable: field.mutable,
                ty,
                initializer: initializer.map(Box::new),
            });

            let existing_entry = adt
                .borrow_mut()
                .fields
                .insert(Rc::clone(&field.name.lexeme), Rc::clone(&member));
            if existing_entry.is_some() {
                self.err(
                    &field.name,
                    "Class member cannot be defined twice".to_string(),
                );
            }
        }
    }

    fn check_duplicate(&self, adt: &MutRc<ADT>) {
        let adt = adt.borrow();
        for (mem_name, _) in adt.fields.iter() {
            if adt.methods.contains_key(mem_name) {
                self.err(
                    &adt.ast.borrow().name,
                    format!(
                        "Cannot have member and method '{}' with same name.",
                        mem_name
                    ),
                );
            }
        }
    }
}
