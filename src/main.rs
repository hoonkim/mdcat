mod cli;
mod image_kitty;
mod ir;
mod layout;
mod parser;
mod style;
mod term;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cfg = match cli::parse_args(args) {
        Ok(c) => c,
        Err(cli::CliError::MissingArg) => {
            eprintln!("usage: mdcat <file.md>");
            std::process::exit(2);
        }
        Err(cli::CliError::Io(e)) => {
            eprintln!("mdcat: {e}");
            std::process::exit(1);
        }
    };
    let src = match cli::read_source(&cfg) {
        Ok(s) => s,
        Err(cli::CliError::Io(e)) => { eprintln!("mdcat: {e}"); std::process::exit(1); }
        Err(_) => unreachable!(),
    };
    print!("{}", src.len()); // 임시: 이후 Task에서 교체
}
