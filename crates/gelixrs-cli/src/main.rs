use std::path::PathBuf;
use std::env;

fn main() {
    let parse = gelixrs::parse_source(vec![PathBuf::from(env::args().skip(1).next().unwrap())]);
    match parse {
        Ok(m) => {
            for m in m {
                m.cst.debug_print();
            }
        }

        Err(e) => {
            for e in e {
                println!("{}", e)
            }
        }
    }
}
