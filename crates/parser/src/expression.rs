use crate::Parser;
use syntax::kind::SyntaxKind;

impl<'p> Parser<'p> {
    /// A 'higher' expression is an expression that is only allowed to appear
    /// as top-level inside a block.
    /// This function can also produce a top-level non-higher expression.
    fn higher_expression(&mut self) {
        match self.peek() {
            SyntaxKind::Var | SyntaxKind::Val => self.variable(),
            _ => self.expression(),
        }
    }

    fn variable(&mut self) {
        self.start_node(SyntaxKind::Variable);
        self.advance(); // Consume 'var' or 'val'
        self.consume(SyntaxKind::Identifier, "Expected variable name.");
        self.consume(SyntaxKind::Equal, "Expected '=' after variable name.");
        self.node_with(SyntaxKind::Initializer, Self::expression);
        self.end_node();
    }

    pub fn expression(&mut self) {
        match self.peek() {
            SyntaxKind::LeftBrace => self.block(),
            SyntaxKind::If => self.if_expression(),
            SyntaxKind::For => self.for_expression(),
            SyntaxKind::Return => self.ret_or_break_expr(SyntaxKind::ReturnExpr),
            SyntaxKind::Break => self.ret_or_break_expr(SyntaxKind::BreakExpr),
            SyntaxKind::When => self.when_expression(),
            _ => self.binary(0),
        }
    }

    fn block(&mut self) {
        self.start_node(SyntaxKind::Block);
        self.advance(); // Consume '{'
        while !self.check(SyntaxKind::RightBrace) && !self.is_at_end() {
            self.higher_expression();
        }
        self.consume(SyntaxKind::RightBrace, "Expected '}' after block.");
        self.end_node();
    }

    fn if_expression(&mut self) {
        self.start_node(SyntaxKind::IfExpr);
        self.advance(); // Consume 'if'
        self.consume(SyntaxKind::LeftParen, "Expected '(' after 'if'.");

        self.node_with(SyntaxKind::ExprCondition, Self::expression);

        self.consume(SyntaxKind::RightParen, "Expected ')' after if condition.");
        self.node_with(SyntaxKind::ExprBody, Self::expression);

        if self.matches(SyntaxKind::Else) {
            self.node_with(SyntaxKind::ExprElse, Self::expression);
        }

        self.end_node();
    }

    fn for_expression(&mut self) {
        self.start_node(SyntaxKind::ForExpr);
        self.advance(); // Consume 'for'
        self.consume(SyntaxKind::LeftParen, "Expected '(' after 'for'.");

        if self.check_next(SyntaxKind::In) {
            // for (item in iterator)
            self.start_node(SyntaxKind::ForIterCond);
            self.consume(SyntaxKind::Identifier, "Expected item name after '('");
            self.advance(); // Consume the `in`
            self.expression();
            self.end_node();
        } else {
            // for (condition)
            self.node_with(SyntaxKind::ExprCondition, Self::expression);
        }

        self.consume(SyntaxKind::RightParen, "Expected ')' after for.");
        self.node_with(SyntaxKind::ExprBody, Self::expression);
        if self.matches(SyntaxKind::Else) {
            self.node_with(SyntaxKind::ExprElse, Self::expression);
        }

        self.end_node();
    }

    fn ret_or_break_expr(&mut self, kind: SyntaxKind) {
        self.start_node(kind);
        self.advance(); // Consume name
        if !self.check_separator() {
            self.expression()
        }
        self.end_node();
    }

    fn when_expression(&mut self) {
        self.start_node(SyntaxKind::WhenExpr);
        self.advance(); // Consume 'when'
        self.consume(SyntaxKind::LeftParen, "Expected '(' after 'when'.");
        self.node_with(SyntaxKind::ExprCondition, Self::expression);
        self.consume(SyntaxKind::RightParen, "Expected ')' after when value.");
        self.consume(SyntaxKind::LeftBrace, "Expected '{' after when value.");

        let mut else_branch_found = false;
        while !self.matches(SyntaxKind::RightBrace) {
            self.start_node(SyntaxKind::WhenBranch);
            if self.matches(SyntaxKind::Else) {
                if else_branch_found {
                    self.error_at_current("'when' expression can only have 1 'else' branch.");
                }
                self.consume(SyntaxKind::Arrow, "Expected '->' after when condition.");
                self.node_with(SyntaxKind::ExprBody, Self::expression);
                else_branch_found = true;
            } else {
                self.node_with(SyntaxKind::ExprCondition, Self::expression);
                self.consume(SyntaxKind::Arrow, "Expected '->' after when condition.");
                self.node_with(SyntaxKind::ExprBody, Self::expression);
            }
            self.end_node()
        }

        self.end_node();
    }

