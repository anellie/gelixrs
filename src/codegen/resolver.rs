use super::super::{
    ast::{
        declaration::{DeclarationList, Function, FuncSignature, FunctionArg},
        expression::{Expression, LOGICAL_BINARY},
        literal::Literal,
    },
    lexer::token::Token,
};
use super::{ClassDef, ClassField, IRGenerator};
use inkwell::{
    builder::Builder,
    context::Context,
    module::Module,
    passes::PassManager,
    types::{AnyTypeEnum, BasicType, BasicTypeEnum, FunctionType, StructType},
    values::BasicValueEnum,
    AddressSpace,
};
use std::{collections::HashMap, mem};
use crate::lexer::token::Type;

/// A resolver. Resolves all variables and types.
pub struct Resolver {
    context: Context,
    builder: Builder,
    module: Module,

    /// All classes.
    types: HashMap<String, ClassDef>,
    // A constant that is used for expressions that don't produce a value but are required to.
    none_const: BasicTypeEnum,
    // All environments/scopes at the moment. Used like a stack.
    environments: Vec<Environment>,

    /// Used to make unique variable names when a name collision occurs.
    /// Increments every time a new scope is created.
    environment_counter: usize,

    /// The current function. Mainly used to resolve return expressions during the 4th pass.
    current_func: Option<FunctionType>,
    /// The name of the current function. Mainly used for error reporting.
    current_func_name: String,

    /// If the resolver is currently resolving a loop.
    is_in_loop: bool,
    /// The return type of the current loop.
    current_loop_type: Option<BasicTypeEnum>
}

impl Resolver {
    /// Will do all passes after one another.
    pub fn resolve(&mut self, list: &mut DeclarationList) -> Option<()> {
        self.run(list)
            .or_else(|err| {
                eprintln!(
                    "[Resolver] {} (occurred in function: {})",
                    err, &self.current_func_name
                );
                Err(())
            })
            .ok()
    }

    fn run(&mut self, list: &mut DeclarationList) -> Result<(), String> {
        self.first_pass(list)?;
        self.second_pass(list)?;
        self.third_pass(list)?;
        self.fourth_pass(list)
    }

    /// During the first pass, the types map is filled with all types.
    /// These types are opaque, and filled later.
    fn first_pass(&mut self, list: &DeclarationList) -> Result<(), String> {
        for class in &list.classes {
            self.declare_type(&class.name)?;
        }

        Ok(())
    }

    fn declare_type(&mut self, name: &Token) -> Result<(), String> {
        // Create an opaque struct & class definition
        let struc = self.context.opaque_struct_type(&name.lexeme);
        let class_def = ClassDef::new(struc);

        // Insert into type map for initializer to be able to resolve it
        let exists = self
            .types
            .insert(name.lexeme.to_string(), class_def)
            .is_some();

        // Create an initializer function
        let init_fn = FuncSignature {
            name: Token::generic_identifier(format!("init-{}", &name.lexeme)),
            return_type: None,
            parameters: vec![FunctionArg {
                name: Token::generic_identifier("this".to_string()),
                _type: name.clone(),
            }],
        };
        self.create_function(&init_fn, true)?;
        self.types.get_mut(&name.lexeme).unwrap().initializer =
            self.module.get_function(&init_fn.name.lexeme);

        // Ensure it doesn't exist yet
        if !exists {
            Ok(())
        } else {
            Err(format!(
                "The type/class/enum {} was declared more than once!",
                name.lexeme
            ))
        }
    }

