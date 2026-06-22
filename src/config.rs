use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "tome",
    version,
    about = "Minimal CLI for managing AI agent skills"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Override skills directory
    #[arg(long, env = "SKILLS_DIR")]
    pub dir: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Install a skill from GitHub
    Add {
        /// GitHub owner/repo or owner/repo@skill-name
        source: String,

        /// Skill path within repo (if not root)
        #[arg(long)]
        path: Option<String>,

        /// Overwrite if already installed
        #[arg(long)]
        force: bool,
    },

    /// Remove an installed skill
    Remove {
        /// Skill name to remove
        name: String,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Update installed skills
    Update {
        /// Specific skill to update (optional)
        name: Option<String>,

        /// Show full update details
        #[arg(long)]
        verbose: bool,
    },

    /// List installed skills
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

fn default_skills_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".agents").join("skills")
    } else {
        // Fallback: current working directory (unlikely on macOS/Linux)
        PathBuf::from(".agents/skills")
    }
}

impl Cli {
    pub fn skills_dir(&self) -> PathBuf {
        self.dir.clone().unwrap_or_else(default_skills_dir)
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.skills_dir().join(".skills.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_skills_dir_default_is_home_based() {
        let cli = Cli::parse_from(["tome", "list"]);
        let dir = cli.skills_dir();
        assert!(
            dir.is_absolute(),
            "Default skills dir should be absolute (home-based), got: {:?}",
            dir
        );
        assert!(
            dir.ends_with(".agents/skills"),
            "Default skills dir should end with `.agents/skills`, got: {:?}",
            dir
        );
    }

    #[test]
    fn test_skills_dir_custom() {
        let cli = Cli::parse_from(["tome", "--dir", "/custom/path", "list"]);
        assert_eq!(cli.skills_dir(), PathBuf::from("/custom/path"));
    }

    #[test]
    fn test_manifest_path_default() {
        let cli = Cli::parse_from(["tome", "list"]);
        let path = cli.manifest_path();
        assert!(
            path.is_absolute(),
            "Default manifest path should be absolute, got: {:?}",
            path
        );
        assert!(
            path.ends_with(".skills.json"),
            "Default manifest path should end with .skills.json, got: {:?}",
            path
        );
    }

    #[test]
    fn test_manifest_path_custom() {
        let cli = Cli::parse_from(["tome", "--dir", "/custom", "list"]);
        assert_eq!(cli.manifest_path(), PathBuf::from("/custom/.skills.json"));
    }

    #[test]
    fn test_add_command_parsing() {
        let cli = Cli::parse_from(["tome", "add", "owner/repo"]);
        assert!(matches!(cli.command, Commands::Add { .. }));
    }

    #[test]
    fn test_remove_command_parsing() {
        let cli = Cli::parse_from(["tome", "remove", "skill-name"]);
        assert!(matches!(cli.command, Commands::Remove { ref name, .. } if name == "skill-name"));
    }

    #[test]
    fn test_update_command_parsing() {
        let cli = Cli::parse_from(["tome", "update"]);
        assert!(matches!(
            cli.command,
            Commands::Update {
                name: None,
                verbose: false
            }
        ));
    }

    #[test]
    fn test_update_verbose_parsing() {
        let cli = Cli::parse_from(["tome", "update", "--verbose"]);
        assert!(matches!(
            cli.command,
            Commands::Update {
                name: None,
                verbose: true
            }
        ));
    }

    #[test]
    fn test_update_specific_parsing() {
        let cli = Cli::parse_from(["tome", "update", "my-skill"]);
        assert!(
            matches!(cli.command, Commands::Update { name: Some(ref n), verbose: false } if n == "my-skill")
        );
    }

    #[test]
    fn test_list_json_parsing() {
        let cli = Cli::parse_from(["tome", "list", "--json"]);
        match cli.command {
            Commands::List { json } => assert!(json),
            _ => panic!("Expected List"),
        }
    }

    #[test]
    fn test_add_force_parsing() {
        let cli = Cli::parse_from(["tome", "add", "owner/repo", "--force"]);
        match cli.command {
            Commands::Add { force, .. } => assert!(force),
            _ => panic!("Expected Add"),
        }
    }

    #[test]
    fn test_add_path_parsing() {
        let cli = Cli::parse_from(["tome", "add", "owner/repo", "--path", "skills/foo"]);
        match cli.command {
            Commands::Add { path, .. } => assert_eq!(path.as_deref(), Some("skills/foo")),
            _ => panic!("Expected Add"),
        }
    }
}
