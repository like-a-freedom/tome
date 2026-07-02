use crate::style::{self, CYAN, DIM, GREEN, YELLOW};
use crossterm::{
    ExecutableCommand, cursor,
    terminal::{Clear, ClearType},
};
use std::io::{self, IsTerminal, Write};

const SPINNER_FRAMES: &[&str] = &["-", "\\", "|", "/"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressPhase {
    Downloading,
    Cloning,
    Inspecting,
    Checking,
    Copying,
    Saving,
}

impl ProgressPhase {
    fn label(self) -> &'static str {
        match self {
            ProgressPhase::Downloading => "downloading",
            ProgressPhase::Cloning => "cloning",
            ProgressPhase::Inspecting => "inspecting",
            ProgressPhase::Checking => "checking",
            ProgressPhase::Copying => "copying",
            ProgressPhase::Saving => "saving",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RepoOutcome {
    pub updated: usize,
    pub current: usize,
    pub failed: usize,
    pub removed: usize,
}

impl RepoOutcome {
    pub fn new(updated: usize, current: usize, failed: usize, removed: usize) -> Self {
        Self {
            updated,
            current,
            failed,
            removed,
        }
    }

    fn is_clean(self) -> bool {
        self.failed == 0 && self.removed == 0
    }
}

#[derive(Debug)]
struct ProgressState {
    source_idx: usize,
    source_count: usize,
    label: String,
    phase: ProgressPhase,
    checked_skills: usize,
    total_skills: usize,
    frame_idx: usize,
}

pub struct UpdateProgress {
    live: bool,
    state: Option<ProgressState>,
}

impl UpdateProgress {
    pub fn new(verbose: bool) -> Self {
        Self {
            live: io::stderr().is_terminal() && !verbose,
            state: None,
        }
    }

    pub fn enabled(&self) -> bool {
        self.live
    }

    pub fn start(&mut self, total_skills: usize, total_sources: usize) {
        if self.live {
            eprintln!("{}", format_header(total_skills, total_sources));
        }
    }

    pub fn repo_started(
        &mut self,
        source_idx: usize,
        source_count: usize,
        label: &str,
        skill_count: usize,
    ) {
        self.state = Some(ProgressState {
            source_idx,
            source_count,
            label: label.to_string(),
            phase: ProgressPhase::Cloning,
            checked_skills: 0,
            total_skills: skill_count,
            frame_idx: 0,
        });
        self.draw();
    }

    pub fn phase(&mut self, phase: ProgressPhase) {
        if let Some(state) = &mut self.state {
            state.phase = phase;
            state.frame_idx = state.frame_idx.wrapping_add(1);
        }
        self.draw();
    }

    pub fn skill_checked(&mut self) {
        if let Some(state) = &mut self.state {
            state.checked_skills = state.checked_skills.saturating_add(1);
            state.frame_idx = state.frame_idx.wrapping_add(1);
        }
        self.draw();
    }

    pub fn repo_finished(&mut self, label: &str, outcome: RepoOutcome) {
        if self.live {
            clear_live_line();
            eprintln!("{}", format_repo_line(label, outcome));
        }
        self.state = None;
    }

    pub fn finish(&mut self) {
        if self.live {
            clear_live_line();
        }
        self.state = None;
    }

    fn draw(&mut self) {
        if !self.live {
            return;
        }

        if let Some(state) = &self.state {
            let mut stderr = io::stderr();
            let _ = stderr.execute(cursor::MoveToColumn(0));
            let _ = stderr.execute(Clear(ClearType::CurrentLine));
            let _ = write!(stderr, "{}", format_live_line(state));
            let _ = stderr.flush();
        }
    }
}

fn clear_live_line() {
    let mut stderr = io::stderr();
    let _ = stderr.execute(cursor::MoveToColumn(0));
    let _ = stderr.execute(Clear(ClearType::CurrentLine));
}

fn pluralize(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("{} {}", count, singular)
    } else {
        format!("{} {}", count, plural)
    }
}

fn format_header(total_skills: usize, total_sources: usize) -> String {
    format!(
        "{} {} from {}",
        style::styled(CYAN, "Updating"),
        pluralize(total_skills, "skill", "skills"),
        pluralize(total_sources, "source", "sources"),
    )
}

fn format_live_line(state: &ProgressState) -> String {
    let frame = SPINNER_FRAMES[state.frame_idx % SPINNER_FRAMES.len()];
    format!(
        "{} source {}/{} - {} - {} - {}/{} skills checked",
        style::styled(CYAN, frame),
        state.source_idx,
        state.source_count,
        state.label,
        state.phase.label(),
        state.checked_skills,
        state.total_skills,
    )
}

fn format_repo_line(label: &str, outcome: RepoOutcome) -> String {
    let marker = if outcome.is_clean() {
        style::styled(GREEN, "ok")
    } else {
        style::styled(YELLOW, "!!")
    };
    let mut parts = Vec::new();
    if outcome.updated > 0 {
        parts.push(style::styled(
            GREEN,
            &format!("{} updated", outcome.updated),
        ));
    }
    if outcome.current > 0 {
        parts.push(style::styled(DIM, &format!("{} current", outcome.current)));
    }
    if outcome.removed > 0 {
        parts.push(style::styled(
            YELLOW,
            &format!("{} removed", outcome.removed),
        ));
    }
    if outcome.failed > 0 {
        parts.push(style::styled(YELLOW, &format!("{} failed", outcome.failed)));
    }
    if parts.is_empty() {
        parts.push(style::styled(DIM, "no changes"));
    }

    format!("{} {:<24} {}", marker, label, parts.join(" - "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_ansi(input: &str) -> String {
        input
            .replace(style::RESET, "")
            .replace(style::BOLD, "")
            .replace(style::DIM, "")
            .replace(style::GREEN, "")
            .replace(style::YELLOW, "")
            .replace(style::CYAN, "")
    }

    #[test]
    fn test_format_header_plural() {
        let rendered = strip_ansi(&format_header(24, 5));
        assert_eq!(rendered, "Updating 24 skills from 5 sources");
    }

    #[test]
    fn test_format_header_singular() {
        let rendered = strip_ansi(&format_header(1, 1));
        assert_eq!(rendered, "Updating 1 skill from 1 source");
    }

    #[test]
    fn test_format_live_line_has_real_counts() {
        let state = ProgressState {
            source_idx: 2,
            source_count: 5,
            label: "owner/repo".to_string(),
            phase: ProgressPhase::Checking,
            checked_skills: 3,
            total_skills: 7,
            frame_idx: 0,
        };

        let rendered = strip_ansi(&format_live_line(&state));

        assert_eq!(
            rendered,
            "- source 2/5 - owner/repo - checking - 3/7 skills checked"
        );
    }

    #[test]
    fn test_format_repo_line_clean_result() {
        let rendered = strip_ansi(&format_repo_line(
            "owner/repo",
            RepoOutcome::new(2, 3, 0, 0),
        ));

        assert!(rendered.contains("ok owner/repo"));
        assert!(rendered.contains("2 updated"));
        assert!(rendered.contains("3 current"));
        assert!(!rendered.contains("failed"));
    }

    #[test]
    fn test_format_repo_line_failure_result() {
        let rendered = strip_ansi(&format_repo_line(
            "owner/repo",
            RepoOutcome::new(0, 1, 2, 1),
        ));

        assert!(rendered.contains("!! owner/repo"));
        assert!(rendered.contains("1 current"));
        assert!(rendered.contains("1 removed"));
        assert!(rendered.contains("2 failed"));
    }

    #[test]
    fn test_disabled_when_verbose() {
        let progress = UpdateProgress {
            live: false,
            state: None,
        };

        assert!(!progress.enabled());
    }
}