    /// During the second pass, all functions are declared.
    fn second_pass(&mut self, list: &mut DeclarationList) -> Result<(), String> {
        for ext_fn in &list.ext_functions {
            self.create_function(&ext_fn, false)?;
        }

        for func in &list.functions {
            self.create_function(&func.sig, false)?;
        }

        for class in list.classes.iter_mut() {
            // This argument is put as the first on all class methods.
            // This allows simply passing in the struct to a regular function
            // to emulate class methods.
            let this_arg = FunctionArg {
                _type: class.name.clone(),
                name: Token::generic_identifier("this".to_string()),
            };

            for func in class.methods.iter_mut() {
                func.sig.parameters.insert(0, this_arg.clone());

                // Rename to prevent naming collisions
                let old_name = func.sig.name.lexeme.clone();
                func.sig.name.lexeme = format!("{}-{}", class.name.lexeme, func.sig.name.lexeme);

                self.create_function(&func.sig, false)?;
                let class_def = self.types.get_mut(&class.name.lexeme).unwrap();
                class_def.methods.insert(
                    old_name,
                    self.module.get_function(&func.sig.name.lexeme).unwrap(),
                );
            }
        }

        Ok(())
    }

    fn create_function(
        &mut self,
        function: &FuncSignature,
        first_is_ptr: bool,
    ) -> Result<(), String> {
        if self
            .module
            .get_function(&function.name.lexeme.to_string())
            .is_some()
        {
            return Err(format!(
                "The function {} was declared more than once!",
                function.name.lexeme
            ));
        }

        let mut parameters = Vec::new();
        for param in &function.parameters {
            parameters.push(self.resolve_type(&param._type)?);
        }
        // TODO: This is rather hacky/ugly
        if first_is_ptr {
            let first = parameters.remove(0);
            if let BasicTypeEnum::StructType(struc) = first {
                parameters.push(struc.ptr_type(AddressSpace::Generic).into());
            } else {
                panic!("create_function: first_is_ptr");
            }
        }
        let parameters = parameters.as_slice();

        let fn_type = if let Some(ret_type) = &function.return_type {
            let call_type = self.resolve_type(&ret_type)?;
            call_type.fn_type(&parameters, false)
        } else {
            self.context.void_type().fn_type(&parameters, false)
        };

        self.module
            .add_function(&function.name.lexeme, fn_type, None);
        self.environments.first_mut().unwrap().variables.insert(
            function.name.lexeme.to_string(),
            VarDef::new(false, fn_type.ptr_type(AddressSpace::Const).into()),
        );
        Ok(())
    }

    /// During the third pass, all class structs are filled.
    fn third_pass(&mut self, list: &mut DeclarationList) -> Result<(), String> {
        for class in list.classes.iter_mut() {
            let mut fields = Vec::with_capacity(class.variables.len());
            let mut fields_map = HashMap::with_capacity(class.variables.len());

            for (i, field) in class.variables.iter_mut().enumerate() {
                fields.push(self.resolve_expression(&mut field.initializer)?);
                fields_map.insert(field.name.lexeme.to_string(), ClassField::new(i as u32, field.is_val));
            }

            let mut superclass = None;
            if let Some(sclass) = &class.superclass {
                let sclass_type = self.get_type(&sclass.lexeme)?;
                fields.push(sclass_type);
                if let BasicTypeEnum::StructType(struc) = sclass_type {
                    superclass = Some(struc);
                } else {
                    Err("Only classes can be inherited from.")?
                }
            }

            let mut class_def = self.types.get_mut(&class.name.lexeme).unwrap();
            class_def._type.set_body(fields.as_slice(), false);
            class_def.var_map = fields_map;
            class_def.superclass = superclass;
        }

        Ok(())
    }

    /// During the fourth pass, all variables inside functions are checked.
    /// This is to ensure the variable is defined and allowed in the current scope.
    fn fourth_pass(&mut self, list: &mut DeclarationList) -> Result<(), String> {
        for func in list.functions.iter_mut() {
            self.resolve_function(func)?;
        }

        for class in list.classes.iter_mut() {
            for method in class.methods.iter_mut() {
                self.resolve_function(method)?;
            }
        }

        Ok(())
    }

    fn resolve_function(&mut self, func: &mut Function) -> Result<(), String> {
        self.current_func_name = func.sig.name.lexeme.to_string();
        self.current_func = Some(
            self.module
                .get_function(&self.current_func_name)
                .unwrap()
                .get_type(),
        );

        self.begin_scope();
        for param in func.sig.parameters.iter_mut() {
            let param_type = self.resolve_type(&param._type)?;
            self.define_variable(&mut param.name, false, false, param_type)?;
        }

        let body_type = self.resolve_expression(&mut func.body)?;
        let ret_type = if let Some(type_tok) = &func.sig.return_type {
            self.resolve_type(type_tok)?
        } else {
            body_type // Body type does not matter when ret type is None
        };
        if body_type != ret_type {
            Err("Function return type does not match body type.".to_string())?;
        }

        self.end_scope();
        Ok(())
    }

