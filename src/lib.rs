pub mod commands;
pub mod config;
pub mod core;
pub mod fs;
pub mod progress;
pub mod tui;

use anyhow::Result;

pub use config::Cli;

pub fn run(cli: Cli) -> Result<()> {
    let skills_dir = cli.skills_dir();
    let manifest_path = cli.manifest_path();

    match cli.command {
        config::Commands::Add { source, path, force } => {
            commands::add::execute(&source, path, force, &skills_dir, &manifest_path)
        }
        config::Commands::Remove { name, force } => {
            commands::remove::execute(&name, force, &skills_dir, &manifest_path)
        }
        config::Commands::Update { name, verbose } => {
            commands::update::execute(name.as_deref(), verbose, &skills_dir, &manifest_path)
        }
        config::Commands::List { json } => {
            commands::list::execute(json, &skills_dir, &manifest_path)
        }
    }
}
