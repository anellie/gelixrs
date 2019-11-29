/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 11/30/19 12:00 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::HashMap;
use std::rc::Rc;

use either::Either::{Left, Right};
use indexmap::IndexMap;

use crate::ast::declaration::Class;
use crate::ast::declaration::Type as ASTType;
use crate::ast::module::Module;
use crate::lexer::token::Token;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::nodes::{ClassMember, Flow, Variable};
use crate::mir::ToMIRResult;

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
        .find_func_or_proto(&format!("create-{}-instance", &class.name.lexeme))
        .unwrap()
        .map_left(|f| f.type_.as_function().clone());

    match &init_func_rc {
        Left(func) => {
            let mut func = func.borrow_mut();
            gen.builder
                .set_pointer(init_func_rc.clone(), func.append_block("entry", false));
        }
        Right(proto) => {
            let mut func = proto.borrow_mut();
            gen.builder
                .set_pointer(init_func_rc.clone(), func.append_block("entry", false));
        }
    };

    let class_variable = Rc::new(Variable {
        mutable: true,
        type_: gen
            .builder
            .find_type(&ASTType::Ident(class.name.clone()))
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
            .unwrap_or_else(|| {
                gen.builder
                    .find_type(field.ty.as_ref().unwrap())
                    .or_type_err(gen, &field.ty, "Unknown class member type")
            })?;

        let member = Rc::new(ClassMember {
            mutable: field.mutable,
            type_,
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

        if let Some(value) = value {
            gen.builder.insert_at_ptr(gen.builder.build_struct_set(
                gen.builder.build_load(Rc::clone(&class_variable)),
                member,
                value,
            ));
        }
    }

    gen.builder.set_return(Flow::Return(gen.builder.build_load(Rc::clone(&class_variable))));
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
