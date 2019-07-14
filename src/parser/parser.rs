//! This module contains all functions directly responsible for parsing the tokens
//! and creating the AST from them.

use super::super::{
    ast::{
        expression::Expression,
        literal::Literal,
        statement::{Function, Statement, Variable},
    },
    token::{Token, Type},
};
use super::Parser;

// TODO: Implement the rest of the parser.
impl<'p> Parser<'p> {
    /// The entry point for generating a statement.
    /// The reason for returning Option is that the parser will error out and abort the current
    /// statement when illegal syntax is encountered.
    pub fn declaration(&mut self) -> Option<Statement<'p>> {
        if self.waiting_for_sync {
            self.syncronize();
        }

        Some(match () {
            _ if self.match_token(Type::Class) => self.class_declaration()?,
            _ if self.match_token(Type::Func) => Statement::Function(self.function()?),
            _ if self.match_token(Type::Var) => Statement::Variable(self.var_declaration(false)?),
            _ if self.match_token(Type::Val) => Statement::Variable(self.var_declaration(true)?),
            _ => self.statement()?,
        })
    }

    fn class_declaration(&mut self) -> Option<Statement<'p>> {
        let name = self.consume(Type::Identifier, "Expected a class name.")?;
        self.consume(Type::LeftBrace, "Expected '{' before class body.");

        let mut methods: Vec<Function> = Vec::new();
        let mut variables: Vec<Variable> = Vec::new();

        while !self.check(Type::RightBrace) && !self.is_at_end() {
            match () {
                _ if self.match_token(Type::Func) => if let Some(f) = self.function() { methods.push(f) },
                _ if self.match_token(Type::Var) => if let Some(v) = self.var_declaration(false) { variables.push(v) },
                _ if self.match_token(Type::Val) => if let Some(v) = self.var_declaration(true) { variables.push(v) },
                _ => (),
            }
        }

        self.consume(Type::RightBrace, "Expected '}' after class body.");
        Some(Statement::Class {
            name,
            methods,
            variables,
        })
    }

    fn function(&mut self) -> Option<Function<'p>> {
        let name = self.consume(Type::Identifier, "Expected a function name.")?; 
        self.consume(Type::LeftParen, "Expected '(' after function name.");

        let mut parameters: Vec<(Token<'p>, Token<'p>)> = Vec::new();
        if !self.check(Type::RightParen) {
            loop {
                parameters.push((
                    self.consume(Type::Identifier, "Expected parameter type.")?,
                    self.consume(Type::Identifier, "Expected parameter name.")?,
                ));
                if !self.match_token(Type::Comma) {
                    break;
                }
            }
        }
        self.consume(Type::RightParen, "Expected ')' after parameters.");

        let mut return_type: Option<Token<'p>> = None;
        if self.match_token(Type::Arrow) {
            return_type = self.consume(Type::Identifier, "Expected return type after '->'.");
        }

        let body = self.statement()?;

        Some(Function {
            name,
            return_type,
            parameters,
            body: Box::new(body),
        })
    }

    fn var_declaration(&mut self, is_val: bool) -> Option<Variable<'p>> {
        let name = self.consume(Type::Identifier, "Expected variable name.")?;

        let mut initializer: Option<Expression<'p>> = None;
        if self.match_token(Type::Equal) {
            initializer = Some(self.expression());
        }

        if !self.hit_newline {
            self.error_at_current("Expected newline after variable declaration.");
        }

        Some(Variable {
            name,
            is_val,
            initializer,
        })
    }

    fn statement(&mut self) -> Option<Statement<'p>> {
        match () {
            _ => self.expression_statement(),
        }
    }

    fn expression_statement(&mut self) -> Option<Statement<'p>> {
        let statement = Statement::Expression(self.expression());
        if !self.hit_newline {
            self.error_at_current("Expected newline after expression.");
        }
        Some(statement)
    }

    fn expression(&mut self) -> Expression<'p> {
        self.advance();
        Expression::Literal(Literal::Int(4546))
    }
}