    fn resolve_expression(&mut self, expression: &mut Expression) -> Result<BasicTypeEnum, String> {
        match expression {
            Expression::Assignment { name, value } => {
                let var = self.find_var(name)?;
                if var.mutable {
                    let expr_type = self.resolve_expression(value)?;
                    if expr_type == var._type {
                        Ok(expr_type)
                    } else {
                        Err(format!("Variable {} is a different type.", name.lexeme))
                    }
                } else {
                    Err(format!("Variable {} is not assignable (val)", name.lexeme))
                }
            }

            Expression::Binary { left, operator, right } => {
                let left = self.resolve_expression(left)?;
                let right = self.resolve_expression(right)?;

                if left == right {
                    if LOGICAL_BINARY.contains(&operator.t_type) {
                        Ok(self.get_type("bool")?)
                    } else {
                        Ok(left)
                    }
                } else {
                    Err(format!(
                        "Binary operands have incompatible types! {:?} and {:?}",
                        left, right
                    ))
                }
            }

            Expression::Block(expressions) => {
                self.begin_scope();
                let ret_type = expressions.iter_mut().try_fold(self.none_const, |_, expr| {
                    self.resolve_expression(expr)
                })?;
                self.end_scope();
                Ok(ret_type)
            }

            Expression::Break(expr) => {
                if !self.is_in_loop {
                    Err("'break' is only allowed in loops.")?;
                }

                let break_type = if let Some(expr) = expr {
                    self.resolve_expression(expr)?
                } else {
                    self.none_const
                };

                if let Some(loop_type) = self.current_loop_type {
                    if break_type != loop_type {
                        Err("All 'break' inside of a loop must have the same type.")?;
                    }
                } else {
                    self.current_loop_type = Some(break_type)
                }

                self.get_type("None")
            }

            Expression::Call { callee, arguments } => {
                let callee = &mut **callee; // Don't you love boxes?
                let mut is_method = false;
                match callee {
                    Expression::Get { object: _, name: _ } => {
                        is_method = true;
                    },

                    Expression::Variable(name) => {
                        if let Some(class_def) = self.types.get(&name.lexeme) {
                            return Ok(class_def._type.into())
                        }
                    }

                    _ => (),
                }

                let callee = self.resolve_expression(callee)?;
                if let BasicTypeEnum::PointerType(ptr) = callee {
                    let func = ptr.get_element_type();
                    if let AnyTypeEnum::FunctionType(func) = func {
                        self.check_func_args(func, arguments, is_method)?;
                        return Ok(func
                            .get_return_type()
                            .get_or_insert(self.none_const)
                            .clone())
                    }
                }

                Err("Only functions or classes are allowed to be called.".to_string())
            }

            Expression::For { condition, body } => {
                // Save previous state in case of nested loops
                let was_in_loop = std::mem::replace(&mut self.is_in_loop, true);
                let prev_loop_type = std::mem::replace(&mut self.current_loop_type, None);

                let condition_type = self.resolve_expression(condition)?;
                if condition_type != self.get_type("bool")? {
                    Err("For condition must be a boolean.")?
                }

                let body_type = self.resolve_expression(body)?;

                // Restore previous state
                self.is_in_loop = was_in_loop;
                let loop_type = std::mem::replace(&mut self.current_loop_type, prev_loop_type);

                if let Some(loop_type) = loop_type {
                    if body_type == loop_type {
                        Ok(body_type)
                    } else {
                        Ok(self.none_const)
                    }
                } else {
                    Ok(body_type)
                }
            }

            Expression::Get { object, name } => {
                let object = self.resolve_expression(object)?;

                if let BasicTypeEnum::StructType(struc) = object {
                    self.resolve_class_get(struc, name)
                } else {
                    Err("Get syntax (x.y) is only supported on class instances.".to_string())
                }
            }

            Expression::Grouping(expr) => self.resolve_expression(expr),

            Expression::If { condition, then_branch, else_branch } => {
                let condition_type = self.resolve_expression(condition)?;
                if condition_type != self.get_type("bool")? {
                    Err("If condition must be a boolean.")?
                }

                let then_type = self.resolve_expression(then_branch)?;
                if let Some(else_branch) = else_branch {
                    let else_type = self.resolve_expression(else_branch)?;
                    if then_type == else_type {
                        return Ok(then_type);
                    }
                }

                self.get_type("None")
            }

            Expression::Return(expr) => {
                if let Some(expr) = expr {
                    let expr_type = self.resolve_expression(expr)?;
                    let func_type = self.cur_fn().get_return_type();
                    if let Some(func_type) = func_type {
                        if expr_type != func_type {
                            Err("Return expression is not the functions return type.")?
                        }
                    } else {
                        Err("Cannot return a value from a function that returns None.")?
                    }
                }
                self.get_type("None")
            }

            Expression::Set { object, name, value } => {
                let object = self.resolve_expression(object)?;

                if let BasicTypeEnum::StructType(struc) = object {
                    let class_def = self.find_class(struc);
                    let field_type = self.get_class_field_type(class_def, name)?;

                    let expr_type = self.resolve_expression(value)?;
                    if expr_type == field_type {
                        Ok(expr_type)
                    } else {
                        Err(format!("Field {} is a different type.", name.lexeme))
                    }
                } else {
                    Err("Set syntax (x.y = z) is only supported on class instances.".to_string())
                }
            }

            Expression::Unary { operator, right } => {
                let right = self.resolve_expression(right)?;

                match operator.t_type {
                    Type::Minus => Ok(right),
                    Type::Bang => {
                        if right == self.get_type("bool")? {
                            Ok(right)
                        } else {
                            Err("'!' can only be used on boolean values.".to_string())
                        }
                    }
                    _ => panic!("Invalid unary expression."),
                }
            }

            Expression::Literal(literal) => Ok(self.type_from_literal(literal)),

            Expression::Variable(name) => Ok(self.find_var(name)?._type),

            Expression::VarDef(var) => {
                let _type = self.resolve_expression(&mut var.initializer)?;
                self.define_variable(&mut var.name, !var.is_val, true, _type)?;
                Ok(self.none_const)
            }

            Expression::When { value, branches, else_branch } => {
                let value = self.resolve_expression(value)?;
                let result_type = self.resolve_expression(else_branch)?;

                for branch in branches.iter_mut() {
                    let condition = self.resolve_expression(&mut branch.0)?;
                    let result = self.resolve_expression(&mut branch.1)?;

                    if condition != value {
                        Err("Can only compare values of same type in 'when'.".to_string())?;
                    }
                    if result != result_type {
                        Err("Resulting values of 'when' must be same type.".to_string())?;
                    }
                }

                Ok(result_type)
            }
        }
    }

