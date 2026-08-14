use crate::services::update_service::UpdateReporter;
use crate::style::{self, CYAN, DIM, GREEN, YELLOW};
use crossterm::{
    cursor, execute,
    style::Print,
    terminal::{Clear, ClearType},
};
use std::io::{self, IsTerminal};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Maximum display width for repo labels in the live progress line.
/// Prevents line wrapping on 80-col terminals when labels are long
/// (e.g. "deanpeters/Product-Manager-Skills" at 33 chars).
const MAX_LABEL_WIDTH: usize = 25;

/// Overall bar scale: clone maps to 0..CLONE_SEGMENT_END, sparse checkout to
/// CLONE_SEGMENT_END..CHECKOUT_SEGMENT_END, skills checked/total to the rest.
const CLONE_SEGMENT_END: u8 = 35;
const CHECKOUT_SEGMENT_END: u8 = 60;
const OVERALL_SCALE_END: u8 = 100;
const PERCENT_SCALE_END: u8 = 100;

fn overall_for_clone(percent: u8) -> u8 {
    map_percent_into(percent, 0, CLONE_SEGMENT_END)
}

fn overall_for_checkout(percent: u8) -> u8 {
    let span = CHECKOUT_SEGMENT_END - CLONE_SEGMENT_END;
    map_percent_into(percent, CLONE_SEGMENT_END, span)
}

/// Map a 0-100 percent into the `start..=start+span` segment of the overall
/// scale, clamping the input so it can never overshoot the segment.
fn map_percent_into(percent: u8, start: u8, span: u8) -> u8 {
    start
        + (u16::from(percent.min(PERCENT_SCALE_END)) * u16::from(span)
            / u16::from(PERCENT_SCALE_END)) as u8
}

fn overall_for_skills(checked: usize, total: usize) -> u8 {
    if total == 0 {
        return CHECKOUT_SEGMENT_END;
    }
    let span = (OVERALL_SCALE_END - CHECKOUT_SEGMENT_END) as usize;
    CHECKOUT_SEGMENT_END + ((span * checked.min(total)) / total) as u8
}

pub use crate::services::update_service::{ProgressPhase, RepoOutcome};

impl ProgressPhase {
    fn label(self) -> &'static str {
        match self {
            ProgressPhase::Downloading => "downloading",
            ProgressPhase::Cloning => "cloning",
            ProgressPhase::Checkout => "checkout",
            ProgressPhase::Inspecting => "inspecting",
            ProgressPhase::Checking => "checking",
            ProgressPhase::Copying => "copying",
            ProgressPhase::Saving => "saving",
        }
    }
}

impl RepoOutcome {
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
    started: std::time::Instant,
    overall: u8,
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
            // Use the execute! macro to chain MoveToColumn(0), Clear(CurrentLine),
            // and Print in a single batch. The macro queues all commands and
            // flushes once, so the terminal sees one atomic update — preventing
            // interleaving when the draw is called rapidly.
            let _ = execute!(
                stderr,
                cursor::MoveToColumn(0),
                Clear(ClearType::CurrentLine),
                Print(format_live_line(state))
            );
        }
    }
}

impl UpdateReporter for UpdateProgress {
    fn start(&mut self, total_skills: usize, total_sources: usize) {
        let header = format_header(total_skills, total_sources);
        if self.live {
            eprintln!("{}", header);
        } else {
            println!("{}", header);
        }
    }

    fn repo_started(
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
            started: std::time::Instant::now(),
            overall: 0,
        });
        self.draw();
    }

    fn phase(&mut self, phase: ProgressPhase) {
        if let Some(state) = &mut self.state {
            state.phase = phase;
            state.frame_idx = state.frame_idx.wrapping_add(1);
            self.draw();
        } else if matches!(phase, ProgressPhase::Saving) {
            println!("Saving manifest...");
        }
    }

    fn skill_checked(&mut self) {
        if let Some(state) = &mut self.state {
            state.checked_skills = state.checked_skills.saturating_add(1);
            state.overall = state
                .overall
                .max(overall_for_skills(state.checked_skills, state.total_skills));
            state.frame_idx = state.frame_idx.wrapping_add(1);
        }
        self.draw();
    }

    fn clone_progress(&mut self, percent: u8) {
        if let Some(state) = &mut self.state {
            state.overall = state.overall.max(overall_for_clone(percent));
            state.frame_idx = state.frame_idx.wrapping_add(1);
        }
        self.draw();
    }

    fn checkout_progress(&mut self, percent: u8) {
        if let Some(state) = &mut self.state {
            state.overall = state.overall.max(overall_for_checkout(percent));
            state.frame_idx = state.frame_idx.wrapping_add(1);
        }
        self.draw();
    }

    fn sync_found(&mut self, count: usize) {
        if let Some(state) = &mut self.state {
            state.total_skills = state.total_skills.saturating_add(count);
            state.overall = state
                .overall
                .max(overall_for_skills(state.checked_skills, state.total_skills));
            state.frame_idx = state.frame_idx.wrapping_add(1);
        }
        self.draw();
    }

    fn repo_finished(&mut self, label: &str, outcome: RepoOutcome) {
        if self.live {
            clear_live_line();
            eprintln!("{}", format_repo_line(label, outcome));
        }
        self.state = None;
    }
}

