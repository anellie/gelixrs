use common::MutRc;
use error::GErr;
use gir_nodes::{declaration::Field, Declaration, ADT};
use std::{cell::RefCell, rc::Rc};

use crate::{eat, GIRGenerator};

impl GIRGenerator {
    pub(super) fn insert_adt_fields(&mut self, decl: Declaration) {
        match decl {
            Declaration::Adt(adt) if adt.borrow().ty.has_members() => self.fill_adt(adt),
            _ => (),
        }
    }

    fn fill_adt(&mut self, adt: MutRc<ADT>) {
        self.build_adt(&adt);
        self.check_duplicate(&adt);
    }

    /// This function will fill the ADT with its members.
    fn build_adt(&mut self, adt: &MutRc<ADT>) {
        let ast = adt.borrow().ast.clone();

        for (index, field) in ast.members().enumerate() {
            let initializer = field.maybe_initializer().map(|e| self.expression(&e));
            let ty = eat!(
                self,
                initializer.as_ref().map_or_else(
                    || self.find_type(&field._type().unwrap()),
                    |i| Ok(i.get_type()),
                )
            );

            if !ty.can_escape() {
                self.err(field.cst(), GErr::E234);
            }

            let member = Rc::new(Field {
                name: field.name(),
                visibility: self.visibility_from_modifiers(field.modifiers()),
                mutable: field.mutable(),
                ty,
                initializer: RefCell::new(initializer.map(Box::new)),
                index,
            });

            let existing_entry = adt
                .borrow_mut()
                .fields
                .insert(field.name(), Rc::clone(&member));
            if existing_entry.is_some() {
                self.err(field.cst(), GErr::E235);
            }
        }
    }

    fn check_duplicate(&self, adt: &MutRc<ADT>) {
        let adt = adt.borrow();
        for (mem_name, _) in adt.fields.iter() {
            if adt.methods.contains_key(mem_name) {
                self.err(adt.ast.name().cst, GErr::E236(mem_name.clone()));
            }
        }
    }
}