    fn resolve_class_get(
        &self,
        struc: StructType,
        name: &Token,
    ) -> Result<BasicTypeEnum, String> {
        let class_def = self.find_class(struc);

        let var_index = class_def.var_map.get(&name.lexeme);
        if let Some(field) = var_index {
            return class_def
                ._type
                .get_field_type_at_index(field.index)
                .ok_or("Internal error trying to get class field.".to_string());
        }

        let method = class_def.methods.get(&name.lexeme);
        if let Some(method) = method {
            return Ok(method
                .as_global_value()
                .as_pointer_value()
                .get_type()
                .into());
        }

        if let Some(sclass) = class_def.superclass {
            return self.resolve_class_get(sclass.clone(), name);
        }

        Err(format!("Unknown class field '{}'.", name.lexeme))
    }

    fn get_class_field_type(&self, class_def: &ClassDef, name: &Token) -> Result<BasicTypeEnum, String> {
        let class_field = class_def
            .var_map
            .get(&name.lexeme);

        if let Some(field) = class_field {
            let field_type = class_def
                ._type
                .get_field_type_at_index(field.index)
                .expect("Internal error trying to get class field.");

            if field.is_val {
                Err(format!("Class field '{}' cannot be set. (val)", name.lexeme))?
            } else {
                return Ok(field_type)
            }
        } else {
            if let Some(superclass) = class_def.superclass {
                let super_def = self.find_class_def(&superclass);
                if let Some(super_def) = super_def {
                    return self.get_class_field_type(super_def, name);
                }
            }
        }

        Err(format!("Unknown class field '{}'.", name.lexeme))
    }

