use std::rc::Rc;

use smol_str::SmolStr;

use crate::{
    ast,
    ast::declaration::{FuncSignature, FunctionParam, Visibility},
    gir::{
        generator::GIRGenerator,
        nodes::{
            declaration::{Declaration, Function, ADT},
            expression::Expr,
        },
        MutRc,
    },
    lexer::token::Token,
};

impl GIRGenerator {
    pub fn intrinsic_methods(&mut self, decl: Declaration) {
        if let Declaration::Adt(adt) = decl {
            self.declare_lifecycle_methods(&adt);
        }
    }

    fn declare_lifecycle_methods(&mut self, adt: &MutRc<ADT>) {
        let mut adt = adt.borrow_mut();
        let ast = Rc::clone(&adt.ast);
        let this_param = FunctionParam::this_param_g(&ast.borrow());

        let init_fn = Self::get_instantiator_ast(this_param.clone());
        let init = eat!(
            self,
            self.generate_gir_fn(init_fn, None, Some(&adt.type_parameters))
        );
        self.generate_instantiator(&mut adt, &init);
        adt.methods
            .insert(SmolStr::new_inline("new-instance"), init);

        let wr_destructor_fn = Self::get_destructor_ast(this_param.clone(), false);
        let wr = eat!(
            self,
            self.generate_gir_fn(wr_destructor_fn, None, Some(&adt.type_parameters))
        );
        self.generate_wr_destructor(&mut adt, &wr);
        adt.methods.insert(SmolStr::new_inline("free-wr"), wr);

        let sr_destructor_fn = Self::get_destructor_ast(this_param, true);
        let sr = eat!(
            self,
            self.generate_gir_fn(sr_destructor_fn, None, Some(&adt.type_parameters))
        );
        self.generate_sr_destructor(&mut adt, &sr);
        adt.methods.insert(SmolStr::new_inline("free-sr"), sr);
    }

    fn generate_instantiator(&mut self, adt: &mut ADT, func: &MutRc<Function>) {
        self.set_pointer(func);
        let var = Rc::clone(&func.borrow().parameters[0]);

        for field in adt.fields.values() {
            if let Some(init) = field.initializer.take().take() {
                self.insert_at_ptr(Expr::store(
                    Expr::load(Expr::lvar(&var), field),
                    *init,
                    true,
                ))
            }
        }

        // Insert at the end of the instantiator to prevent
        // an edge case of an empty instantiator, which IR would interpret
        // incorrectly as an external function
        self.insert_at_ptr(Expr::none_const_())
    }

    fn generate_wr_destructor(&mut self, adt: &mut ADT, func: &MutRc<Function>) {
        self.set_pointer(func);
        let var = Rc::clone(&func.borrow().parameters[0]);

        todo!();

        self.insert_at_ptr(Expr::none_const_())
    }

    fn generate_sr_destructor(&mut self, adt: &mut ADT, func: &MutRc<Function>) {
        self.set_pointer(func);
        let var = Rc::clone(&func.borrow().parameters[0]);

        todo!();

        self.insert_at_ptr(Expr::none_const_())
    }

    /// Returns AST of the ADT instantiator.
    fn get_instantiator_ast(mut this_param: FunctionParam) -> ast::Function {
        this_param.type_ = ast::Type::Weak(Box::new(this_param.type_));
        ast::Function {
            sig: FuncSignature {
                name: Token::generic_identifier("new-instance"),
                visibility: Visibility::Public,
                generics: None,
                return_type: None,
                parameters: vec![this_param],
                variadic: false,
                modifiers: vec![],
            },
            body: None,
        }
    }

    /// Returns signature of the ADT destructor.
    fn get_destructor_ast(mut this_param: FunctionParam, strong_ref: bool) -> ast::Function {
        if strong_ref {
            this_param.type_ = ast::Type::Strong(Box::new(this_param.type_));
        } else {
            this_param.type_ = ast::Type::Weak(Box::new(this_param.type_));
        }

        let mut sig = FuncSignature {
            name: Token::generic_identifier(if strong_ref { "free-sr" } else { "free-wr" }),
            visibility: Visibility::Public,
            generics: None,
            return_type: None,
            parameters: vec![
                this_param,
                FunctionParam {
                    type_: ast::Type::Ident(Token::generic_identifier("bool")),
                    name: Token::generic_identifier("refcount_is_0"),
                },
            ],
            variadic: false,
            modifiers: vec![],
        };
        if !strong_ref {
            sig.parameters.pop();
        }
        ast::Function { sig, body: None }
    }
}
