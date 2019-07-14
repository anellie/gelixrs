# All grammar rules:

file           → declaration* EOF ;

declaration    → classDecl
               | funDecl
               | varDecl
               | statement ;

classDecl      → "class" IDENTIFIER ( "ext" IDENTIFIER )?
                 "{" (varDecl | funDecl) "}" ;
funDecl        → "func" function ;
varDecl        → ("var" | "val") IDENTIFIER ( "=" expression )? ;
enumDecl       → "enum" IDENTIFIER "{" IDENTIFIER* "}" ;

statement      → exprStmt
               | forStmt
               | ifStmt
               | returnStmt 
               | errorStmt;

exprStmt       → expression ;
forStmt        → "for" "(" (IDENTIFIER "in" expression | expression) ")" statement ;
returnStmt     → "return" expression? ";" ;
errorStmt      → "error" expression? ";" ;

takeExpr       → "take" expression ("else" expression)? ";" ;
whenExpr       → "when" "(" expression ")" 
                    "{" (expression "->" statement)* "}";
block          → "{" declaration* "}" ;
ifExpr         → "if" "(" expression ")" statement ( "else" statement )? ;

// TODO: return and error should propably also be an expression. This would allow use in 'if' without a block.
expression     → block | assignment | whenExpr | ifExpr | takeExpr ;

assignment     → ( call "." )? IDENTIFIER ( slice )? "=" assignment
               | logic_or;

logic_or       → logic_and ( "or" logic_and )* ;
logic_and      → equality ( "and" equality )* ;
equality       → comparison ( ( "!=" | "==" ) comparison )* ;
comparison     → addition ( ( ">" | ">=" | "<" | "<=" ) addition )* ;
addition       → multiplication ( ( "-" | "+" ) multiplication )* ;
multiplication → unary ( ( "/" | "*" ) unary )* ;

unary          → ( "!" | "-" ) unary | call ("++" | "--");
call           → primary ( "(" arguments? ")" | "." IDENTIFIER )* ;
slice          → primary ( "[" expression "]" | "." IDENTIFIER )* ;
primary        → "true" | "false" | "null" | "this" | "error" 
               | ARRAY | NUMBER | STRING | IDENTIFIER | "(" expression ")"
               | "super" "." IDENTIFIER ;

function       → IDENTIFIER "(" parameters? ")" ("->" TYPE)? (block | "=" statement) ;
parameters     → TYPE IDENTIFIER ( "," TYPE IDENTIFIER )* ;
arguments      → expression ( "," expression )* ;

ARRAY          → "[" (primary*)? "]" 
NUMBER         → DIGIT+ ( "." DIGIT+ )? ;
STRING         → '"' <any char except '"'>* '"' ;
TYPE           → IDENTIFIER ;
IDENTIFIER     → ALPHA ( ALPHA | DIGIT )* ;
ALPHA          → 'a' ... 'z' | 'A' ... 'Z' | '_' ;
DIGIT          → '0' ... '9' ;

## Thanks to Robert Nystrom and his book 'Crafting Interpreters'. Gelix grammar is heavily inspired by Lox.