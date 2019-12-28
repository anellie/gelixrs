/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/28/19 1:12 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::{
    error::Res,
    mir::{
        generator::{
            passes::{ModulePass, PassType},
            MIRGenerator,
        },
        nodes::{Class, ClassMember, Expr, Type},
        MutRc,
    },
};

/// This pass fills all classes with their members
/// and creates their internal init function.
pub struct InsertClassMembers();

impl ModulePass for InsertClassMembers {
    fn get_type(&self) -> PassType {
        PassType::Type
    }

    fn run_type(&self, gen: &mut MIRGenerator, ty: Type) -> Res<()> {
        if let Type::Class(cls) = ty {
            fill_class(gen, cls)?
        }
        Ok(())
    }
}

fn fill_class(gen: &mut MIRGenerator, class: MutRc<Class>) -> Res<()> {
    build_class(gen, &class)?;
    check_duplicate(gen, &class)
}

/// This function will fill the class with its members while also generating the init method.
fn build_class(gen: &mut MIRGenerator, class: &MutRc<Class>) -> Res<()> {
    let ast = Rc::clone(&class.borrow().ast);
    let class_variable = {
        let inst = class.borrow().instantiator.type_.as_function().clone();
        let mut func = inst.borrow_mut();
        gen.set_pointer(inst.clone(), func.append_block("entry", false));
        Rc::clone(&func.parameters[0])
    };

    let offset = class.borrow().members.len();
    for (i, field) in ast.variables.iter().enumerate() {
        let value = field.initializer.as_ref().map(|e| gen.expression(e));
        let value = match value {
            Some(v) => Some(v?),
            None => None,
        };
        let type_ = value
            .as_ref()
            .map(|v| Ok(v.get_type()))
            .unwrap_or_else(|| gen.builder.find_type(field.ty.as_ref().unwrap()))?;

        let member = Rc::new(ClassMember {
            mutable: field.mutable,
            type_,
            index: i + offset,
            has_default_value: field.initializer.is_some(),
        });

        let existing_entry = class
            .borrow_mut()
            .members
            .insert(Rc::clone(&field.name.lexeme), Rc::clone(&member));
        if existing_entry.is_some() {
            return Err(gen.err(&field.name, "Class member cannot be defined twice"));
        }

        if let Some(value) = value {
            gen.insert_at_ptr(Expr::struct_set(Expr::load(&class_variable), member, value));
        }
    }

    gen.insert_at_ptr(Expr::ret(Expr::load(&class_variable)));
    Ok(())
}

fn check_duplicate(gen: &mut MIRGenerator, class: &MutRc<Class>) -> Res<()> {
    let class = class.borrow();
    for (mem_name, _) in class.members.iter() {
        if class.methods.contains_key(mem_name) {
            return Err(gen.err(
                &class.ast.name,
                &format!(
                    "Cannot have class member and method '{}' with same name.",
                    mem_name
                ),
            ));
        }
    }
    Ok(())
}
