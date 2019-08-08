use std::process;

// TODO proper args parsing...
fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.len() {
        2 => gelixrs::do_file(&args[1], false),
        3  => gelixrs::do_file(&args[1], true),
        _ => {
            println!("Usage: gelixrs [file path] [--print-parser]");
            process::exit(64);
        }
    }
}