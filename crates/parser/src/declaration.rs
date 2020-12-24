use crate::Parser;
use error::GErr;
use syntax::kind::SyntaxKind;

// All tokens that indicate that a function has a body (bodies are optional in enum and interface definitions).
static START_OF_FN_BODY: [SyntaxKind; 2] = [SyntaxKind::LeftBrace, SyntaxKind::Equal];

// All tokens that can be modifiers at all.
pub static MODIFIERS: [SyntaxKind; 5] = [
    SyntaxKind::Public,
    SyntaxKind::Private,
    SyntaxKind::Extern,
    SyntaxKind::Variadic,
    SyntaxKind::Strong,
];

// All tokens that can be modifiers on any declaration.
static GLOBAL_MODIFIERS: [SyntaxKind; 2] = [SyntaxKind::Public, SyntaxKind::Private];

// All tokens that can be modifiers on a class member.
static MEMBER_MODIFIERS: [SyntaxKind; 0] = [];
// All tokens that can be modifiers on a method.
static METHOD_MODIFIERS: [SyntaxKind; 1] = [SyntaxKind::Strong];
// All tokens that can be modifiers on a constructor.
static CONSTRUCTOR_MODIFIERS: [SyntaxKind; 0] = [];

// All tokens that can be modifiers on a function.
static FUNC_MODIFIERS: [SyntaxKind; 2] = [SyntaxKind::Extern, SyntaxKind::Variadic];
// All tokens that can be modifiers on an import declaration.
static IMPORT_MODIFIERS: [SyntaxKind; 0] = [];

impl<'p> Parser<'p> {
    pub(crate) fn declaration(&mut self) {
        let checkpoint = self.checkpoint();
        self.consume_modifiers();

        let ty = match self.peek() {
            SyntaxKind::Func => SyntaxKind::FunctionDecl,
            SyntaxKind::Import | SyntaxKind::Export => SyntaxKind::ImportDecl,
            SyntaxKind::Impl => SyntaxKind::ImplDecl,
            _ => SyntaxKind::AdtDecl,
        };
        self.start_node_at(checkpoint, ty);

        match self.advance_checked() {
            SyntaxKind::Func => self.function(&FUNC_MODIFIERS),
            SyntaxKind::Class => self.generic_adt(CLASS_CONF),
            SyntaxKind::Export => self.import_declaration(),
            SyntaxKind::Import => self.import_declaration(),
            SyntaxKind::Interface => self.generic_adt(IFACE_CONF),
            SyntaxKind::Impl => self.iface_impl(),
            SyntaxKind::Enum => self.generic_adt(ENUM_CONF),
            _ => self.error_at_current(GErr::E002),
        }
        self.end_node();
    }

    fn function(&mut self, mods: &'static [SyntaxKind]) {
        self.function_(mods, false)
    }

    fn function_(&mut self, mods: &'static [SyntaxKind], force_ext: bool) {
        let is_extern = force_ext
            || self
                .modifiers
                .iter()
                .any(|kind| *kind == SyntaxKind::Extern);
        self.func_signature(mods);

        if !is_extern {
            self.start_node(SyntaxKind::FunctionBody);
            if !self.check(SyntaxKind::LeftBrace) {
                self.consume(
                    SyntaxKind::Equal,
                    "start of block or '='",
                    "function signature",
                );
            }
            self.expression();
            self.end_node();
        };
    }

    fn func_signature(&mut self, mods: &'static [SyntaxKind]) {
        self.start_node(SyntaxKind::FunctionSignature);
        self.check_mods(&mods, "function");
        self.generic_ident("'func'");
        self.consume(SyntaxKind::LeftParen, "'('", "function name");
        self.func_parameters();
        if self.matches(SyntaxKind::Arrow) {
            self.type_()
        }
        self.end_node();
    }

    pub(crate) fn func_parameters(&mut self) {
        if !self.check(SyntaxKind::RightParen) {
            loop {
                self.start_node(SyntaxKind::Parameter);
                self.consume(SyntaxKind::Identifier, "parameter name", "left parenthesis");
                self.consume(SyntaxKind::Colon, "':'", "parameter name");
                self.type_();
                self.end_node();
                if !self.matches(SyntaxKind::Comma) {
                    break;
                }
            }
        }
        self.consume(SyntaxKind::RightParen, "')'", "parameters");
    }