fn clear_live_line() {
    let mut stderr = io::stderr();
    let _ = execute!(
        stderr,
        cursor::MoveToColumn(0),
        Clear(ClearType::CurrentLine)
    );
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
    let label = crate::style::truncate_with_ellipsis(&state.label, MAX_LABEL_WIDTH);
    let bar = format_progress_bar(state.overall as usize, OVERALL_SCALE_END as usize);
    let detail = match state.phase {
        ProgressPhase::Cloning | ProgressPhase::Checkout => format!("{}%", state.overall),
        _ => format!("{}/{}", state.checked_skills, state.total_skills),
    };
    format!(
        "{} {}/{} {} {} {} {} {}",
        style::styled(CYAN, frame),
        state.source_idx,
        state.source_count,
        label,
        bar,
        state.phase.label(),
        detail,
        format_elapsed(state.started.elapsed()),
    )
}

const BAR_CELLS: usize = 10;
const BAR_FILLED: char = '⣿';
const BAR_EMPTY: char = '⠀';

fn format_progress_bar(current: usize, total: usize) -> String {
    let filled = (BAR_CELLS * current.min(total))
        .checked_div(total)
        .unwrap_or(0);
    let mut bar = String::with_capacity(BAR_CELLS);
    bar.extend(std::iter::repeat_n(BAR_FILLED, filled));
    bar.extend(std::iter::repeat_n(BAR_EMPTY, BAR_CELLS - filled));
    format!("[{bar}]")
}

fn format_elapsed(elapsed: std::time::Duration) -> String {
    let total = elapsed.as_secs();
    if total < 60 {
        let tenths = elapsed.subsec_millis() / 100;
        format!("{}.{}s", total, tenths)
    } else {
        format!("{}m {}s", total / 60, total % 60)
    }
}

