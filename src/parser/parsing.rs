/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 11/4/19 8:30 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

//! This module contains all functions directly responsible for parsing the tokens
//! and creating the AST from them.

use std::rc::Rc;

use either::Either;

use crate::ast::declaration::{IFaceImpl, Interface, InterfaceFunc, Type};
use crate::ast::module::Import;
use crate::Error;

use super::Parser;
use super::super::{
    ast::{
        declaration::{Class, Enum, FuncSignature, Function, FunctionArg, Variable},
        expression::Expression,
        literal::Literal,
        module::Module,
    },
    lexer::token::{Token, TType},
};

// All expressions that require no semicolon when used as a higher expression.
static NO_SEMICOLON: [TType; 3] = [TType::If, TType::LeftBrace, TType::When];

// All tokens that indicate that an interface function does not have a body (bodies are optional).
static IFACE_END_OF_FUNCTION: [TType; 2] = [TType::Func, TType::RightBrace];

#[macro_use]
mod bin_macro {
    /// This macro is used to generate binary operator parsing functions.
    /// The parser is a recursive descent parser.
    /// name is the name of the binary operation, next is the descending function name.
    /// matching is an array literal of the tokens that should match.
    #[macro_export]
    macro_rules! binary_op {
        ($name:ident, $next:ident, $matching:expr) => {
            fn $name(&mut self) -> Option<Expression> {
                let mut left = self.$next()?;
                while let Some(operator) = self.match_tokens(&$matching) {
                    let right = self.$next()?;
                    left = Expression::Binary {
                        left: Box::new(left), operator, right: Box::new(right)
                    }
                }
                Some(left)
            }
        };
    }
}