    fn generic_adt(&mut self, conf: ADTConfig) {
        self.check_mods(conf.modifiers, conf.name);
        self.generic_ident("ADT identifier");

        self.consume(SyntaxKind::LeftBrace, "'{'", "before body");

        while !self.check(SyntaxKind::RightBrace) && !self.is_at_end() {
            self.consume_modifiers();
            match self.peek() {
                SyntaxKind::Var | SyntaxKind::Val if conf.has_members => self.adt_member(),
                SyntaxKind::Construct if conf.has_constructors => self.constructor(),
                SyntaxKind::Func => self.method(conf.force_extern),
                SyntaxKind::Identifier if conf.has_cases => self.enum_case(),
                _ => self.error_at_current(GErr::E004),
            }
        }

        self.consume(SyntaxKind::RightBrace, "'}'", "body");
    }

    fn method(&mut self, force_extern: bool) {
        self.start_node(SyntaxKind::Method);
        self.advance(); // Consume 'func'
        self.function_(&METHOD_MODIFIERS, force_extern);
        self.end_node();
    }

    fn adt_member(&mut self) {
        self.check_mods(&MEMBER_MODIFIERS, "class member");

        self.start_node(SyntaxKind::AdtMember);
        self.advance(); // Consume 'var' or 'val'
        self.consume(SyntaxKind::Identifier, "variable name", "var/val");

        match self.advance_checked() {
            SyntaxKind::Equal => self.node_with(SyntaxKind::Initializer, Self::expression),

            SyntaxKind::Colon => {
                self.type_();
                if self.matches(SyntaxKind::Equal) {
                    self.node_with(SyntaxKind::Initializer, Self::expression);
                }
            }

            _ => self.error_at_current(GErr::E005),
        }

        self.end_node();
    }

    fn constructor(&mut self) {
        self.check_mods(&CONSTRUCTOR_MODIFIERS, "constructor");

        self.start_node(SyntaxKind::Constructor);
        self.start_node(SyntaxKind::FunctionSignature);
        self.advance(); // Consume 'construct'
        self.consume(SyntaxKind::LeftParen, "'('", "'construct'");

        if !self.check(SyntaxKind::RightParen) {
            loop {
                self.start_node(SyntaxKind::Parameter);
                self.consume(SyntaxKind::Identifier, "parameter name", "'construct'");
                if self.matches(SyntaxKind::Colon) {
                    self.type_()
                }
                self.end_node();
                if !self.matches(SyntaxKind::Comma) {
                    break;
                }
            }
        }
        self.consume(SyntaxKind::RightParen, "')'", "parameters");
        self.end_node();

        self.maybe_fn_body();
        self.end_node();
    }

    fn maybe_fn_body(&mut self) {
        if START_OF_FN_BODY.contains(&self.peek()) {
            if !self.check(SyntaxKind::LeftBrace) {
                self.consume(
                    SyntaxKind::Equal,
                    "start of block or '='",
                    "function signature",
                );
            }
            self.node_with(SyntaxKind::FunctionBody, Self::expression);
        }
    }

    fn enum_case(&mut self) {
        self.start_node(SyntaxKind::EnumCase);

        if self.peek_next() == SyntaxKind::LeftBrace {
            self.generic_adt(CASE_CONF);
        } else {
            self.generic_ident("<internal error>");

            if self.matches(SyntaxKind::LeftParen) {
                while !self.check(SyntaxKind::RightParen) && !self.is_at_end() {
                    self.consume_modifiers();

                    self.start_node(SyntaxKind::Variable);
                    self.consume_either(
                        SyntaxKind::Val,
                        SyntaxKind::Var,
                        "'var' or 'val'",
                        "left parenthesis",
                    );
                    self.consume(SyntaxKind::Identifier, "member name", "var/val");
                    self.consume(SyntaxKind::Colon, "':'", "member name");
                    self.type_();
                    self.end_node();

                    if !self.matches(SyntaxKind::Comma) {
                        break;
                    }
                }
                self.consume(SyntaxKind::RightParen, "')'", "members");
            }
        }

        self.end_node();
    }

    fn import_declaration(&mut self) {
        self.check_mods(&IMPORT_MODIFIERS, "import/export");
        self.consume(SyntaxKind::Identifier, "path", "import/export");
        while self.matches(SyntaxKind::Slash) {
            self.consume_either(
                SyntaxKind::Identifier,
                SyntaxKind::Plus,
                "'+' or path",
                "'/'",
            );
        }
    }

