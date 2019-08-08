//! This module contains all functions directly responsible for parsing the tokens
//! and creating the AST from them.

use super::super::{
    ast::{
        declaration::{Declaration, Function, Variable},
        expression::Expression,
        literal::Literal,
        statement::Statement,
    },
    lexer::token::{Token, Type},
};
use super::Parser;

// TODO: Implement the rest of the parser.
impl<'p> Parser<'p> {
    /// The entry point for generating a statement.
    /// The reason for returning Option is that the parser will error out and abort the current
    /// statement when illegal syntax is encountered.
    pub fn declaration(&mut self) -> Option<Declaration<'p>> {
        if self.waiting_for_sync {
            self.syncronize();
        }

        Some(match () {
            _ if self.match_token(Type::Class) => self.class_declaration()?,
            _ if self.match_token(Type::Enum) => self.enum_declaration()?,
            _ if self.match_token(Type::Func) => Declaration::Function(self.function()?),
            _ => {
                self.error_at_current("Encountered invalid top-level declaration.");
                None?
            },
        })
    }

    fn class_declaration(&mut self) -> Option<Declaration<'p>> {
        let name = self.consume(Type::Identifier, "Expected a class name.")?;
        self.consume(Type::LeftBrace, "Expected '{' before class body.");

        let mut methods: Vec<Function> = Vec::new();
        let mut variables: Vec<Variable> = Vec::new();

        while !self.check(Type::RightBrace) && !self.is_at_end() {
            match () {
                _ if self.match_token(Type::Func) => if let Some(f) = self.function() { methods.push(f) },
                _ if self.match_token(Type::Var) => if let Some(v) = self.variable(false) { variables.push(v) },
                _ if self.match_token(Type::Val) => if let Some(v) = self.variable(true) { variables.push(v) },
                _ => (),
            }
        }

        self.consume(Type::RightBrace, "Expected '}' after class body.");
        Some(Declaration::Class {
            name,
            methods,
            variables,
        })
    }

    fn enum_declaration(&mut self) -> Option<Declaration<'p>> {
        let name = self.consume(Type::Identifier, "Expected an enum name.")?;
        self.consume(Type::LeftBrace, "Expected '{' before enum body.");

        let mut variants: Vec<Token> = Vec::new();
        while !self.check(Type::RightBrace) {
            variants.push(self.consume(Type::Identifier, "Expected enum variant.")?);
            if !self.match_token(Type::Comma) { break; }
        }
        self.consume(Type::RightBrace, "Expected '}' after enum body.");

        Some(Declaration::Enum { name, variants })
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

    fn variable(&mut self, is_val: bool) -> Option<Variable<'p>> {
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

    fn statement(&mut self) -> Option<Statement<'p>> {
        Some(match () {
            _ if self.match_token(Type::LeftBrace) => self.block()?,
            _ if self.match_token(Type::Error) => self.error_statement()?,
            _ if self.match_token(Type::For) => self.for_statement()?,
            _ if self.match_token(Type::If) => self.if_expr_or_stmt()?,
            _ if self.match_token(Type::Return) => self.return_statement()?,
            _ if self.match_token(Type::Var) => Statement::Variable(self.variable(false)?),
            _ if self.match_token(Type::Val) => Statement::Variable(self.variable(true)?),
            _ => self.expression_statement()?,
        })
    }

    fn error_statement(&mut self) -> Option<Statement<'p>> {
        let mut value = None;
        if !self.check_semi_or_nl() {
            value = Some(self.expression()?);
        }
        self.consume_semi_or_nl("Expected newline or ';' after 'error'.");
        Some(Statement::Error(value))
    }

    fn return_statement(&mut self) -> Option<Statement<'p>> {
        let mut value = None;
        if !self.check_semi_or_nl() {
            value = Some(self.expression()?);
        }
        self.consume_semi_or_nl("Expected newline or ';' after 'return'.");
        Some(Statement::Return(value))
    }

    fn for_statement(&mut self) -> Option<Statement<'p>> {
        self.consume(Type::LeftParen, "Expected '(' after 'for'.");
        
        Some(//if self.check_next(Type::In) { // for (x in y)
            // TODO: Implement "for each in" loops
        /*} else*/ { // for (condition)
            let condition = self.expression()?;
            self.consume(Type::RightParen, "Expected ')' after for condition.");
            let body = Box::new(self.statement()?);

            Statement::For { condition, body }
        })
    }

    fn if_expr_or_stmt(&mut self) -> Option<Statement<'p>> {
        self.consume(Type::LeftParen, "Expected '(' after 'if'.");
        let condition = Box::new(self.expression()?);
        self.consume(Type::RightParen, "Expected ')' after if condition.");
        let then_branch = Box::new(self.statement()?);

        // TODO: Scratch my eyes out at the mostrosity that I have created here...
        if self.match_token(Type::Else) {
            let else_branch = Box::new(self.statement()?);

            if let Statement::Expression(then_branch) = *then_branch {
                if let Statement::Expression(else_branch) = *else_branch {
                    Some(Statement::Expression(Expression::IfElse {
                        condition,
                        then_branch: Box::new(then_branch),
                        else_branch: Box::new(else_branch)
                    }))
                } else {
                    Some(Statement::If {
                        condition,
                        then_branch: Box::new(Statement::Expression(then_branch)),
                        else_branch: Some(else_branch),
                    })
                }
            } else {
                Some(Statement::If {
                    condition,
                    then_branch,
                    else_branch: Some(else_branch),
                })
            }
        } else {
            Some(Statement::If {
                condition,
                then_branch,
                else_branch: None,
            })
        }

    }

    fn block(&mut self) -> Option<Statement<'p>> {
        let mut statements: Vec<Statement> = Vec::new();
        while !self.check(Type::RightBrace) && !self.is_at_end() {
            if let Some(f) = self.statement() {
                statements.push(f)
            }
        }

        self.consume(Type::RightBrace, "Expected '}' after block.");
        
        // Handle edge case of empty block; would otherwise result in None being incorrectly returned
        if statements.is_empty() {
            return Some(Statement::Block(statements))
        }

        if let Statement::Expression(_) = statements.last()? {
            Some(Statement::Expression(Expression::Block(statements)))
        } else {
            Some(Statement::Block(statements))
        }
    }

    fn expression_statement(&mut self) -> Option<Statement<'p>> {
        let requires_semicolon = ![Type::If, Type::Take, Type::LeftBrace, Type::When].contains(&self.current.t_type);
        let statement = Statement::Expression(self.expression()?);
        if requires_semicolon {
            self.consume_semi_or_nl("Expected newline or ';' after expression.");
        }
        Some(statement)
    }

    // TODO: Some redundant code...
    fn expression(&mut self) -> Option<Expression<'p>> {
        Some(match () {
            _ if self.match_token(Type::Take) => self.take_expression()?,
            _ if self.match_token(Type::When) => self.when_expression()?,
            _ if self.match_token(Type::LeftBrace) => {
                if let Statement::Expression(expr) = self.block()? {
                    expr
                } else {
                    self.error_at_current("Last statement of the block needs to be an expression for the block to be an expression.");
                    None?
                }
            },
            _ if self.match_token(Type::If) => {
                if let Statement::Expression(expr) = self.if_expr_or_stmt()? {
                    expr
                } else {
                    self.error_at_current("If requires expressions as arms to be used as an expression.");
                    None?
                }
            },
            _ => self.assignment()?,
        })
    }

    fn take_expression(&mut self) -> Option<Expression<'p>> {
        let value = Box::new(self.expression()?);
        let mut else_branch = None;
        if self.match_token(Type::Else) {
            else_branch = Some(Box::new(self.expression()?));
        }
        Some(Expression::Take { value, else_branch })
    }

    /// TODO: Consider unrolling when into if-elseif-else constructs.
    /// Update: Don't. LLVM provides a much more convinient way.
    fn when_expression(&mut self) -> Option<Expression<'p>> {
        self.consume(Type::LeftParen, "Expected '(' after 'when'.");
        let value = Box::new(self.expression()?);
        self.consume(Type::RightParen, "Expected ')' after when value.");
        self.consume(Type::LeftBrace, "Expected '{' after when value.");

        let mut branches: Vec<(Expression<'p>, Expression<'p>)> = Vec::new();
        let mut else_branch = None;
        while !self.check(Type::RightBrace) {
            if self.match_token(Type::Else) {
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

    fn assignment(&mut self) -> Option<Expression<'p>> {
        let expression = self.logic_or()?;

        if self.match_token(Type::Equal) {
            let value = Box::new(self.assignment()?);
            match expression {
                Expression::Variable(name) => Some(Expression::Assignment { name, value }),
                Expression::Get { object, name } => Some(Expression::Set {
                    object: object,
                    name: name,
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

    // TODO: The 6 functions below are mostly identical; refactor it to be less ew

    fn logic_or(&mut self) -> Option<Expression<'p>> {
        let mut left = self.logic_and()?;
        while let Some(operator) = self.match_tokens(&[Type::Or]) {
            let right = self.logic_and()?;
            left = Expression::Logical {
                left: Box::new(left), operator, right: Box::new(right)
            }
        }
        Some(left)
    }

    fn logic_and(&mut self) -> Option<Expression<'p>> {
        let mut left = self.equality()?;
        while let Some(operator) = self.match_tokens(&[Type::And]) {
            let right = self.equality()?;
            left = Expression::Logical {
                left: Box::new(left), operator, right: Box::new(right)
            }
        }
        Some(left)
    }

    fn equality(&mut self) -> Option<Expression<'p>> {
        let mut left = self.comparison()?;
        while let Some(operator) = self.match_tokens(&[Type::BangEqual, Type::EqualEqual]) {
            let right = self.comparison()?;
            left = Expression::Binary {
                left: Box::new(left), operator, right: Box::new(right)
            }
        }
        Some(left)
    }

    fn comparison(&mut self) -> Option<Expression<'p>> {
        let mut left = self.addition()?;
        while let Some(operator) = self.match_tokens(&[Type::Greater, Type::Less, Type::GreaterEqual, Type::LessEqual]) {
            let right = self.addition()?;
            left = Expression::Binary {
                left: Box::new(left), operator, right: Box::new(right)
            }
        }
        Some(left)
    }
        
    fn addition(&mut self) -> Option<Expression<'p>> {
        let mut left = self.multiplication()?;
        while let Some(operator) = self.match_tokens(&[Type::Minus, Type::Plus]) {
            let right = self.multiplication()?;
            left = Expression::Binary {
                left: Box::new(left), operator, right: Box::new(right)
            }
        }
        Some(left)
    }
        
    fn multiplication(&mut self) -> Option<Expression<'p>> {
        let mut left = self.unary()?;
        while let Some(operator) = self.match_tokens(&[Type::Slash, Type::Star]) {
            let right = self.unary()?;
            left = Expression::Binary {
                left: Box::new(left), operator, right: Box::new(right)
            }
        }
        Some(left)
    }

    fn unary(&mut self) -> Option<Expression<'p>> {
        Some(if let Some(operator) = self.match_tokens(&[Type::Bang, Type::Minus]) {
            let right = Box::new(self.unary()?);
            Expression::Unary {
                operator,
                right
            }
        } else {
            self.call()?
        })
    }

    fn call(&mut self) -> Option<Expression<'p>> {
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

                    let paren = self.consume(Type::RightParen, "Expected ')' after arguments.")?;
                    expression = Expression::Call {
                        callee: Box::new(expression),
                        token: paren,
                        arguments
                    }
                },  
                _ if self.match_token(Type::Dot) => {
                    expression = Expression::Get {
                        object: Box::new(expression),
                        name: self.consume(Type::Identifier, "Expected property name after '.'.")?,
                    }
                },
                _ => break,
            }
        }
        Some(expression)
    }

    // TODO: Support for array literals
    fn primary(&mut self) -> Option<Expression<'p>> {
        Some(match () {
            _ if self.match_token(Type::Null) => Expression::Literal(Literal::Null),
            _ if self.match_token(Type::False) => Expression::Literal(Literal::Bool(false)),
            _ if self.match_token(Type::True) => Expression::Literal(Literal::Bool(true)),
            _ if self.match_token(Type::LeftParen) => self.grouping()?,
            _ if self.check(Type::Identifier) => Expression::Variable(self.advance()),
            _ if self.check(Type::This) => Expression::This(self.advance()),
            _ if self.check(Type::Int) => self.integer()?,
            _ if self.check(Type::Float) => self.float()?,
            _ if self.check(Type::String) => self.string(),
            _ => {
                self.error_at_current("Expected primary expression.");
                None? // lol
            }
        })
    }

    fn grouping(&mut self) -> Option<Expression<'p>> {
        let expression = self.expression()?;
        self.consume(Type::RightParen, "Expected ')' after expression.");
        Some(Expression::Grouping(Box::new(expression)))
    }

    fn integer(&mut self) -> Option<Expression<'p>> {
        let token = self.advance();
        Some(Expression::Literal(Literal::Int(
            token.lexeme.parse().ok()?,
        )))
    }

    fn float(&mut self) -> Option<Expression<'p>> {
        // TODO: Support for single-prec float
        let token = self.advance();
        Some(Expression::Literal(Literal::Double(token.lexeme.parse().ok()?)))
    }

    fn string(&mut self) -> Expression<'p> {
        let token = self.advance();
        Expression::Literal(Literal::String(token.lexeme.to_string()))
    }
}
