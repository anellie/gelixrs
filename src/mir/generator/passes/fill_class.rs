/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/13/19 10:16 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::HashMap;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast::declaration::Class;
use crate::ast::declaration::Type as ASTType;
use crate::ast::module::Module;
use crate::lexer::token::Token;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::nodes::{ClassMember, Flow, Variable};

/// This pass fills all classes with their members
/// and creates their internal init function.
pub fn fill_class_pass(gen: &mut MIRGenerator, list: &mut Module) -> Res<()> {
    for class in list.classes.iter_mut() {
        fill_class(gen, class)?;
    }
    Ok(())
}

fn fill_class(gen: &mut MIRGenerator, class: &mut Class) -> Res<()> {
    let class_rc = gen.builder.find_class(&class.name.lexeme).unwrap();
    let mut class_mir = class_rc.borrow_mut();
    build_class(gen, class, &mut class_mir.members)?;
    check_duplicate(gen, &class.name, &class_mir.members, &class_mir.methods)
}

/// This function will fill the class with its members while also generating the init method.
fn build_class(
    gen: &mut MIRGenerator,
    class: &mut Class,
    fields: &mut IndexMap<Rc<String>, Rc<ClassMember>>,
) -> Res<()> {
    let init_func_rc = gen
        .builder
        .find_function(&format!("create-{}-instance", &class.name.lexeme))
        .unwrap();

    {
        let mut func = init_func_rc.borrow_mut();
        gen.builder
            .set_pointer(init_func_rc.clone(), func.append_block("entry", false));
    }

    let class_variable = Rc::new(Variable {
        mutable: true,
        type_: gen
            .find_type(&ASTType::Ident(class.name.clone()))
            .ok()
            .unwrap(),
        name: Rc::new("this".to_string()),
    });
    gen.builder
        .add_function_variable(Rc::clone(&class_variable));

    let offset = fields.len();
    for (i, field) in class.variables.drain(..).enumerate() {
        let value = field
            .initializer
            .as_ref()
            .map(|e| gen.generate_expression(e));
        let value = match value {
            Some(v) => Some(v?),
            None => None,
        };
        let type_ = value
            .as_ref()
            .map(|v| Ok(v.get_type()))
            .unwrap_or_else(|| gen.find_type(field.ty.as_ref().unwrap()))?;

        let member = Rc::new(ClassMember {
            mutable: field.mutable,
            type_,
            index: (i + offset) as u32,
            has_default_value: field.initializer.is_some(),
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

        if let Some(value) = value {
            gen.builder.insert_at_ptr(gen.builder.build_struct_set(
                gen.builder.build_load(Rc::clone(&class_variable)),
                member,
                value,
            ));
        }
    }

    gen.builder.set_return(Flow::Return(
        gen.builder.build_load(Rc::clone(&class_variable)),
    ));
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
