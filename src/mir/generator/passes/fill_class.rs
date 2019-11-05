/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 11/5/19 9:14 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::HashMap;
use std::rc::Rc;

use either::Either::{Left, Right};
use indexmap::IndexMap;

use crate::ast::declaration::Class;
use crate::ast::module::Module;
use crate::lexer::token::Token;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::nodes::ClassMember;

/// This pass fills all classes with their members
/// and creates their internal init function.
pub fn fill_class_pass(gen: &mut MIRGenerator, list: &mut Module) -> Res<()> {
    for class in list.classes.iter_mut() {
        fill_class(gen, class)?;
    }
    Ok(())
}

fn fill_class(gen: &mut MIRGenerator, class: &mut Class) -> Res<()> {
    let class_rc = gen.builder.find_class_or_proto(&class.name.lexeme).unwrap();

    match class_rc {
        Left(class_rc) => {
            let mut class_mir = class_rc.borrow_mut();
            build_class(gen, class, &mut class_mir.members)?;
            check_duplicate(gen, &class.name, &class_mir.members, &class_mir.methods)
        }

        Right(proto) => {
            let mut proto = proto.borrow_mut();
            gen.builder.set_generic_types_rc(&proto.generic_args);

            build_class(gen, class, &mut proto.members)?;
            check_duplicate(gen, &class.name, &proto.members, &proto.methods)?;

            gen.builder.generic_types.clear();
            Ok(())
        }
    }
}

/// This function will fill the class with its members while also generating the init method.
fn build_class(
    gen: &mut MIRGenerator,
    class: &mut Class,
    fields: &mut IndexMap<Rc<String>, Rc<ClassMember>>,
) -> Res<()> {
    let init_func_rc = gen
        .builder
        .find_function(&format!("{}-internal-init", &class.name.lexeme))
        .unwrap();
    let mut init_func = init_func_rc.borrow_mut();
    let class_parameter = Rc::clone(&init_func.parameters[0]);
    gen.builder.set_pointer(
        Left(Rc::clone(&init_func_rc)),
        init_func.append_block("entry"),
    );
    drop(init_func);

    let offset = fields.len();
    for (i, field) in class.variables.drain(..).enumerate() {
        let value = gen.generate_expression(&field.initializer)?;
        let member = Rc::new(ClassMember {
            mutable: field.mutable,
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
            gen.builder.build_load(Rc::clone(&class_parameter)),
            member,
            value,
        ));
    }

    Ok(())
}

fn check_duplicate<T>(
    gen: &mut MIRGenerator,
    tok: &Token,
    members: &IndexMap<Rc<String>, Rc<ClassMember>>,
    methods: &HashMap<Rc<String>, T>,
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
