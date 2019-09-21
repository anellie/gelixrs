/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/21/19 2:16 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

//! This module contains all functions directly responsible for parsing the tokens
//! and creating the AST from them.

use std::rc::Rc;

use either::Either;

use crate::ast::declaration::ASTType;
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
    lexer::token::{Token, Type},
};

// All expressions that require no semicolon when used as a higher expression.
static NO_SEMICOLON: [Type; 3] = [Type::If, Type::LeftBrace, Type::When];

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
            Type::Class => module.classes.push(self.class_declaration()?),
            Type::Enum => module.enums.push(self.enum_declaration()?),
            Type::ExFn => module.ext_functions.push(self.ex_func_declaration()?),
            Type::Func => module.functions.push(self.function()?),
            Type::Import => module.imports.push(self.import_declaration()?),
            _ => self.error_at_current("Encountered invalid top-level declaration.")?,
        }

        Some(())
    }

    fn ex_func_declaration(&mut self) -> Option<FuncSignature> {
        let name = self.consume(Type::Identifier, "Expected an external function name.")?;
        self.consume(Type::LeftParen, "Expected '(' after function name.");

        let parameters = self.func_parameters()?;
        let return_type = if self.match_token(Type::Arrow) {
            Some(self.type_("Expected return type after '->'.")?)
        } else {
            None
        };

        Some(FuncSignature {
            name,
            return_type,
            parameters,
        })
    }

    fn func_parameters(&mut self) -> Option<Vec<FunctionArg>> {
        let mut parameters: Vec<FunctionArg> = Vec::new();
        if !self.check(Type::RightParen) {
            loop {
                parameters.push(FunctionArg {
                    _type: self.type_("Expected parameter type.")?,
                    name: self.consume(Type::Identifier, "Expected parameter name.")?,
                });
                if !self.match_token(Type::Comma) {
                    break;
                }
            }
        }
        self.consume(Type::RightParen, "Expected ')' after parameters.");
        Some(parameters)
    }

    fn class_declaration(&mut self) -> Option<Class> {
        let name = self.consume(Type::Identifier, "Expected a class name.")?;
        let mut generics = Vec::new();
        if self.match_token(Type::Less) {
            while let Some(type_) = self.match_tokens(&[Type::Identifier]) {
                generics.push(type_);
                self.match_token(Type::Comma);
            }
            self.consume(Type::Greater, "Expected '>' after type parameters.")?;
        }

        let superclass = if self.match_token(Type::Ext) {
            Some(self.consume(Type::Identifier, "Expected superclass name.")?)
        } else {
            None
        };

        self.consume(Type::LeftBrace, "Expected '{' before class body.");

        let mut methods: Vec<Function> = Vec::new();
        let mut variables: Vec<Variable> = Vec::new();

        while !self.check(Type::RightBrace) && !self.is_at_end() {
            match self.advance().t_type {
                Type::Func => methods.push(self.function()?),
                Type::Var => variables.push(self.variable(false)?),
                Type::Val => variables.push(self.variable(true)?),
                _ => self.error_at_current("Encountered invalid declaration inside class.")?,
            }
        }

        self.consume(Type::RightBrace, "Expected '}' after class body.");
        Some(Class {
            name,
            generics,
            superclass,
            methods,
            variables,
        })
    }

    fn enum_declaration(&mut self) -> Option<Enum> {
        let name = self.consume(Type::Identifier, "Expected an enum name.")?;
        self.consume(Type::LeftBrace, "Expected '{' before enum body.");

        let mut variants: Vec<Token> = Vec::new();
        while !self.check(Type::RightBrace) {
            variants.push(self.consume(Type::Identifier, "Expected enum variant.")?);
            if !self.match_token(Type::Comma) {
                break;
            }
        }
        self.consume(Type::RightBrace, "Expected '}' after enum body.");

        Some(Enum { name, variants })
    }

    fn import_declaration(&mut self) -> Option<Import> {
        let mut path = Vec::new();
        if !self.check(Type::Identifier) {
            self.error_at_current("Expected path after 'import'.")?
        }

        let mut consumed_slash = false;
        while self.check(Type::Identifier) || self.check(Type::Plus) {
            path.push(self.advance().lexeme);
            consumed_slash = self.match_token(Type::Slash);
        }
        if consumed_slash {
            self.error_at_current("Trailing '/' in import.")?
        }

        let symbol = path.pop().unwrap();
        Some(Import { path, symbol })
    }

    fn function(&mut self) -> Option<Function> {
        let sig = self.ex_func_declaration()?;
        let body = self.expression()?;
        Some(Function { sig, body })
    }

    fn variable(&mut self, is_val: bool) -> Option<Variable> {
        let name = self.consume(Type::Identifier, "Expected variable name.")?;
        self.consume(Type::Equal, "Expected '=' after variable name.");
        let initializer = self.expression()?;
        self.consume_semi_or_nl("Expected newline or ';' after variable declaration.");

        Some(Variable {
            name,
            is_val,
            initializer,
        })
    }

    /// A 'higher' expression is an expression that is only allowed to appear
    /// as top-level inside a block.
    /// This function can also produce a top-level non-higher expression.
    fn higher_expression(&mut self) -> Option<Expression> {
        Some(match () {
            _ if self.match_token(Type::Var) => Expression::VarDef(Box::new(self.variable(false)?)),
            _ if self.match_token(Type::Val) => Expression::VarDef(Box::new(self.variable(true)?)),
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
            _ if self.match_token(Type::LeftBrace) => self.block(),
            _ if self.match_token(Type::If) => self.if_expression(),
            _ if self.match_token(Type::Return) => self.return_expression(),
            _ if self.match_token(Type::Break) => self.break_expression(),
            _ if self.match_token(Type::For) => self.for_expression(),
            _ if self.match_token(Type::When) => self.when_expression(),
            _ => self.assignment(),
        }
    }

    fn block(&mut self) -> Option<Expression> {
        let mut expressions: Vec<Expression> = Vec::new();
        while !self.check(Type::RightBrace) && !self.is_at_end() {
            expressions.push(self.higher_expression()?);
        }

        self.consume(Type::RightBrace, "Expected '}' after block.");
        Some(Expression::Block(expressions))
    }

    fn if_expression(&mut self) -> Option<Expression> {
        self.consume(Type::LeftParen, "Expected '(' after 'if'.");
        let condition = Box::new(self.expression()?);
        self.consume(Type::RightParen, "Expected ')' after if condition.");
        let then_branch = Box::new(self.expression()?);

        let else_branch = if self.match_token(Type::Else) {
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
        self.consume(Type::LeftParen, "Expected '(' after 'for'.");

        if self.check_next(Type::From) {
            // for (var from x to y)
            let variable_name = self.consume(Type::Identifier, "Expected identifier after '('")?;
            self.consume(Type::From, "Expected 'from' after identifier.")?;

            let initial_value = self.expression()?;
            self.consume(Type::To, "Expected 'to' after starting value.")?;

            let last_value = self.expression()?;
            self.consume(Type::RightParen, "Expected ')' after for condition.");

            let last_value = Expression::Binary {
                left: Box::new(last_value),
                operator: Token::generic_token(Type::Minus),
                right: Box::new(Expression::Literal(Literal::I64(1))),
            };

            let variable = Expression::VarDef(Box::new(Variable {
                name: variable_name.clone(),
                is_val: false,
                initializer: Expression::Binary {
                    left: Box::new(initial_value),
                    operator: Token::generic_token(Type::Minus),
                    right: Box::new(Expression::Literal(Literal::I64(1))),
                },
            }));

            let var_increment = Expression::Assignment {
                name: variable_name.clone(),
                value: Box::new(Expression::Binary {
                    left: Box::new(Expression::Variable(variable_name.clone())),
                    operator: Token::generic_token(Type::Plus),
                    right: Box::new(Expression::Literal(Literal::I64(1))),
                }),
            };

            let body = self.expression()?;
            let else_b = if self.match_token(Type::Else) {
                Some(Box::new(self.expression()?))
            } else {
                None
            };

            let for_loop = Expression::For {
                condition: Box::new(Expression::Binary {
                    left: Box::new(Expression::Variable(variable_name.clone())),
                    operator: Token::generic_token(Type::BangEqual),
                    right: Box::new(last_value),
                }),
                body: Box::new(Expression::Block(vec![var_increment, body])),
                else_b,
            };

            Some(Expression::Block(vec![variable, for_loop]))
        } else {
            // for (condition)
            let condition = Box::new(self.expression()?);
            self.consume(Type::RightParen, "Expected ')' after for condition.");
            let body = Box::new(self.expression()?);

            let else_b = if self.match_token(Type::Else) {
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
        self.consume(Type::LeftParen, "Expected '(' after 'when'.");
        let value = Box::new(self.expression()?);
        self.consume(Type::RightParen, "Expected ')' after when value.");
        self.consume(Type::LeftBrace, "Expected '{' after when value.");

        let mut branches: Vec<(Expression, Expression)> = Vec::new();
        let mut else_branch = None;
        while !self.match_token(Type::RightBrace) {
            if self.match_token(Type::Else) {
                if else_branch.is_some() {
                    self.error_at_current("'when' expression can only have 1 'else' branch.");
                }
                self.consume(Type::Arrow, "Expected '->' after when condition.");
                else_branch = Some(self.expression()?);
            } else {
                let condition = self.expression()?;
                self.consume(Type::Arrow, "Expected '->' after when condition.");
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
        self.consume(Type::LeftParen, "Expected '(' after closure.");

        let parameters = self.func_parameters()?;
        let return_type = if self.match_token(Type::Arrow) {
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
            },
            body,
        }))))
    }

    fn assignment(&mut self) -> Option<Expression> {
        let expression = self.logic_or()?;

        if self.match_token(Type::Equal) {
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
    binary_op!(logic_or, logic_and, [Type::Or]);
    binary_op!(logic_and, equality, [Type::And]);
    binary_op!(equality, comparison, [Type::BangEqual, Type::EqualEqual]);
    binary_op!(
        comparison,
        addition,
        [
            Type::Less,
            Type::LessEqual,
            Type::Greater,
            Type::GreaterEqual
        ]
    );
    binary_op!(addition, multiplication, [Type::Plus, Type::Minus]);
    binary_op!(multiplication, unary, [Type::Star, Type::Slash]);

    fn unary(&mut self) -> Option<Expression> {
        Some(
            if let Some(operator) = self.match_tokens(&[Type::Bang, Type::Minus]) {
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
                _ if self.match_token(Type::LeftParen) => {
                    let mut arguments: Vec<Expression> = Vec::new();
                    if !self.check(Type::RightParen) {
                        loop {
                            arguments.push(self.expression()?);
                            if !self.match_token(Type::Comma) {
                                break;
                            }
                        }
                    }

                    self.consume(Type::RightParen, "Expected ')' after call arguments.")?;
                    expression = Expression::Call {
                        callee: Box::new(expression),
                        arguments,
                    }
                }

                _ if self.match_token(Type::Dot) => {
                    expression = Expression::Get {
                        object: Box::new(expression),
                        name: self
                            .consume(Type::Identifier, "Expected property name after '.'.")?,
                    }
                }

                // TODO deduplicate this
                _ if self.match_token(Type::Less) => {
                    let mut types = Vec::new();
                    loop {
                        types.push(self.type_("Expected generic type.")?);
                        if !self.match_token(Type::Comma) { break; }
                    }

                    self.consume(Type::Greater, "Expected '>' after type parameters.")?;
                    self.consume(Type::LeftParen, "Expected '(' after type parameters.")?;

                    let mut arguments: Vec<Expression> = Vec::new();
                    if !self.check(Type::RightParen) {
                        loop {
                            arguments.push(self.expression()?);
                            if !self.match_token(Type::Comma) {
                                break;
                            }
                        }
                    }

                    self.consume(Type::RightParen, "Expected ')' after call arguments.")?;
                    expression = Expression::CallWithGeneric {
                        callee: Box::new(expression),
                        types,
                        arguments,
                    }

                }

                _ => break,
            }
        }
        Some(expression)
    }

    fn primary(&mut self) -> Option<Expression> {
        Some(match () {
            _ if self.match_token(Type::None) => Expression::Literal(Literal::None),
            _ if self.match_token(Type::False) => Expression::Literal(Literal::Bool(false)),
            _ if self.match_token(Type::True) => Expression::Literal(Literal::Bool(true)),
            _ if self.match_token(Type::LeftParen) => self.grouping()?,
            _ if self.check(Type::Identifier) => Expression::Variable(self.advance()),
            _ if self.check(Type::Int) => self.integer()?,
            _ if self.check(Type::Float) => self.float()?,
            _ if self.check(Type::String) => self.string(),
            _ if self.match_token(Type::Func) => self.closure()?,
            _ if self.match_token(Type::LeftBracket) => self.array()?,
            _ => {
                self.error_at_current("Expected expression.");
                None?
            }
        })
    }

    fn grouping(&mut self) -> Option<Expression> {
        let expression = self.expression()?;
        self.consume(Type::RightParen, "Expected ')' after expression.");
        Some(Expression::Grouping(Box::new(expression)))
    }

    fn array(&mut self) -> Option<Expression> {
        let mut values: Vec<Expression> = Vec::new();
        loop {
            values.push(self.expression()?);
            if self.match_token(Type::RightBracket) {
                break;
            }
            self.consume(Type::Comma, "Expected ']' or ',' after array value.");
        }
        Some(Expression::Literal(Literal::Array(Either::Left(Rc::new(values)))))
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

    /// Reads a type name.
    fn type_(&mut self, msg: &str) -> Option<ASTType> {
        Some(match self.current.t_type {
            Type::Identifier => {
                let token = self.advance();

                if self.match_token(Type::Less) {
                    let mut types = Vec::new();
                    loop {
                        types.push(self.type_("Expected generic type.")?);
                        if !self.match_token(Type::Comma) { break; }
                    }
                    self.consume(Type::Greater, "Expected '>' after type parameters.")?;

                    ASTType::Generic {
                        token,
                        types
                    }
                } else {
                    ASTType::Token(token)
                }
            },

            Type::LeftBracket => {
                self.advance(); // consume '['
                let arr_type = self.type_("Expected type after '[' in array type.")?;
                self.consume(Type::RightBracket, "Expected ']' after array type.");
                ASTType::Array(Box::new(arr_type))
            }

            Type::LeftParen => {
                let mut params = Vec::new();
                loop {
                    params.push(self.type_("Expected closure parameter type.")?);
                    if !self.match_token(Type::Comma) { break; }
                }

                self.consume(Type::RightParen, "Expected ')' after closure parameters.")?;

                let ret_type = if self.match_token(Type::Arrow) {
                    Some(Box::new(self.type_("Expected return type after '->'.")?))
                } else {
                    None
                };

                ASTType::Closure { params, ret_type }
            }

            _ => {
                self.error_at_current(msg)?;
                None?
            }
        })
    }
}
