use std::path::PathBuf;

fn main() {
    let parse = gelixrs::parse_source(vec![PathBuf::from("parsetest.gel")]).unwrap();
    for m in parse {
        m.cst.debug_print();
    }
}
