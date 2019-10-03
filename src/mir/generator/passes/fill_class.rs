/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/3/19 5:20 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::HashMap;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast::declaration::Class;
use crate::ast::module::Module;
use crate::lexer::token::Token;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::nodes::{MIRClassMember, MIRVariable};

/// This pass fills all classes with their members
/// and creates their internal init function.
pub fn fill_class_pass(gen: &mut MIRGenerator, list: &mut Module) -> Res<()> {
    for class in list.classes.iter_mut() {
        fill_class(gen, class)?;
    }
    Ok(())
}

fn fill_class(gen: &mut MIRGenerator, class: &mut Class) -> Res<()> {
    let mut fields = IndexMap::with_capacity(class.variables.len());

    build_class(gen, class, &mut fields)?;

    let class_rc = gen.builder.find_class(&class.name.lexeme).unwrap();
    let mut class_def = class_rc.borrow_mut();
    check_duplicate(gen, &class.name, &fields, &class_def.methods)?;

    class_def.members = fields;
    Ok(())
}

/// This function will fill the class with its members while also generating the init method.
fn build_class(
    gen: &mut MIRGenerator,
    class: &mut Class,
    fields: &mut IndexMap<Rc<String>, Rc<MIRClassMember>>,
) -> Res<()> {
    let function_rc = gen
        .builder
        .find_function(&format!("{}-internal-init", &class.name.lexeme))
        .unwrap();
    let mut function = function_rc.borrow_mut();
    let class_var = Rc::clone(&function.parameters[0]);
    let entry = function.append_block("entry");
    gen.builder.set_pointer(Rc::clone(&function_rc), entry);
    drop(function);

    let offset = fields.len();
    for (i, field) in class.variables.drain(..).enumerate() {
        let value = gen.generate_expression(&field.initializer)?;
        let member = Rc::new(MIRClassMember {
            mutable: !field.is_val,
            type_: value.get_type(),
            index: (i + offset) as u32,
        });

        let existing_entry = fields.insert(Rc::clone(&field.name.lexeme), Rc::clone(&member));
        if existing_entry.is_some() {
            return Err(MIRGenerator::error(
                gen,
                &field.name,
                &field.name,
                "Class member cannot be defined twice",
            ));
        }

        gen.builder.insert_at_ptr(gen.builder.build_struct_set(
            gen.builder.build_load(Rc::clone(&class_var)),
            member,
            value,
        ));
    }

    Ok(())
}

fn check_duplicate(
    gen: &mut MIRGenerator,
    tok: &Token,
    members: &IndexMap<Rc<String>, Rc<MIRClassMember>>,
    methods: &HashMap<Rc<String>, Rc<MIRVariable>>,
) -> Res<()> {
    for (mem_name, _) in members.iter() {
        if methods.contains_key(mem_name) {
            return Err(MIRGenerator::error(
                gen,
                tok,
                tok,
                &format!(
                    "Cannot have class member and method '{}' with same name.",
                    mem_name
                ),
            ));
        }
    }
    Ok(())
}
