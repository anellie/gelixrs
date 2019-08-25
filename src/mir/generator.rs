/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/24/19 8:18 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::mir::builder::MIRBuilder;
use std::collections::HashMap;
use crate::mir::mir::{MIRVariable, MIRType, MIRStructMem, MIRExpression};
use crate::ast::declaration::{DeclarationList, Class, Function, FuncSignature, FunctionArg};
use crate::ast::expression::{Expression};
use crate::mir::MIR;
use crate::lexer::token::Token;
use std::rc::Rc;

#[macro_use]
mod match_macro {
    /// This macro checks if 2 enum values are of the same variant.
    /// It does not check that their content is the same.
    #[macro_export]
    macro_rules! matches(
        ($e:expr, $p:pat) => (
            match $e {
                $p => true,
                _ => false
            }
        )
    );
}

type Res<T> = Result<T, Error>;

pub struct MIRGenerator {
    builder: MIRBuilder,

    environments: Vec<HashMap<Rc<String>, Rc<MIRVariable>>>,

    is_in_loop: bool,
    current_loop_ret_type: Option<MIRType>,

    none_const: Rc<String>
}

impl MIRGenerator {
    /// Will do all passes after one another.
    pub fn generate(mut self, list: DeclarationList) -> Option<MIR> {
        self.run(list)
            .or_else(|err| {
                eprintln!(
                    "[MIRGenerator] {}:
{} |
{} | {}
{} |",
                    err.message,
                    err.line
                        .map(|l| (l - 1).to_string())
                        .unwrap_or("?".to_string()),
                    err.line.map(|l| l.to_string()).unwrap_or("?".to_string()),
                    err.code,
                    err.line
                        .map(|l| (l + 1).to_string())
                        .unwrap_or("?".to_string()),
                );
                Err(())
            })
            .ok()?;

        Some(self.builder.get_mir())
    }

    fn run(&mut self, mut list: DeclarationList) -> Res<()> {
        self.first_pass(&list)?;
        self.second_pass(&mut list)?;
        self.third_pass(&mut list)?;
        self.fourth_pass(list)
    }

    /// During the first pass, the types map is filled with all types/structs.
    /// These types are empty, and filled later.
    fn first_pass(&mut self, list: &DeclarationList) -> Res<()> {
        for class in &list.classes {
            self.create_class(&class)?;
        }

        Ok(())
    }

    fn create_class(&mut self, class: &Class) -> Res<()> {
        // Create init function
        self.create_function(
            &FuncSignature {
                name: Token::generic_identifier(format!("{}-internal-init", &class.name.lexeme)),
                return_type: None,
                parameters: vec![FunctionArg {
                    name: Token::generic_identifier("this".to_string()),
                    _type: class.name.clone(),
                }],
            }
        )?;

        // Create struct (filled later)
        self.builder
            .create_struct(Rc::clone(&class.name.lexeme))
            .ok_or_else(|| Error::new(
                Some(class.name.line),
                "Class was already defined!",
                format!("class {} {{ ... }}", &class.name.lexeme))
            )?;

        Ok(())
    }

    /// During the second pass, all functions are declared/created.
    fn second_pass(&mut self, list: &mut DeclarationList) -> Res<()> {
        for function in list.ext_functions.iter().chain(list.functions.iter().map(|f| &f.sig)) {
            self.create_function(&function)?;
        }

        for class in list.classes.iter_mut() {
            let name = &class.name.lexeme;
            for method in class.methods.iter_mut() {
                method.sig.name.lexeme = Rc::new(format!("{}-{}", name, method.sig.name.lexeme));
                self.create_function(&method.sig)?;
            }
        }

        Ok(())
    }

    fn create_function(&mut self, func_sig: &FuncSignature) -> Res<()> {
        let ret_type = &self.builder.find_type(
            func_sig.return_type
                .as_ref()
                .map(|t| &t.lexeme)
                .unwrap_or(&self.none_const)
        ).ok_or_else(|| Error::new_fn("Unknown function return type", &func_sig))?;

        let mut parameters = Vec::with_capacity(func_sig.parameters.len());
        for param in func_sig.parameters.iter() {
            parameters.push(Rc::new(MIRVariable {
                mutable: false,
                name: Rc::clone(&param.name.lexeme),
                _type: self.builder
                    .find_type(&param._type.lexeme)
                    .ok_or_else(|| Error::new_fn("Function parameter has unknown type", &func_sig))?
            }))
        }

        let function = self.builder
            .create_function(Rc::clone(&func_sig.name.lexeme), ret_type.clone(), parameters)
            .ok_or_else(|| Error::new_fn("Function was declared twice", &func_sig))?;

        self.environments.first_mut().unwrap().insert(
            Rc::clone(&func_sig.name.lexeme),
            Rc::new(MIRVariable::new(Rc::clone(&func_sig.name.lexeme), MIRType::Function(function), false))
        );

        Ok(())
    }

