use std::{env, path::PathBuf};

fn main() {
    let parse = gelixrs::parse_source(vec![PathBuf::from(env::args().nth(1).unwrap())]);
    match parse {
        Ok(m) => {
            for m in m {
                println!("{:#?}", m.cst);
            }
        }

        Err(e) => {
            for e in e {
                println!("{}", e)
            }
        }
    }
}