impl Parser {
    /// Parses the tokens and returns a full module.
    /// Returns a list of errors on failure.
    pub fn parse(mut self, module: &mut Module) -> Result<(), Vec<Error>> {
        while !self.is_at_end() {
            // Only true on error
            if self.declaration(module).is_none() {
                self.synchronize();
            }
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors)
        }
    }

    /// The entry point for generating a declaration.
    /// The reason for returning Option is that the parser will error out and abort the current
    /// declaration when illegal syntax is encountered.
    /// Note that synchronization is not done on error, and is done by the caller.
    pub fn declaration(&mut self, module: &mut Module) -> Option<()> {
        match self.advance().t_type {
            TType::Class => module.classes.push(self.class_declaration()?),
            TType::Enum => module.enums.push(self.enum_declaration()?),
            TType::ExFn => module.ext_functions.push(self.ex_func_declaration()?),
            TType::Func => module.functions.push(self.function()?),
            TType::Import => module.imports.push(self.import_declaration()?),
            TType::Interface => module.interfaces.push(self.iface_declaration()?),
            TType::Impl => module.iface_impls.push(self.iface_impl()?),
            _ => self.error_at_current("Encountered invalid top-level declaration.")?,
        }

        Some(())
    }

    fn ex_func_declaration(&mut self) -> Option<FuncSignature> {
        let (name, generics) = self.generic_ident()?;
        self.consume(TType::LeftParen, "Expected '(' after function name.");

        let parameters = self.func_parameters()?;
        let return_type = if self.match_token(TType::Arrow) {
            Some(self.type_("Expected return type after '->'.")?)
        } else {
            None
        };

        Some(FuncSignature {
            name,
            return_type,
            parameters,
            generics
        })
    }

    fn func_parameters(&mut self) -> Option<Vec<FunctionArg>> {
        let mut parameters: Vec<FunctionArg> = Vec::new();
        if !self.check(TType::RightParen) {
            loop {
                let name = self.consume(TType::Identifier, "Expected parameter name.")?;
                self.consume(TType::Colon, "Expected ':' after parameter name.");
                let type_ = self.type_("Expected parameter type.")?;
                parameters.push(FunctionArg { type_, name });
                if !self.match_token(TType::Comma) {
                    break;
                }
            }
        }
        self.consume(TType::RightParen, "Expected ')' after parameters.");
        Some(parameters)
    }

    fn class_declaration(&mut self) -> Option<Class> {
        let (name, generics) = self.generic_ident()?;

        self.consume(TType::LeftBrace, "Expected '{' before class body.");

        let mut methods: Vec<Function> = Vec::new();
        let mut variables: Vec<Variable> = Vec::new();

        while !self.check(TType::RightBrace) && !self.is_at_end() {
            match self.advance().t_type {
                TType::Func => methods.push(self.function()?),
                TType::Var => variables.push(self.variable(true)?),
                TType::Val => variables.push(self.variable(false)?),
                _ => self.error_at_current("Encountered invalid declaration inside class.")?,
            }
        }

        self.consume(TType::RightBrace, "Expected '}' after class body.");
        Some(Class {
            name,
            generics,
            methods,
            variables,
        })
    }

    fn enum_declaration(&mut self) -> Option<Enum> {
        let name = self.consume(TType::Identifier, "Expected an enum name.")?;
        self.consume(TType::LeftBrace, "Expected '{' before enum body.");

        let mut variants: Vec<Token> = Vec::new();
        while !self.check(TType::RightBrace) {
            variants.push(self.consume(TType::Identifier, "Expected enum variant.")?);
            if !self.match_token(TType::Comma) {
                break;
            }
        }
        self.consume(TType::RightBrace, "Expected '}' after enum body.");

        Some(Enum { name, variants })
    }

    fn import_declaration(&mut self) -> Option<Import> {
        let import_token = self.current.clone();
        let mut path = Vec::new();
        if !self.check(TType::Identifier) {
            self.error_at_current("Expected path after 'import'.")?
        }

        let mut consumed_slash = false;
        let mut symbol = self.advance();
        while self.check(TType::Identifier) || self.check(TType::Plus) {
            path.push(std::mem::replace(&mut symbol, self.advance()).lexeme);
            consumed_slash = self.match_token(TType::Slash);
        }
        if consumed_slash {
            self.error_at_current("Trailing '/' in import.")?
        }

        Some(Import { path, symbol })
    }

    fn iface_declaration(&mut self) -> Option<Interface> {
        let (name, generics) = self.generic_ident()?;

        self.consume(TType::LeftBrace, "Expected '{' before interface body.");

        let mut methods = Vec::new();
        while !self.check(TType::RightBrace) && !self.is_at_end() {
            match self.advance().t_type {
                TType::Func => {
                    let sig = self.ex_func_declaration()?;
                    let body = if !IFACE_END_OF_FUNCTION.contains(&self.current.t_type) {
                        Some(self.expression()?)
                    } else {
                        None
                    };
                    methods.push(InterfaceFunc { sig, body })
                }
                _ => self.error_at_current("Encountered invalid declaration inside interface.")?,
            }
        }

        self.consume(TType::RightBrace, "Expected '}' after interface body.");
        Some(Interface {
            name,
            generics,
            methods,
        })
    }

    fn iface_impl(&mut self) -> Option<IFaceImpl> {
        let (iface, iface_generics) = self.generic_ident()?;
        self.consume(TType::For, "Expected 'for' after interface name.");
        let (class, class_generics) = self.generic_ident()?;
        self.consume(TType::LeftBrace, "Expected '{' before impl body.");

        let mut methods: Vec<Function> = Vec::new();
        while !self.check(TType::RightBrace) && !self.is_at_end() {
            match self.advance().t_type {
                TType::Func => methods.push(self.function()?),
                _ => self.error_at_current("Encountered invalid declaration inside impl.")?,
            }
        }
        self.consume(TType::RightBrace, "Expected '}' after impl body.");

        Some(IFaceImpl {
            iface,
            class,
            iface_generics,
            class_generics,
            methods,
        })
    }

    fn function(&mut self) -> Option<Function> {
        let sig = self.ex_func_declaration()?;
        let body = self.expression()?;
        Some(Function { sig, body })
    }

    fn variable(&mut self, mutable: bool) -> Option<Variable> {
        let name = self.consume(TType::Identifier, "Expected variable name.")?;
        self.consume(TType::Equal, "Expected '=' after variable name.");
        let initializer = self.expression()?;
        self.consume_semi_or_nl("Expected newline or ';' after variable declaration.");

        Some(Variable {
            name,
            mutable,
            initializer,
        })
    }

    /// A 'higher' expression is an expression that is only allowed to appear
    /// as top-level inside a block.
    /// This function can also produce a top-level non-higher expression.
    fn higher_expression(&mut self) -> Option<Expression> {
        Some(match () {
            _ if self.match_token(TType::Var) => Expression::VarDef(Box::new(self.variable(true)?)),
            _ if self.match_token(TType::Val) => Expression::VarDef(Box::new(self.variable(false)?)),
            _ => {
                let requires_semicolon = !NO_SEMICOLON.contains(&self.current.t_type);
                let expression = self.expression()?;
                if requires_semicolon {
                    self.consume_semi_or_nl("Expected newline or ';' after expression.");
                }
                expression
            }
        })
    }

    fn expression(&mut self) -> Option<Expression> {
        match () {
            _ if self.match_token(TType::LeftBrace) => self.block(),
            _ if self.match_token(TType::If) => self.if_expression(),
            _ if self.match_token(TType::Return) => self.return_expression(),
            _ if self.match_token(TType::Break) => self.break_expression(),
            _ if self.match_token(TType::For) => self.for_expression(),
            _ if self.match_token(TType::When) => self.when_expression(),
            _ => self.assignment(),
        }
    }

    fn block(&mut self) -> Option<Expression> {
        let mut expressions: Vec<Expression> = Vec::new();
        while !self.check(TType::RightBrace) && !self.is_at_end() {
            expressions.push(self.higher_expression()?);
        }

        self.consume(TType::RightBrace, "Expected '}' after block.");
        Some(Expression::Block(expressions))
    }

    fn if_expression(&mut self) -> Option<Expression> {
        self.consume(TType::LeftParen, "Expected '(' after 'if'.");
        let condition = Box::new(self.expression()?);
        self.consume(TType::RightParen, "Expected ')' after if condition.");
        let then_branch = Box::new(self.expression()?);

        let else_branch = if self.match_token(TType::Else) {
            Some(Box::new(self.expression()?))
        } else {
            None
        };

        Some(Expression::If {
            condition,
            then_branch,
            else_branch,
        })
    }

    fn for_expression(&mut self) -> Option<Expression> {
        self.consume(TType::LeftParen, "Expected '(' after 'for'.");

        if self.check_next(TType::From) {
            // for (var from x to y)
            let variable_name = self.consume(TType::Identifier, "Expected identifier after '('")?;
            self.consume(TType::From, "Expected 'from' after identifier.")?;

            let initial_value = self.expression()?;
            self.consume(TType::To, "Expected 'to' after starting value.")?;

            let last_value = self.expression()?;
            self.consume(TType::RightParen, "Expected ')' after for condition.");

            let last_value = Expression::Binary {
                left: Box::new(last_value),
                operator: Token::generic_token(TType::Minus),
                right: Box::new(Expression::Literal(Literal::I64(1))),
            };

            let variable = Expression::VarDef(Box::new(Variable {
                name: variable_name.clone(),
                mutable: true,
                initializer: Expression::Binary {
                    left: Box::new(initial_value),
                    operator: Token::generic_token(TType::Minus),
                    right: Box::new(Expression::Literal(Literal::I64(1))),
                },
            }));

            let var_increment = Expression::Assignment {
                name: variable_name.clone(),
                value: Box::new(Expression::Binary {
                    left: Box::new(Expression::Variable(variable_name.clone())),
                    operator: Token::generic_token(TType::Plus),
                    right: Box::new(Expression::Literal(Literal::I64(1))),
                }),
            };

            let body = self.expression()?;
            let else_b = if self.match_token(TType::Else) {
                Some(Box::new(self.expression()?))
            } else {
                None
            };

            let for_loop = Expression::For {
                condition: Box::new(Expression::Binary {
                    left: Box::new(Expression::Variable(variable_name.clone())),
                    operator: Token::generic_token(TType::BangEqual),
                    right: Box::new(last_value),
                }),
                body: Box::new(Expression::Block(vec![var_increment, body])),
                else_b,
            };

            Some(Expression::Block(vec![variable, for_loop]))
        } else {
            // for (condition)
            let condition = Box::new(self.expression()?);
            self.consume(TType::RightParen, "Expected ')' after for condition.");
            let body = Box::new(self.expression()?);

            let else_b = if self.match_token(TType::Else) {
                Some(Box::new(self.expression()?))
            } else {
                None
            };

            Some(Expression::For {
                condition,
                body,
                else_b,
            })
        }
    }

    fn return_expression(&mut self) -> Option<Expression> {
        let value = if !self.check_semi_or_nl() {
            Some(Box::new(self.expression()?))
        } else {
            None
        };

        Some(Expression::Return(value))
    }

    fn break_expression(&mut self) -> Option<Expression> {
        let value = if !self.check_semi_or_nl() {
            Some(Box::new(self.expression()?))
        } else {
            None
        };

        Some(Expression::Break(value))
    }

    fn when_expression(&mut self) -> Option<Expression> {
        self.consume(TType::LeftParen, "Expected '(' after 'when'.");
        let value = Box::new(self.expression()?);
        self.consume(TType::RightParen, "Expected ')' after when value.");
        self.consume(TType::LeftBrace, "Expected '{' after when value.");

        let mut branches: Vec<(Expression, Expression)> = Vec::new();
        let mut else_branch = None;
        while !self.match_token(TType::RightBrace) {
            if self.match_token(TType::Else) {
                if else_branch.is_some() {
                    self.error_at_current("'when' expression can only have 1 'else' branch.");
                }
                self.consume(TType::Arrow, "Expected '->' after when condition.");
                else_branch = Some(self.expression()?);
            } else {
                let condition = self.expression()?;
                self.consume(TType::Arrow, "Expected '->' after when condition.");
                let expression = self.expression()?;
                branches.push((condition, expression));
            }
        }
        if else_branch.is_none() {
            self.error_at_current("'when' expression is missing 'else' branch.");
        }

        Some(Expression::When {
            value,
            branches,
            else_branch: Box::new(else_branch?),
        })
    }

    fn closure(&mut self) -> Option<Expression> {
        self.consume(TType::LeftParen, "Expected '(' after closure.");

        let parameters = self.func_parameters()?;
        let return_type = if self.match_token(TType::Arrow) {
            Some(self.type_("Expected return type after '->'.")?)
        } else {
            None
        };

        let body = self.expression()?;

        Some(Expression::Literal(Literal::Closure(Rc::new(Function {
            sig: FuncSignature {
                name: Token::generic_identifier("closure".to_string()),
                return_type,
                parameters,
                generics: None
            },
            body,
        }))))
    }

    fn assignment(&mut self) -> Option<Expression> {
        let expression = self.logic_or()?;

        if self.match_token(TType::Equal) {
            let value = Box::new(self.expression()?);
            match expression {
                Expression::Variable(name) => Some(Expression::Assignment { name, value }),
                Expression::Get { object, name } => Some(Expression::Set {
                    object,
                    name,
                    value,
                }),
                _ => {
                    self.error_at_current("Invalid assignment target.");
                    None
                }
            }
        } else {
            Some(expression)
        }
    }

    /// See the macro at the top of the file for info on how this works.
    binary_op!(logic_or, logic_and, [TType::Or]);
    binary_op!(logic_and, equality, [TType::And]);
    binary_op!(equality, comparison, [TType::BangEqual, TType::EqualEqual]);
    binary_op!(
        comparison,
        addition,
        [
            TType::Less,
            TType::LessEqual,
            TType::Greater,
            TType::GreaterEqual
        ]
    );
    binary_op!(addition, multiplication, [TType::Plus, TType::Minus]);
    binary_op!(multiplication, unary, [TType::Star, TType::Slash]);

    fn unary(&mut self) -> Option<Expression> {
        Some(
            if let Some(operator) = self.match_tokens(&[TType::Bang, TType::Minus]) {
                let right = Box::new(self.unary()?);
                Expression::Unary { operator, right }
            } else {
                self.call()?
            },
        )
    }

    fn call(&mut self) -> Option<Expression> {
        let mut expression = self.primary()?;
        loop {
            match () {
                _ if self.match_token(TType::LeftParen) => {
                    let mut arguments: Vec<Expression> = Vec::new();
                    if !self.check(TType::RightParen) {
                        loop {
                            arguments.push(self.expression()?);
                            if !self.match_token(TType::Comma) {
                                break;
                            }
                        }
                    }

                    self.consume(TType::RightParen, "Expected ')' after call arguments.")?;
                    expression = Expression::Call {
                        callee: Box::new(expression),
                        arguments,
                    }
                }

                _ if self.match_token(TType::Dot) => {
                    expression = Expression::Get {
                        object: Box::new(expression),
                        name: self
                            .consume(TType::Identifier, "Expected property name after '.'.")?,
                    }
                }

                _ => break,
            }
        }
        Some(expression)
    }

    fn primary(&mut self) -> Option<Expression> {
        Some(match () {
            _ if self.match_token(TType::None) => Expression::Literal(Literal::None),
            _ if self.match_token(TType::False) => Expression::Literal(Literal::Bool(false)),
            _ if self.match_token(TType::True) => Expression::Literal(Literal::Bool(true)),
            _ if self.match_token(TType::LeftParen) => self.grouping()?,
            _ if self.check(TType::Identifier) => Expression::Variable(self.advance()),
            _ if self.check(TType::Int) => self.integer()?,
            _ if self.check(TType::Float) => self.float()?,
            _ if self.check(TType::String) => self.string(),
            _ if self.match_token(TType::Func) => self.closure()?,
            _ if self.match_token(TType::LeftBracket) => self.array()?,
            _ => {
                self.error_at_current("Expected expression.");
                None?
            }
        })
    }

    fn grouping(&mut self) -> Option<Expression> {
        let expression = self.expression()?;
        self.consume(TType::RightParen, "Expected ')' after expression.");
        Some(expression)
    }

    fn array(&mut self) -> Option<Expression> {
        let mut values: Vec<Expression> = Vec::new();
        loop {
            values.push(self.expression()?);
            if self.match_token(TType::RightBracket) {
                break;
            }
            self.consume(TType::Comma, "Expected ']' or ',' after array value.");
        }
        Some(Expression::Literal(Literal::Array(Either::Left(Rc::new(
            values,
        )))))
    }

    fn integer(&mut self) -> Option<Expression> {
        let token = self.advance();
        let mut split = token.lexeme.split('i');

        let literal = self.make_int_literal(split.next().unwrap(), split.next().unwrap_or("64"));
        if literal.is_none() {
            self.error_at_current("Invalid value for integer literal (Too big?).");
        }

        literal
    }

    fn make_int_literal(&mut self, num: &str, type_: &str) -> Option<Expression> {
        Some(Expression::Literal(match &type_[..] {
            "8" => Literal::I8(num.parse().ok()?),
            "16" => Literal::I16(num.parse().ok()?),
            "32" => Literal::I32(num.parse().ok()?),
            "64" => Literal::I64(num.parse().ok()?),
            _ => {
                self.error_at_current("Invalid integer size.")?;
                return None;
            }
        }))
    }

    fn float(&mut self) -> Option<Expression> {
        let token = self.advance();
        Some(Expression::Literal(match &token.lexeme[..1] {
            "f" => Literal::F32(token.lexeme.parse().ok()?),
            _ => Literal::F64(token.lexeme.parse().ok()?),
        }))
    }

    fn string(&mut self) -> Expression {
        let token = self.advance();
        Expression::Literal(Literal::String(token.lexeme))
    }

    // Reads an identifier followed by optional generic type parameters.
    fn generic_ident(&mut self) -> Option<(Token, Option<Vec<Token>>)> {
        let name = self.consume(TType::Identifier, "Expected a name.")?;
        let mut generics = None;
        if self.match_token(TType::Less) {
            let mut generics_vec = Vec::with_capacity(1);
            while let Some(type_) = self.match_tokens(&[TType::Identifier]) {
                generics_vec.push(type_);
                self.match_token(TType::Comma);
            }
            self.consume(TType::Greater, "Expected '>' after type parameters.")?;
            generics = Some(generics_vec)
        }
        Some((name, generics))
    }

    /// Reads a type name.
    fn type_(&mut self, msg: &str) -> Option<Type> {
        Some(match self.current.t_type {
            TType::Identifier => {
                let token = self.advance();

                if self.match_token(TType::Less) {
                    let mut types = Vec::new();
                    loop {
                        types.push(self.type_("Expected generic type.")?);
                        if !self.match_token(TType::Comma) {
                            break;
                        }
                    }
                    self.consume(TType::Greater, "Expected '>' after type parameters.")?;

                    Type::Generic { token, types }
                } else {
                    Type::Ident(token)
                }
            }

            TType::LeftBracket => {
                self.advance(); // consume '['
                let arr_type = self.type_("Expected type after '[' in array type.")?;
                self.consume(TType::RightBracket, "Expected ']' after array type.");
                Type::Array(Box::new(arr_type))
            }

            TType::LeftParen => {
                let mut params = Vec::new();
                loop {
                    params.push(self.type_("Expected closure parameter type.")?);
                    if !self.match_token(TType::Comma) {
                        break;
                    }
                }

                self.consume(TType::RightParen, "Expected ')' after closure parameters.")?;

                let ret_type = if self.match_token(TType::Arrow) {
                    Some(Box::new(self.type_("Expected return type after '->'.")?))
                } else {
                    None
                };

                Type::Closure { params, ret_type }
            }

            _ => {
                self.error_at_current(msg)?;
                None?
            }
        })
    }
}
