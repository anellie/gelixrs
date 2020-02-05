/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 1/26/20 10:42 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::{
    error::Res,
    mir::{
        generator::{
            intrinsics::INTRINSICS,
            passes::{ModulePass, PassType},
            MIRGenerator,
        },
        get_iface_impls,
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
    build_destructor(gen, &class);
    check_duplicate(gen, &class)
}

/// This function will fill the class with its members while also generating the init method.
fn build_class(gen: &mut MIRGenerator, class: &MutRc<Class>) -> Res<()> {
    let ast = Rc::clone(&class.borrow().ast);
    let class_variable = {
        let inst = class.borrow().instantiator.type_.as_function().clone();
        gen.set_pointer(inst.clone());
        let inst = inst.borrow();
        Rc::clone(&inst.parameters[0])
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
            gen.insert_at_ptr(Expr::StructSet {
                object: Box::new(Expr::load(&class_variable)),
                index: member.index,
                value: Box::new(value),
                first_set: true,
            });
        }
    }

    // Insert at the end of the instanciator to prevent
    // an edge case of an empty instanciator, which IR would interpret
    // incorrectly as an external function
    gen.insert_at_ptr(Expr::none_const());

    Ok(())
}

fn build_destructor(gen: &mut MIRGenerator, class: &MutRc<Class>) {
    let (class_variable, dealloc_var) = {
        let dest = class.borrow().destructor.type_.as_function().clone();
        let func = dest.borrow();
        gen.set_pointer(dest.clone());
        (
            Rc::clone(&func.parameters[0]),
            Rc::clone(&func.parameters[1]),
        )
    };

    let mut if_free_exprs = Vec::with_capacity(class.borrow().members.len() + 3);
    if_free_exprs.push(Expr::mod_rc(Expr::load(&class_variable), false));

    let free_iface = INTRINSICS.with(|i| i.borrow().free_iface.clone()).unwrap();
    let free_method = get_iface_impls(&Type::Class(Rc::clone(class)))
        .map(|impls| {
            impls
                .borrow()
                .interfaces
                .get(&free_iface)
                .map(|iface| Rc::clone(iface.methods.get_index(0).unwrap().1))
        })
        .flatten();
    if let Some(method) = free_method {
        if_free_exprs.push(Expr::call(
            Expr::load(&method),
            vec![Expr::load(&class_variable)],
        ));
    }

    let class = class.borrow();
    for field in class.members.values() {
        if_free_exprs.push(Expr::mod_rc(
            Expr::struct_get(Expr::load(&class_variable), field),
            true,
        ));
    }
    if_free_exprs.push(Expr::Free(Box::new(Expr::load(&class_variable))));

    gen.insert_at_ptr(Expr::if_(
        Expr::load(&dealloc_var),
        Expr::Block(if_free_exprs),
        Expr::none_const(),
        false,
    ));
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
