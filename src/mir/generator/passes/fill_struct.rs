/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/11/19, 7:14 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::ast::declaration::Class;
use crate::ast::module::Module;
use crate::lexer::token::Token;
use crate::mir::generator::passes::PreMIRPass;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::nodes::{MIRExpression, MIRFunction, MIRStructMem};
use crate::mir::MutRc;
use std::collections::HashMap;
use std::rc::Rc;

/// This pass fills all structs with their members
/// and creates their internal init function.
pub struct FillStructPass<'p> {
    gen: &'p mut MIRGenerator,
}

impl<'p> PreMIRPass for FillStructPass<'p> {
    fn run(mut self, list: &mut Module) -> Res<()> {
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
                } else if done_classes
                    .iter()
                    .any(|cls| cls.name.lexeme == super_name.lexeme)
                {
                    // Superclass was already resolved.
                    super_tok = None;
                } else {
                    // Superclass doesn't exist.
                    return Err(MIRGenerator::error(
                        self.gen.builder.module_path(), 
                        &super_name,
                        &super_name,
                        &format!("Unknown class '{}'", super_name.lexeme),
                    ));
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
    fn fill_class_struct(&mut self, class: &mut Class) -> Res<()> {
        let mut fields = HashMap::with_capacity(class.variables.len());
        let mut fields_vec = Vec::with_capacity(class.variables.len());

        let mut superclass = None;
        if let Some(super_name) = &class.superclass {
            let super_struct = self
                .gen
                .builder
                .find_struct(&super_name.lexeme)
                .ok_or_else(|| MIRGenerator::error(self.gen.builder.module_path(), super_name, super_name, "Unknown class"))?;

            for member in super_struct.borrow().members.iter() {
                fields.insert(Rc::clone(member.0), Rc::clone(member.1));
                fields_vec.push(Rc::clone(member.1));
            }

            superclass = Some(super_struct);
        }

        self.build_class_init(class, &mut fields, &mut fields_vec)?;

        let class_rc = self.gen.builder.find_struct(&class.name.lexeme).unwrap();
        let mut class_def = class_rc.borrow_mut();
        self.check_duplicate(&class.name, &fields, &class_def.methods)?;
        class_def.members = fields;
        class_def.member_order = fields_vec;
        class_def.super_struct = superclass;
        Ok(())
    }

    fn check_duplicate(
        &self,
        tok: &Token,
        members: &HashMap<Rc<String>, Rc<MIRStructMem>>,
        methods: &HashMap<Rc<String>, MutRc<MIRFunction>>,
    ) -> Res<()> {
        for (mem_name, _) in members.iter() {
            if methods.contains_key(mem_name) {
                return Err(MIRGenerator::error(
                    self.gen.builder.module_path(), 
                    tok,
                    tok,
                    &format!(
                        "Cannot have class member and method '{}' with same name ",
                        mem_name
                    ),
                ));
            }
        }
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

        let offset = fields.len();
        for (i, field) in class.variables.drain(..).enumerate() {
            let value = self.gen.generate_expression(&field.initializer)?;
            let member = Rc::new(MIRStructMem {
                mutable: !field.is_val,
                _type: value.get_type(),
                index: (i + offset) as u32,
            });

            let existing_entry = fields.insert(Rc::clone(&field.name.lexeme), Rc::clone(&member));
            fields_vec.push(Rc::clone(&member));

            if existing_entry.is_some() {
                return Err(MIRGenerator::error(
                    self.gen.builder.module_path(),
                    &field.name,
                    &field.name,
                    "Class member cannot be defined twice",
                ));
            }

            self.gen
                .builder
                .insert_at_ptr(self.gen.builder.build_struct_set(
                    self.gen.builder.build_load(Rc::clone(&struct_var)),
                    member,
                    value,
                ));
        }

        if let Some(sclass) = &class.superclass {
            let sclass_def = self.gen.builder.find_struct(&sclass.lexeme).unwrap();
            let function_rc = self
                .gen
                .builder
                .find_function(&format!("{}-internal-init", &sclass.lexeme))
                .unwrap();

            let super_init_call = self.gen.builder.build_call(
                MIRExpression::Function(function_rc),
                vec![self
                    .gen
                    .builder
                    .build_bitcast(MIRExpression::VarGet(Rc::clone(&struct_var)), &sclass_def)],
            );

            self.gen.builder.insert_at_ptr(super_init_call);
        }

        Ok(())
    }

    pub fn new(gen: &'p mut MIRGenerator) -> FillStructPass<'p> {
        FillStructPass { gen }
    }
}
