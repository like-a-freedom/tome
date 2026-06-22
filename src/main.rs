use clap::Parser;
use tome::{Cli, run};

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli) {
        eprintln!("Error: {:#}", err);
        std::process::exit(1);
    }
}