    fn iface_impl(&mut self) {
        self.node_with(SyntaxKind::Implementing, |this| this.type_());
        self.consume(SyntaxKind::For, "'for'", "interface name");
        self.node_with(SyntaxKind::Implementor, |this| this.type_());
        self.consume(SyntaxKind::LeftBrace, "'{'", "impl body");

        while !self.check(SyntaxKind::RightBrace) && !self.is_at_end() {
            match self.peek() {
                SyntaxKind::Func => self.method(false),
                _ => self.error_at_current(GErr::E004),
            }
        }
        self.consume(SyntaxKind::RightBrace, "'}'", "impl body");
    }

    // Reads an identifier followed by optional generic type parameters.
    fn generic_ident(&mut self, after: &'static str) {
        self.start_node(SyntaxKind::Ident);
        self.consume(SyntaxKind::Identifier, "a name", after);
        if self.matches(SyntaxKind::LeftBracket) {
            while self.check(SyntaxKind::Identifier) {
                self.start_node(SyntaxKind::TypeParameter);
                self.advance();
                if self.matches(SyntaxKind::Colon) {
                    self.type_();
                }
                self.end_node();
                if !self.matches(SyntaxKind::Comma) {
                    break;
                }
            }
            self.consume(SyntaxKind::RightBracket, "']'", "type parameters");
        }
        self.end_node();
    }

    fn consume_modifiers(&mut self) {
        self.modifiers.clear();
        while MODIFIERS.contains(&self.peek()) {
            let modifier = self.peek();
            self.modifiers.push(modifier);
            self.node_with(SyntaxKind::Modifier, |this| {
                this.advance();
            });
        }
    }

    fn check_mods(&mut self, allowed: &'static [SyntaxKind], name: &'static str) {
        for mod_ in self
            .modifiers
            .clone()
            .iter()
            .filter(|m| !allowed.contains(&m) && !GLOBAL_MODIFIERS.contains(&m))
        {
            self.error_at_current(GErr::E006 {
                modifier: format!("{:?}", mod_),
                on: name,
            })
        }
    }

    /// Reads a type name.
    pub(crate) fn type_(&mut self) {
        self.start_node(SyntaxKind::Type);
        let token = self.advance();
        match token.kind {
            SyntaxKind::Identifier => {
                if self.matches(SyntaxKind::LeftBracket) {
                    loop {
                        self.type_();
                        if !self.matches(SyntaxKind::Comma) {
                            break;
                        }
                    }
                    self.consume(SyntaxKind::RightBracket, "']'", "type parameters");
                } else if self.matches(SyntaxKind::Colon) {
                    self.consume(SyntaxKind::Identifier, "case name", "':'");
                }
            }

            // Read inner
            SyntaxKind::Tilde | SyntaxKind::AndSym | SyntaxKind::Star => self.type_(),

            SyntaxKind::LeftParen => {
                if !self.check(SyntaxKind::RightParen) {
                    loop {
                        self.type_();
                        if !self.matches(SyntaxKind::Comma) {
                            break;
                        }
                    }
                }

                self.consume(SyntaxKind::RightParen, "')'", "closure parameters");
                if self.matches(SyntaxKind::Colon) {
                    self.type_()
                }
            }

            _ => self.error_at_current(GErr::E003),
        }
        self.end_node();
    }
}

struct ADTConfig {
    name: &'static str,
    modifiers: &'static [SyntaxKind],
    has_members: bool,
    has_constructors: bool,
    has_cases: bool,
    force_extern: bool,
}

const CLASS_CONF: ADTConfig = ADTConfig {
    name: "class",
    modifiers: &[SyntaxKind::Extern],
    has_members: true,
    has_constructors: true,
    has_cases: false,
    force_extern: false,
};

const IFACE_CONF: ADTConfig = ADTConfig {
    name: "interface",
    modifiers: &[],
    has_members: false,
    has_constructors: false,
    has_cases: false,
    force_extern: true,
};

const ENUM_CONF: ADTConfig = ADTConfig {
    name: "enum",
    modifiers: &[],
    has_members: true,
    has_constructors: false,
    has_cases: true,
    force_extern: false,
};

const CASE_CONF: ADTConfig = ADTConfig {
    name: "enum case",
    modifiers: &[],
    has_members: true,
    has_constructors: true,
    has_cases: false,
    force_extern: false,
};
