/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/13/19 10:11 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::HashSet;
use std::rc::Rc;

use crate::ast::declaration::{Class as ASTClass, Constructor, FuncSignature, FunctionArg, Visibility};
use crate::ast::Expression as ASTExpr;
use crate::ast::module::Module;
use crate::ast::Type;
use crate::lexer::token::Token;
use crate::mir::{mutrc_new, ToMIRResult};
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::generator::passes::declare_func::create_function;
use crate::mir::nodes::{Block, Class, ClassPrototype, Expression, Type as MType, Variable};
use crate::option::Flatten;

/// This pass declares all classes.
/// It does not fill them; they are kept empty.
pub fn declare_class_pass(gen: &mut MIRGenerator, module: &mut Module) -> Res<()> {
    for class in module.classes.iter_mut() {
        create_class(gen, class)?;
    }

    Ok(())
}

fn create_class(gen: &mut MIRGenerator, class: &mut ASTClass) -> Res<()> {
    gen.builder.try_reserve_name(&class.name)?;
    if let Some(generics) = &mut class.generics {
        generics.push(class.name.clone()); // The class itself is treated as a generic parameter that is substituted
    }

    let init_fn_sig = get_instantiator_fn_sig(class);
    let this_arg = FunctionArg::this_arg(&class.name);
    maybe_add_default_constructor(class);

    // If the AST class has generic parameters, it must be compiled to
    // a class prototype instead of an actual class.
    if let Some(generics) = &class.generics {
        gen.builder.set_generic_types(generics);

        let mir_class = mutrc_new(ClassPrototype {
            name: Rc::clone(&class.name.lexeme),
            generic_args: gen.builder.generic_types.to_vec(),
            ..Default::default()
        });
        gen.builder
            .prototypes
            .classes
            .insert(Rc::clone(&class.name.lexeme), Rc::clone(&mir_class));
        let mut mir_class = mir_class.borrow_mut();

        mir_class.instantiator = create_function(gen, &init_fn_sig, false, Some(generics))?
            .right()
            .unwrap();

        // Do all user-defined methods
        for method in class.methods.iter_mut() {
            let method_name = modify_method_sig(&class.name.lexeme, &mut method.sig, &this_arg);
            let mir_method = create_function(gen, &method.sig, false, Some(generics))?
                .right()
                .unwrap();
            mir_class.methods.insert(method_name, mir_method);
        }

        let mut constructor_list = HashSet::with_capacity(class.constructors.len());
        for (i, constructor) in class.constructors.iter().enumerate() {
            let sig = get_constructor_sig(gen, &class, constructor, &this_arg, i)?;
            let mir_rc = create_function(gen, &sig, false, Some(generics))?.right().unwrap();
            let mut mir_fn = mir_rc.borrow_mut();
            let block = insert_constructor_setters(gen, &class, constructor, &mir_fn.parameters)?;
            mir_fn.blocks.insert(Rc::new("entry".to_string()), block);
            mir_class.constructors.push(Rc::clone(&mir_rc));

            let params = mir_fn.parameters.iter().skip(1).map(|p| &p.type_).cloned().collect::<Vec<MType>>();
            if !constructor_list.insert(params) {
                return Err(gen.error(&class.name, &class.name, "Class contains constructors with duplicate signatures."))
            }
        }
    } else {
        let mir_class = mutrc_new(Class {
            name: Rc::clone(&class.name.lexeme),
            ..Default::default()
        });
        gen.builder
            .module
            .classes
            .insert(Rc::clone(&class.name.lexeme), Rc::clone(&mir_class));
        let mut mir_class = mir_class.borrow_mut();

        mir_class.instantiator = create_function(gen, &init_fn_sig, false, None)?
            .left()
            .unwrap();

        // Do all user-defined methods
        for method in class.methods.iter_mut() {
            let method_name = modify_method_sig(&class.name.lexeme, &mut method.sig, &this_arg);
            let mir_method = create_function(gen, &method.sig, false, None)?
                .left()
                .unwrap();
            mir_class.methods.insert(method_name, mir_method);
        }

        let mut constructor_list = HashSet::with_capacity(class.constructors.len());
        for (i, constructor) in class.constructors.iter().enumerate() {
            let sig = get_constructor_sig(gen, &class, constructor, &this_arg, i)?;
            let mir_var = create_function(gen, &sig, false, None)?.left().unwrap();
            let mir_fn = mir_var.type_.as_function();
            let mut mir_fn = mir_fn.borrow_mut();
            let block = insert_constructor_setters(gen, &class, constructor, &mir_fn.parameters)?;
            mir_fn.blocks.insert(Rc::new("entry".to_string()), block);
            mir_class.constructors.push(Rc::clone(&mir_var));

            let params = mir_fn.parameters.iter().skip(1).map(|p| &p.type_).cloned().collect::<Vec<MType>>();
            if !constructor_list.insert(params) {
                return Err(gen.error(&class.name, &class.name, "Class contains constructors with duplicate signatures."))
            }
        }
    }

    gen.builder.generic_types.clear();
    Ok(())
}