fn format_repo_line(label: &str, outcome: RepoOutcome) -> String {
    let marker = if outcome.is_clean() {
        style::styled(GREEN, "✔")
    } else {
        style::styled(YELLOW, "✖")
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
            started: std::time::Instant::now(),
            overall: 77,
        };

        let rendered = strip_ansi(&format_live_line(&state));

        assert!(
            rendered.starts_with("⠋ 2/5 owner/repo [⣿⣿⣿⣿"),
            "spinner+position+label+bar prefix, got: {rendered:?}"
        );
        assert!(rendered.contains("checking 3/7"));
        assert!(rendered.ends_with('s'), "elapsed suffix, got: {rendered:?}");
    }

    #[test]
    fn test_format_live_line_truncates_long_label() {
        let state = ProgressState {
            source_idx: 2,
            source_count: 5,
            label: "deanpeters/Product-Manager-Skills".to_string(),
            phase: ProgressPhase::Checking,
            checked_skills: 3,
            total_skills: 7,
            frame_idx: 0,
            started: std::time::Instant::now(),
            overall: 77,
        };

        let rendered = strip_ansi(&format_live_line(&state));
        assert!(
            rendered.contains("deanpeters/Product-Man..."),
            "label should be truncated with ellipsis, got: {:?}",
            rendered
        );
        assert!(
            style::display_width(&rendered) <= 80,
            "live line should fit in 80 display columns, got width {}: {:?}",
            style::display_width(&rendered),
            rendered
        );
    }

    #[test]
    fn test_format_live_line_clone_overall() {
        let state = ProgressState {
            source_idx: 1,
            source_count: 1,
            label: "owner/repo".to_string(),
            phase: ProgressPhase::Cloning,
            checked_skills: 0,
            total_skills: 7,
            frame_idx: 0,
            started: std::time::Instant::now(),
            overall: 35,
        };

        let rendered = strip_ansi(&format_live_line(&state));

        assert!(rendered.contains("cloning 35%"));
        assert!(
            rendered.contains("[⣿⣿⣿"),
            "35% of 10 cells = 3 filled, got: {rendered:?}"
        );
    }

    #[test]
    fn test_format_live_line_checkout_overall() {
        let state = ProgressState {
            source_idx: 1,
            source_count: 1,
            label: "owner/repo".to_string(),
            phase: ProgressPhase::Checkout,
            checked_skills: 0,
            total_skills: 7,
            frame_idx: 0,
            started: std::time::Instant::now(),
            overall: 47,
        };

        let rendered = strip_ansi(&format_live_line(&state));

        assert!(rendered.contains("checkout 47%"));
        assert!(
            rendered.contains("[⣿⣿⣿⣿"),
            "47% of 10 cells = 4 filled, got: {rendered:?}"
        );
    }

    #[test]
    fn test_format_progress_bar_full() {
        assert_eq!(format_progress_bar(7, 7), "[⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿]");
    }

    #[test]
    fn test_format_progress_bar_zero_total_does_not_panic() {
        assert_eq!(format_progress_bar(0, 0), "[⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀]");
    }

    #[test]
    fn test_format_progress_bar_half() {
        assert_eq!(format_progress_bar(5, 10), "[⣿⣿⣿⣿⣿⠀⠀⠀⠀⠀]");
    }

    #[test]
    fn test_format_elapsed_seconds() {
        assert_eq!(
            format_elapsed(std::time::Duration::from_millis(46_300)),
            "46.3s"
        );
    }

    #[test]
    fn test_format_elapsed_minutes() {
        assert_eq!(format_elapsed(std::time::Duration::from_secs(95)), "1m 35s");
    }

    #[test]
    fn test_format_elapsed_boundary_does_not_round_to_60s() {
        assert_eq!(
            format_elapsed(std::time::Duration::from_millis(59_960)),
            "59.9s"
        );
        assert_eq!(format_elapsed(std::time::Duration::from_secs(60)), "1m 0s");
    }

    #[test]
    fn test_clone_progress_updates_state() {
        let mut progress = UpdateProgress {
            live: false,
            state: Some(ProgressState {
                source_idx: 1,
                source_count: 1,
                label: "owner/repo".to_string(),
                phase: ProgressPhase::Cloning,
                checked_skills: 0,
                total_skills: 7,
                frame_idx: 0,
                started: std::time::Instant::now(),
                overall: 0,
            }),
        };

        progress.clone_progress(45);

        assert_eq!(progress.state.as_ref().unwrap().overall, 15);
    }

    #[test]
    fn test_overall_for_clone_segment() {
        assert_eq!(overall_for_clone(0), 0);
        assert_eq!(overall_for_clone(45), 15);
        assert_eq!(overall_for_clone(100), 35);
        assert_eq!(overall_for_clone(200), 35);
    }

    #[test]
    fn test_overall_for_checkout_segment() {
        assert_eq!(overall_for_checkout(0), 35);
        assert_eq!(overall_for_checkout(50), 47);
        assert_eq!(overall_for_checkout(100), 60);
    }

    #[test]
    fn test_overall_for_skills_segment() {
        assert_eq!(overall_for_skills(0, 7), 60);
        assert_eq!(overall_for_skills(3, 7), 77);
        assert_eq!(overall_for_skills(7, 7), 100);
        assert_eq!(overall_for_skills(0, 0), 60);
    }

    #[test]
    fn test_overall_never_moves_backward() {
        let mut progress = UpdateProgress {
            live: false,
            state: Some(ProgressState {
                source_idx: 1,
                source_count: 1,
                label: "owner/repo".to_string(),
                phase: ProgressPhase::Cloning,
                checked_skills: 0,
                total_skills: 2,
                frame_idx: 0,
                started: std::time::Instant::now(),
                overall: 0,
            }),
        };

        progress.clone_progress(100);
        assert_eq!(progress.state.as_ref().unwrap().overall, 35);
        progress.checkout_progress(20);
        assert_eq!(progress.state.as_ref().unwrap().overall, 40);
        progress.checkout_progress(100);
        assert_eq!(progress.state.as_ref().unwrap().overall, 60);
        progress.skill_checked();
        progress.skill_checked();
        assert_eq!(progress.state.as_ref().unwrap().overall, 100);
        progress.sync_found(2);
        assert_eq!(
            progress.state.as_ref().unwrap().overall,
            100,
            "extending the total must not move the bar backward"
        );
    }

    #[test]
    fn test_format_repo_line_clean_result() {
        let rendered = strip_ansi(&format_repo_line(
            "owner/repo",
            RepoOutcome::new(2, 3, 0, 0),
        ));

        assert!(rendered.contains("✔ owner/repo"));
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

        assert!(rendered.contains("✖ owner/repo"));
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
