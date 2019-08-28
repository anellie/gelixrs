/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/26/19 6:45 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::ast::declaration::{Class, DeclarationList};
use crate::mir::generator::passes::PreMIRPass;
use crate::mir::generator::{Error, MIRGenerator, Res};
use crate::mir::mir::MIRStructMem;
use std::collections::HashMap;
use std::rc::Rc;

pub struct FillStructPass<'p> {
    gen: &'p mut MIRGenerator,
}

impl<'p> PreMIRPass for FillStructPass<'p> {
    fn run(mut self, list: &mut DeclarationList) -> Res<()> {
        let mut done_classes = Vec::with_capacity(list.classes.len());

        while !list.classes.is_empty() {
            let mut class = list.classes.pop().unwrap();

            // This monster ensures all superclasses are filled first.
            let mut super_tok = class.superclass.clone();
            while super_tok.is_some() {
                let super_name = super_tok.clone().unwrap();
                let class_index = list
                    .classes
                    .iter()
                    .position(|cls| cls.name.lexeme == super_name.lexeme);

                if let Some(class_index) = class_index {
                    let mut superclass = list.classes.remove(class_index);
                    super_tok = superclass.superclass.clone();
                    self.fill_class_struct(&mut superclass)?;
                    done_classes.push(superclass);
                } else {
                    if done_classes
                        .iter()
                        .any(|cls| cls.name.lexeme == super_name.lexeme)
                    {
                        // Superclass was already resolved.
                        super_tok = None;
                    } else {
                        // Superclass doesn't exist.
                        Err(Error {
                            line: Some(super_name.line),
                            message: format!("Unknown class '{}'", super_name.lexeme),
                            code: format!(
                                "class {} ext {} {{ ... }}",
                                class.name.lexeme, super_name.lexeme
                            ),
                        })?;
                    }
                }
            }

            self.fill_class_struct(&mut class)?;
            done_classes.push(class)
        }

        list.classes = done_classes;
        Ok(())
    }
}

impl<'p> FillStructPass<'p> {
    fn fill_class_struct(&mut self, class: &mut Class) -> Result<(), Error> {
        let mut fields = HashMap::with_capacity(class.variables.len());
        let mut fields_vec = Vec::with_capacity(class.variables.len());

        let mut superclass = None;
        if let Some(super_name) = &class.superclass {
            let super_struct = self
                .gen
                .builder
                .find_struct(&super_name.lexeme)
                .ok_or_else(|| {
                    Error::new(
                        Some(super_name.line),
                        "Unknown class",
                        format!(
                            "class {} ext {} {{ ... }}",
                            class.name.lexeme, super_name.lexeme
                        ),
                    )
                })?;

            for member in super_struct.borrow().members.iter() {
                fields.insert(Rc::clone(member.0), Rc::clone(member.1));
                fields_vec.push(Rc::clone(member.1));
            }

            superclass = Some(super_struct);
        }

        self.build_class_init(class, &mut fields, &mut fields_vec)?;

        let class_rc = self.gen.builder.find_struct(&class.name.lexeme).unwrap();
        let mut class_def = class_rc.borrow_mut();
        class_def.members = fields;
        class_def.member_order = fields_vec;
        class_def.super_struct = superclass;

        Ok(())
    }

    fn build_class_init(
        &mut self,
        class: &mut Class,
        fields: &mut HashMap<Rc<String>, Rc<MIRStructMem>>,
        fields_vec: &mut Vec<Rc<MIRStructMem>>,
    ) -> Res<()> {
        let function_rc = self
            .gen
            .builder
            .find_function(&format!("{}-internal-init", &class.name.lexeme))
            .unwrap();
        let mut function = function_rc.borrow_mut();
        let struct_var = Rc::clone(&function.parameters[0]);
        function.append_block("entry".to_string());
        drop(function);
        self.gen
            .builder
            .set_pointer(Rc::clone(&function_rc), Rc::new("entry".to_string()));

        for (i, field) in class.variables.drain(..).enumerate() {
            let value = self.gen.generate_expression(field.initializer)?;
            let member = Rc::new(MIRStructMem {
                mutable: !field.is_val,
                _type: value.get_type(),
                index: i as u32,
            });

            fields.insert(Rc::clone(&field.name.lexeme), Rc::clone(&member));
            fields_vec.push(Rc::clone(&member));

            self.gen
                .builder
                .insert_at_ptr(self.gen.builder.build_struct_set(
                    self.gen.builder.build_load(Rc::clone(&struct_var)),
                    member,
                    value,
                ));
        }

        Ok(())
    }

    pub fn new(gen: &'p mut MIRGenerator) -> FillStructPass<'p> {
        FillStructPass { gen }
    }
}