    fn find_class_def(&self, struc: &StructType) -> Option<&ClassDef> {
        Some(self.types
            .iter()
            .find(|&_type| _type.1._type == *struc)?
            .1)
    }

    fn resolve_type(&mut self, token: &Token) -> Result<BasicTypeEnum, String> {
        self.get_type(&token.lexeme)
    }

    fn get_type(&mut self, name: &str) -> Result<BasicTypeEnum, String> {
        Ok(match name {
            "bool" => self.context.bool_type().as_basic_type_enum(),
            "f32" => self.context.f32_type().as_basic_type_enum(),
            "f64" => self.context.f64_type().as_basic_type_enum(),
            "i32" => self.context.i32_type().as_basic_type_enum(),
            "i64" => self.context.i64_type().as_basic_type_enum(),
            "String" => self
                .context
                .i8_type()
                .ptr_type(AddressSpace::Generic)
                .as_basic_type_enum(),
            "None" => self.none_const,
            _ => self
                .types
                .get(name)
                .ok_or(format!("Unknown type {}", name))?
                ._type
                .as_basic_type_enum(),
        })
    }

    fn type_from_literal(&mut self, literal: &Literal) -> BasicTypeEnum {
        match literal {
            Literal::None => self.none_const,
            Literal::Bool(_) => self.context.bool_type().as_basic_type_enum(),
            Literal::Float(_) => self.context.f32_type().as_basic_type_enum(),
            Literal::Double(_) => self.context.f64_type().as_basic_type_enum(),
            Literal::Int(_) => self.context.i64_type().as_basic_type_enum(),
            Literal::String(_) => self
                .context
                .i8_type()
                .ptr_type(AddressSpace::Generic)
                .as_basic_type_enum(),
            _ => panic!("Unimplemented literal!"),
        }
    }

    fn define_variable(
        &mut self,
        token: &mut Token,
        mutable: bool,
        allow_redefine: bool,
        _type: BasicTypeEnum,
    ) -> Result<(), String> {
        let def = VarDef::new(mutable, _type);

        if self.var_exists(token) {
            let cur_env = self.environments.last_mut().unwrap();
            let new_name = format!("{}-{}", token.lexeme, self.environment_counter);
            let old_name = mem::replace(&mut token.lexeme, new_name);
            cur_env
                .moved_vars
                .insert(old_name.clone(), token.lexeme.clone());

            let was_defined_in_cur_env = cur_env.variables.insert(old_name, def.clone()).is_some();

            if was_defined_in_cur_env && !allow_redefine {
                Err(format!("Cannot redefine variable {}.", token.lexeme))?
            }
        }

        let cur_env = self.environments.last_mut().unwrap();
        cur_env.variables.insert(token.lexeme.to_string(), def);

        Ok(())
    }

    fn find_var(&mut self, token: &mut Token) -> Result<VarDef, String> {
        for env in self.environments.iter().rev() {
            if let Some(var) = env.variables.get(&token.lexeme) {
                if env.moved_vars.contains_key(&token.lexeme) {
                    token.lexeme = env.moved_vars[&token.lexeme].to_string();
                }
                return Ok(var.clone());
            }
        }

        Err(format!("Variable {} is not defined.", token.lexeme))
    }

    fn var_exists(&mut self, token: &Token) -> bool {
        self.environments.iter().any(|env| env.variables.contains_key(&token.lexeme))
    }

    fn find_class(&self, struc: StructType) -> &ClassDef {
        self.types
            .iter()
            .find(|&_type| _type.1._type == struc)
            .unwrap()
            .1
    }

