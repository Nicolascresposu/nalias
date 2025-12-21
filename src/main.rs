use clap::Parser;
use nalias::{Cli, dispatch};

fn main() {
    let cli = Cli::parse();
    let code = match dispatch(cli) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            if let Some(hint) = error.hint() {
                eprintln!("hint: {hint}");
            }
            error.exit_code()
        }
    };
    std::process::exit(code);
}
