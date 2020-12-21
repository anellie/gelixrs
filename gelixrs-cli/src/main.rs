use std::fs::read_to_string;

fn main() {
    let src = read_to_string("parsetest.gel").unwrap();
    let result = parser::parse(&src);
    result.debug_print();
}
