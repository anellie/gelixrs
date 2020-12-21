use std::fs::read_to_string;
use std::rc::Rc;

fn main() {
    let src = read_to_string("tests/string/escape_invalid.gel").unwrap();
    let tokens = lexer::lex(&Rc::new(src));
    for tok in tokens {
        println!("{:?}: {:?}", tok.span, tok.token)
    }
}
