use common::MutRc;
use error::GErr;
use gir_nodes::{ADT, declaration::{ADTType, Field}};
use std::{cell::RefCell, rc::Rc};

use crate::{eat, GIRGenerator};

impl GIRGenerator {
    pub(super) fn insert_adt_fields(&mut self, adt: &MutRc<ADT>) {
        let mut adt = adt.borrow_mut();
        match &adt.ty {
            ADTType::Class { .. } => self.fill_adt(&mut adt),

            ADTType::Enum { cases } => {
                let cases = Rc::clone(cases);
                self.fill_adt(&mut adt);
                for case in cases.values() {
                    case.borrow_mut().fields = adt.fields.clone();
                    self.fill_adt(&mut case.borrow_mut());
                }
            }

            _ => ()
        }
    }

    fn fill_adt(&mut self, adt: &mut ADT) {
        self.build_adt(adt);
        self.check_duplicate(adt);
    }

    /// This function will fill the ADT with its members.
    fn build_adt(&mut self, adt: &mut ADT) {
        let ast = adt.ast.clone();
        let offset = adt.fields.len(); // For enum cases which already contain fields

        for (index, field) in ast.members().enumerate() {
            let index = offset + index;
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
                initialized: initializer.is_some(),
                initializer: RefCell::new(initializer),
                index,
            });

            let existing_entry = adt
                .fields
                .insert(field.name(), Rc::clone(&member));
            if existing_entry.is_some() {
                self.err(field.cst(), GErr::E235);
            }
        }
    }

    fn check_duplicate(&self, adt: &ADT) {
        for (mem_name, _) in adt.fields.iter() {
            if adt.methods.contains_key(mem_name) {
                self.err(adt.ast.name().cst, GErr::E236(mem_name.clone()));
            }
        }
    }
}