    fn binary(&mut self, minimum_binding_power: u8) {
        let checkpoint = self.checkpoint();
        self.unary();

        while let Some((lbp, rbp)) = self.peek().infix_binding_power() {
            if lbp < minimum_binding_power {
                return;
            }

            self.node_with(SyntaxKind::Operator, |this| {
                this.advance();
            });

            self.start_node_at(checkpoint, SyntaxKind::BinaryExpr);
            self.binary(rbp);
            self.end_node();
        }
    }

    fn unary(&mut self) {
        if let Some(rbp) = self.peek().prefix_binding_power() {
            self.start_node(SyntaxKind::PrefixExpr);
            self.node_with(SyntaxKind::Operator, |this| {
                this.advance();
            });
            self.binary(rbp);
            self.end_node();
        } else {
            self.call();
        }
    }

    fn call(&mut self) {
        let checkpoint = self.checkpoint();
        self.primary();

        loop {
            match self.peek() {
                SyntaxKind::LeftParen => {
                    self.start_node_at(checkpoint, SyntaxKind::Callee);
                    self.start_node_at(checkpoint, SyntaxKind::CallExpr);
                    self.end_node();

                    self.advance(); // Consume '('
                    if !self.check(SyntaxKind::RightParen) {
                        loop {
                            self.node_with(SyntaxKind::CallArgument, Self::expression);
                            if !self.matches(SyntaxKind::Comma) {
                                break;
                            }
                        }
                    }

                    self.consume(SyntaxKind::RightParen, "Expected ')' after call arguments.");
                    self.end_node();
                }

                SyntaxKind::Dot => {
                    self.start_node_at(checkpoint, SyntaxKind::Callee);
                    self.start_node_at(checkpoint, SyntaxKind::GetExpr);
                    self.end_node();

                    self.advance(); // Consume '.'
                    self.identifier();
                    self.end_node();
                }

                SyntaxKind::Colon => {
                    self.start_node_at(checkpoint, SyntaxKind::Callee);
                    self.start_node_at(checkpoint, SyntaxKind::GetStaticExpr);
                    self.end_node();

                    self.advance(); // Consume ':'
                    self.consume(SyntaxKind::Identifier, "Expected property name after ':'.");
                    self.end_node();
                }

                _ => break,
            }
        }
    }

    fn primary(&mut self) {
        match self.peek() {
            SyntaxKind::False
            | SyntaxKind::True
            | SyntaxKind::Int
            | SyntaxKind::Float
            | SyntaxKind::String => {
                self.start_node(SyntaxKind::Literal);
                self.advance();
                self.end_node();
            }
            SyntaxKind::LeftParen => self.grouping_or_closure(),
            SyntaxKind::Identifier => self.identifier(),
            _ => self.error_at_current("Expected expression."),
        }
    }

    fn identifier(&mut self) {
        self.start_node(SyntaxKind::Ident);
        self.advance();

        if self.matches(SyntaxKind::LeftBracket) {
            loop {
                self.type_("Expected generic type.");
                if !self.matches(SyntaxKind::Comma) {
                    break;
                }
            }
            self.consume(
                SyntaxKind::RightBracket,
                "Expected ']' after type parameters.",
            );
        }
        self.end_node()
    }

    fn grouping_or_closure(&mut self) {
        if (self.check(SyntaxKind::Identifier)
            && (self.check_next(SyntaxKind::Colon) || self.check_next(SyntaxKind::Comma)))
            || self.check(SyntaxKind::RightParen)
        {
            self.closure()
        } else {
            self.grouping()
        }
    }

    fn grouping(&mut self) {
        self.start_node(SyntaxKind::Grouping);
        self.advance();
        self.expression();
        self.consume(SyntaxKind::RightParen, "Expected ')' after expression.");
        self.end_node();
    }

    fn closure(&mut self) {
        self.start_node(SyntaxKind::ClosureLiteral);
        self.advance();

        self.start_node(SyntaxKind::FunctionSignature);
        self.func_parameters();
        if self.matches(SyntaxKind::Colon) {
            self.type_("Expected return type after ':'.")
        }
        self.end_node();

        self.consume(SyntaxKind::Arrow, "Expected '->' after closure signature.");
        self.node_with(SyntaxKind::FunctionBody, Self::expression);

        self.end_node();
    }
}