    fn third_pass(&mut self, list: &mut DeclarationList) -> Res<()> {
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

    fn fill_class_struct(&mut self, class: &mut Class) -> Result<(), Error> {
        let mut fields = HashMap::with_capacity(class.variables.len());

        let mut superclass = None;
        if let Some(super_name) = &class.superclass {
            let super_struct = self.builder
                .find_struct(&super_name.lexeme)
                .ok_or_else(|| Error::new(
                    Some(super_name.line),
                    "Unknown class",
                    format!(
                        "class {} ext {} {{ ... }}",
                        class.name.lexeme, super_name.lexeme
                    )
                ))?;

            for member in super_struct.borrow().members.iter() {
                fields.insert(Rc::clone(member.0), Rc::clone(member.1));
            }

            superclass = Some(super_struct);
        }

        self.build_class_init(class, &mut fields)?;

        let class_rc = self.builder.find_struct(&class.name.lexeme).unwrap();
        let mut class_def = class_rc.borrow_mut();
        class_def.members = fields;
        class_def.super_struct = superclass;

        Ok(())
    }

    fn build_class_init(
        &mut self,
        class: &mut Class,
        fields: &mut HashMap<Rc<String>, Rc<MIRStructMem>>
    ) -> Res<()> {
        let function_rc = self.builder.find_function(&format!("{}-internal-init", &class.name.lexeme)).unwrap();
        let mut function = function_rc.borrow_mut();
        function.append_block("entry".to_string());
        drop(function);
        self.builder.set_pointer(Rc::clone(&function_rc), Rc::new("entry".to_string()));

        for (i, field) in class.variables.drain(..).enumerate() {
            // TODO: Properly build StructSet instead of just uselessly evaluating the expression...
            fields.insert(
                Rc::clone(&field.name.lexeme),
                Rc::new(MIRStructMem {
                    mutable: !field.is_val,
                    _type: self.generate_expression(field.initializer)?.get_type(),
                    index: i as u32
                })
            );
        }

        Ok(())
    }

    /// During the fourth pass, all function bodies are created.
    /// Type-checking happens at the same time.
    fn fourth_pass(&mut self, list: DeclarationList) -> Result<(), Error> {
        for func in list.functions.into_iter().chain(
            list.classes
                .into_iter()
                .map(|class| class.methods)
                .flatten()
        ) {
            self.generate_function(func)?;
        }

        Ok(())
    }

    fn generate_function(&mut self, func: Function) -> Res<()> {
        let function_rc = self.builder.find_function(&func.sig.name.lexeme).unwrap();
        let mut function = function_rc.borrow_mut();
        function.append_block("entry".to_string());
        drop(function);
        self.builder.set_pointer(Rc::clone(&function_rc), Rc::new("entry".to_string()));

        self.begin_scope();
        for param in function_rc.borrow().parameters.iter() {
            self.insert_variable(Rc::clone(param), false, func.sig.name.line)?;
        }

        let body_type = self.generate_expression(func.body)?.get_type();
        if body_type != function_rc.borrow().ret_type {
            Err(Error::new_fn(
                "Function return type does not match body type",
                &func.sig,
            ))?;
        }

        self.end_scope();
        Ok(())
    }

    fn generate_expression(&mut self, expression: Expression) -> Res<MIRExpression> {
        match expression {
            Expression::Assignment { name, value } => {
                let var = self.find_var(&name)?;
                if var.mutable {
                    let value = self.generate_expression(*value)?;
                    if value.get_type() == var._type {
                        Ok(self.builder.build_store(var, value))
                    } else {
                        Err(Error::new(
                            Some(name.line),
                            &format!("Variable {} is a different type", name.lexeme),
                            name.lexeme.to_string(),
                        ))?
                    }
                } else {
                    Err(Error::new(
                        Some(name.line),
                        &format!("Variable {} is not assignable (val)", name.lexeme),
                        name.lexeme.to_string(),
                    ))?
                }
            }

            _ => Err(Error::new(None, "", "".to_string()))
        }
    }

    fn define_variable(
        &mut self,
        token: &mut Token,
        mutable: bool,
        _type: MIRType,
    ) -> Res<()> {
        let def = Rc::new(MIRVariable::new(Rc::clone(&token.lexeme), _type, mutable));
        self.builder.add_function_variable(Rc::clone(&def));
        self.insert_variable(def, true, token.line)?;
        Ok(())
    }

    fn insert_variable(&mut self, var: Rc<MIRVariable>, allow_redefine: bool, line: usize) -> Res<()> {
        let cur_env = self.environments.last_mut().unwrap();
        let was_defined = cur_env.insert(Rc::clone(&var.name), Rc::clone(&var)).is_some();
        if was_defined && !allow_redefine {
            Err(Error {
                line: Some(line),
                message: format!("Cannot redefine variable '{}' in the same scope.", &var.name),
                code: (*var.name).clone(),
            })?;
        }

        Ok(())
    }

    fn find_var(&mut self, token: &Token) -> Res<Rc<MIRVariable>> {
        for env in self.environments.iter().rev() {
            if let Some(var) = env.get(&token.lexeme) {
                return Ok(Rc::clone(var))
            }
        }

        Err(Error {
            line: Some(token.line),
            message: format!("Variable '{}' is not defined", token.lexeme),
            code: (*token.lexeme).clone(),
        })
    }

    fn begin_scope(&mut self) {
        self.environments.push(HashMap::new());
    }

    fn end_scope(&mut self) {
        self.environments.pop();
    }

    pub fn new() -> MIRGenerator {
        MIRGenerator {
            builder: MIRBuilder::new(),
            environments: Vec::with_capacity(5),

            is_in_loop: false,
            current_loop_ret_type: None,

            none_const: Rc::new("None".to_string())
        }
    }
}


pub struct Error {
    pub line: Option<usize>,
    pub message: String,
    pub code: String,
}

impl Error {
    pub fn new_expr(message: String, expr: &Expression) -> Error {
        Error {
            line: expr.get_line(),
            message,
            code: expr.to_string(),
        }
    }

    pub fn new_fn(message: &str, func_sig: &FuncSignature) -> Error {
        Error {
            line: Some(func_sig.name.line),
            message: message.to_string(),
            code: format!("func {}(...)", func_sig.name.lexeme),
        }
    }

    pub fn new(line: Option<usize>, message: &str, code: String) -> Error {
        Error { line, message: message.to_owned(), code }
    }
}