/// Returns signature of the class instantiator.
fn get_instantiator_fn_sig(class: &mut ASTClass) -> FuncSignature {
    let fn_name = Token::generic_identifier(format!("create-{}-instance", &class.name.lexeme));
    FuncSignature {
        name: fn_name,
        visibility: Visibility::Public,
        generics: None,
        return_type: Some(Type::Ident(class.name.clone())),
        parameters: vec![],
    }
}

/// Returns the MIR function signature of a class constructor.
fn get_constructor_sig(
    gen: &mut MIRGenerator,
    class: &ASTClass,
    constructor: &Constructor,
    this_arg: &FunctionArg,
    index: usize,
) -> Res<FuncSignature> {
    let name = Token::generic_identifier(format!("{}-constructor-{}", &class.name.lexeme, index));
    let mut parameters = constructor
        .parameters
        .iter()
        .map(|(name, ty)| {
            let type_ = ty
                .clone()
                .or_else(|| get_field_by_name(class, name).map(|(_, ty)| ty).flatten_())
                .or_err(
                    gen,
                    name,
                    "Cannot infer type of field with default value (specify type explicitly.)",
                )?;
            Ok(FunctionArg {
                type_,
                name: name.clone(),
            })
        })
        .collect::<Res<Vec<FunctionArg>>>()?;
    parameters.insert(0, this_arg.clone());
    Ok(FuncSignature {
        name,
        visibility: constructor.visibility,
        generics: None,
        return_type: None,
        parameters,
    })
}

fn get_field_by_name(class: &ASTClass, name: &Token) -> Option<(usize, Option<Type>)> {
    class
        .variables
        .iter()
        .enumerate()
        .find(|(_, mem)| mem.name.lexeme == name.lexeme)
        .map(|(i, mem)| (i, mem.ty.clone()))
}

/// Insert all constructor 'setter' parameters into the entry
/// block of the MIR function.
fn insert_constructor_setters(
    gen: &mut MIRGenerator,
    class: &ASTClass,
    constructor: &Constructor,
    mir_fn_params: &Vec<Rc<Variable>>,
) -> Res<Block> {
    let mut block = Vec::new();
    for (index, (param, _)) in constructor
        .parameters
        .iter()
        .enumerate()
        .filter(|(_, (_, ty))| ty.is_none())
        {
            let (field_index, _) =
                get_field_by_name(class, param).or_err(gen, param, "Unknown class field.")?;
        block.push(Expression::StructSet {
            object: Box::new(Expression::VarGet(Rc::clone(&mir_fn_params[0]))),
            index: field_index as u32,
            value: Box::new(Expression::VarGet(Rc::clone(&mir_fn_params[index + 1]))),
        })
    }
    Ok(block)
}

/// Will add a default constructor with no parameters
/// should the class not contain any other constructors.
fn maybe_add_default_constructor(class: &mut ASTClass) {
    if class.constructors.is_empty() {
        class.constructors.push(Constructor {
            parameters: vec![],
            visibility: Visibility::Public,
            body: ASTExpr::Block(vec![]),
        })
    }
}

/// Modify the AST method signature to fit MIR codegen requirements.
fn modify_method_sig(class_name: &Rc<String>, method: &mut FuncSignature, this_arg: &FunctionArg) -> Rc<String> {
    let old_name = Rc::clone(&method.name.lexeme);
    // Change the method name to $class-$method to prevent name collisions
    method.name.lexeme = Rc::new(format!("{}-{}", class_name, method.name.lexeme));
    // Add implicit this arg
    method.parameters.insert(0, this_arg.clone());
    old_name
}
