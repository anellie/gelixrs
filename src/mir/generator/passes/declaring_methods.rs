/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/20/19 6:38 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::HashSet;
use std::rc::Rc;

use either::Either::Left;

use crate::ast::declaration::{Constructor, FuncSignature, FunctionArg, Visibility};
use crate::ast::Type as ASTType;
use crate::ast::{Class as ASTClass, Expression};
use crate::error::{Error, Res};
use crate::lexer::token::{TType, Token};
use crate::mir::generator::builder::MIRBuilder;
use crate::mir::generator::passes::declaring_globals::create_function;
use crate::mir::generator::passes::{ModulePass, PassType};
use crate::mir::generator::MIRGenerator;
use crate::mir::nodes::{Block, Class, Expr, IFaceMethod, Interface, Type, Variable};
use crate::mir::result::ToMIRResult;
use crate::mir::MutRc;
use crate::option::Flatten;

/// This pass defines all methods on classes and interfaces.
pub struct DeclareMethods();

impl ModulePass for DeclareMethods {
    fn get_type(&self) -> PassType {
        PassType::Type
    }

    fn run_type(&self, gen: &mut MIRGenerator, ty: Type) -> Result<(), Error> {
        match ty {
            Type::Class(cls) => declare_for_class(&mut gen.builder, cls)?,
            Type::Interface(iface) => declare_for_iface(&mut gen.builder, iface)?,
            _ => (),
        }
        Ok(())
    }
}

fn declare_for_class(builder: &mut MIRBuilder, class: MutRc<Class>) -> Res<()> {
    let ast = Rc::clone(&class.borrow().ast);
    let this_arg = FunctionArg::this_arg(&ast.name);

    // Do the instantiator
    let init_fn_sig = get_instantiator_fn_sig(&ast);
    class.borrow_mut().instantiator = create_function(builder, Left(&init_fn_sig), false, None)?;

    // Do all user-defined methods
    for method in ast.methods.iter() {
        let mir_method =
            create_function(builder, Left(&method.sig), false, Some(this_arg.clone()))?;
        class
            .borrow_mut()
            .methods
            .insert(Rc::clone(&method.sig.name.lexeme), mir_method);
    }

    // Do all constructors
    let mut constructor_list = HashSet::with_capacity(ast.constructors.len());

    let default = maybe_default_constructor(&ast);
    let iter = ast.constructors.iter().chain(default.iter()).enumerate();
    for (i, constructor) in iter {
        let sig = get_constructor_sig(builder, &ast, constructor, &this_arg, i)?;
        let mir_var = create_function(builder, Left(&sig), false, None)?;
        let mir_fn = mir_var.type_.as_function();
        let mut mir_fn = mir_fn.borrow_mut();
        let block = insert_constructor_setters(builder, &ast, constructor, &mir_fn.parameters)?;
        mir_fn.blocks.insert(Rc::new("entry".to_string()), block);
        class.borrow_mut().constructors.push(Rc::clone(&mir_var));

        let params = mir_fn
            .parameters
            .iter()
            .skip(1)
            .map(|p| &p.type_)
            .cloned()
            .collect::<Vec<Type>>();
        if !constructor_list.insert(params) {
            return Err(Error::new(
                &ast.name,
                "MIR",
                "Class contains constructors with duplicate signatures.".to_string(),
                &builder.path,
            ));
        }
    }

    Ok(())
}

/// Returns signature of the class instantiator.
fn get_instantiator_fn_sig(class: &ASTClass) -> FuncSignature {
    let fn_name = Token::generic_identifier(format!("create-{}-instance", &class.name.lexeme));
    FuncSignature {
        name: fn_name,
        visibility: Visibility::Public,
        generics: None,
        return_type: Some(ASTType::Ident(class.name.clone())),
        parameters: vec![],
        variadic: false,
    }
}

/// Returns the MIR function signature of a class constructor.
fn get_constructor_sig(
    builder: &mut MIRBuilder,
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
                    &builder.path,
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
        variadic: false,
    })
}

fn get_field_by_name(class: &ASTClass, name: &Token) -> Option<(usize, Option<ASTType>)> {
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
    builder: &mut MIRBuilder,
    class: &ASTClass,
    constructor: &Constructor,
    mir_fn_params: &[Rc<Variable>],
) -> Res<Block> {
    let mut block = Vec::new();
    for (index, (param, _)) in constructor
        .parameters
        .iter()
        .enumerate()
        .filter(|(_, (_, ty))| ty.is_none())
    {
        let (field_index, _) =
            get_field_by_name(class, param).or_err(&builder.path, param, "Unknown class field.")?;
        block.push(Expr::struct_set_index(
            Expr::load(&mir_fn_params[0]),
            field_index,
            Expr::load(&mir_fn_params[index + 1]),
        ))
    }
    Ok(block)
}

/// Will return a default constructor with no parameters
/// should the class not contain a constructor.
fn maybe_default_constructor(class: &ASTClass) -> Option<Constructor> {
    if class.constructors.is_empty() {
        Some(Constructor {
            parameters: vec![],
            visibility: Visibility::Public,
            body: Expression::Block(vec![], Token::generic_token(TType::RightBrace)),
        })
    } else {
        None
    }
}

fn declare_for_iface(builder: &mut MIRBuilder, iface: MutRc<Interface>) -> Res<()> {
    let ast = Rc::clone(&iface.borrow().ast);
    for method in ast.methods.iter() {
        let ret_type = match method.sig.return_type.as_ref() {
            Some(ty) => builder.find_type(ty)?,
            None => Type::None,
        };

        let mut parameters = Vec::with_capacity(method.sig.parameters.len());
        for param in method.sig.parameters.iter() {
            parameters.push(builder.find_type(&param.type_)?);
        }

        let dupe = iface.borrow_mut().methods.insert(
            Rc::clone(&method.sig.name.lexeme),
            IFaceMethod {
                name: Rc::clone(&method.sig.name.lexeme),
                parameters,
                ret_type,
                has_default_impl: method.body.is_some(),
            },
        );

        if dupe.is_some() {
            return Err(Error::new(
                &method.sig.name,
                "MIR",
                format!(
                    "Method with name {} already defined",
                    method.sig.name.lexeme
                ),
                &builder.path,
            ));
        }
    }
    Ok(())
}
