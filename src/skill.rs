//! Embedded agent manual, printed by `tome skill [topic]`.
//!
//! Progressive disclosure: the core SKILL.md stays small; deeper references
//! load only when the agent asks for a topic.

use anyhow::{Result, bail};

const CORE: &str = include_str!("skill/SKILL.md");
const COMMANDS: &str = include_str!("skill/commands.md");
const TUI: &str = include_str!("skill/tui.md");
const WORKFLOWS: &str = include_str!("skill/workflows.md");

pub const TOPICS: [&str; 3] = ["commands", "tui", "workflows"];

/// Resolve a manual topic; `None` returns the core SKILL.md.
pub fn document(topic: Option<&str>) -> Result<&'static str> {
    match topic {
        None => Ok(CORE),
        Some("commands") => Ok(COMMANDS),
        Some("tui") => Ok(TUI),
        Some("workflows") => Ok(WORKFLOWS),
        Some(other) => bail!(
            "Unknown skill topic '{other}'. Available: {}",
            TOPICS.join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_manual_is_a_skill_with_triggers_and_pointers() {
        let doc = document(None).unwrap();
        assert!(doc.starts_with("---\nname: tome\n"));
        assert!(doc.contains("tome skill commands"));
        assert!(doc.contains("Space"));
    }

    #[test]
    fn topics_resolve_to_embedded_references() {
        assert!(document(Some("commands")).unwrap().contains("--no-sparse"));
        assert!(document(Some("tui")).unwrap().contains("Manage"));
        assert!(document(Some("workflows")).unwrap().contains("partial"));
    }

    #[test]
    fn unknown_topic_lists_available_topics() {
        let err = document(Some("bogus")).unwrap_err().to_string();
        for topic in ["commands", "tui", "workflows"] {
            assert!(err.contains(topic), "{err}");
        }
    }
}
