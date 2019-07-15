use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.len() {
        2 => gelixrs::run_file(&args[1]),
        _ => {
            println!("Usage: gelixrs [file path]");
            process::exit(64);
        }
    }
}