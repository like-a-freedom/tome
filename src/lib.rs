pub mod commands;
pub mod config;
pub mod core;

pub mod fs;
pub mod output;
pub mod progress;
pub mod services;
pub mod style;
pub mod tui;

pub mod testutil;

/// Default git branch name used when branch detection fails.
pub const DEFAULT_BRANCH: &str = "main";

use anyhow::Result;

pub use config::Cli;

pub fn run(cli: Cli) -> Result<()> {
    let skills_dir = cli.skills_dir();
    let manifest_path = cli.manifest_path();

    match cli.command {
        config::Commands::Add {
            source,
            path,
            force,
            no_sparse,
        } => commands::add::execute(
            &source,
            path,
            force,
            !no_sparse,
            &skills_dir,
            &manifest_path,
        ),
        config::Commands::Remove { name, force } => {
            commands::remove::execute_options(name.as_deref(), force, &skills_dir, &manifest_path)
        }
        config::Commands::Update {
            name,
            interactive,
            verbose,
            no_sparse,
        } => commands::update::execute_options(
            name.as_deref(),
            interactive,
            verbose,
            !no_sparse,
            &skills_dir,
            &manifest_path,
        ),
        config::Commands::List { json, plain } => {
            commands::list::execute_options(json, plain, &skills_dir, &manifest_path)
        }
        config::Commands::Export { interactive } => {
            commands::export::execute_options(interactive, &skills_dir, &manifest_path)
        }
    }
}