    fn check_func_args(
        &mut self,
        func: FunctionType,
        arguments: &mut Vec<Expression>,
        is_method: bool,
    ) -> Result<(), String> {
        let expected = func.get_param_types();
        // Casting bool to int: 1 = true; 0 = false
        let expected_len = expected.len() - (is_method as usize);

        if expected_len != arguments.len() {
            Err(format!(
                "Incorrect amount of function arguments. (Expected {}; got {})",
                expected_len,
                arguments.len()
            ))?;
        }

        for (i, arg) in arguments.iter_mut().enumerate() {
            let arg_type = self.resolve_expression(arg)?;
            if arg_type != expected[i] {
                Err("Call argument is the wrong type.")?;
            }
        }

        Ok(())
    }

    fn begin_scope(&mut self) {
        self.environments.push(Environment::new());
        self.environment_counter += 1;
    }

    fn end_scope(&mut self) {
        self.environments.pop();
    }

    fn cur_fn(&self) -> FunctionType {
        self.current_func.unwrap()
    }

    pub fn new() -> Resolver {
        let context = Context::create();
        let module = context.create_module("main");
        let builder = context.create_builder();

        let mut environments = Vec::with_capacity(10);
        environments.push(Environment::new()); // global environment

        let none_const = context
            .struct_type(&[BasicTypeEnum::IntType(context.bool_type())], true)
            .into();

        Resolver {
            context,
            module,
            builder,
            types: HashMap::with_capacity(10),
            none_const,
            environments,
            environment_counter: 0,
            current_func: None,
            current_func_name: "".to_string(),
            is_in_loop: false,
            current_loop_type: None
        }
    }

    /// Turns the resolver into a generator for IR. Call resolve() first.
    pub fn into_generator(self, decl_list: DeclarationList) -> IRGenerator {
        let mpm = PassManager::create(());
        mpm.add_instruction_combining_pass();
        mpm.add_reassociate_pass();
        mpm.add_cfg_simplification_pass();
        mpm.add_basic_alias_analysis_pass();

        // Break tests
        // mpm.add_dead_arg_elimination_pass();
        // mpm.add_dead_store_elimination_pass();
        // mpm.add_global_dce_pass();
        // mpm.add_tail_call_elimination_pass();

        // Cause segfaults
        // mpm.add_gvn_pass();
        // mpm.add_loop_deletion_pass();
        // mpm.add_loop_unswitch_pass();
        // mpm.add_promote_memory_to_register_pass();

        mpm.add_instruction_combining_pass();
        mpm.add_reassociate_pass();

        let none_const = self
            .context
            .struct_type(&[BasicTypeEnum::IntType(self.context.bool_type())], true)
            .const_named_struct(&[BasicValueEnum::IntValue(
                self.context.bool_type().const_int(0, false),
            )]);

        IRGenerator {
            context: self.context,
            module: self.module,
            builder: self.builder,
            mpm,

            variables: HashMap::with_capacity(10),
            current_fn: None,
            loop_cont_block: None,

            decl_list,

            types: self.types,
            none_const: none_const.into(),
        }
    }
}

/// An environment holds all variables in the current scope.
/// To keep track of all variables, a stack of them is used.
#[derive(Debug)]
struct Environment {
    /// Key = name; all variables in this environment/scope
    variables: HashMap<String, VarDef>,

    /// Variables that were moved due to a naming collision
    moved_vars: HashMap<String, String>,
}

impl Environment {
    pub fn new() -> Environment {
        Environment {
            variables: HashMap::with_capacity(5),
            moved_vars: HashMap::with_capacity(5),
        }
    }
}

/// A variable definition inside of a scope.
#[derive(Clone, Debug)]
struct VarDef {
    /// If the variable can be reassigned.
    pub mutable: bool,
    /// The underlying LLVM type.
    pub _type: BasicTypeEnum,
}

impl VarDef {
    pub fn new(mutable: bool, _type: BasicTypeEnum) -> VarDef {
        VarDef { mutable, _type }
    }
}
