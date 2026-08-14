use crate::style::{self, CYAN, DIM, GREEN, YELLOW};

fn check_mark() -> &'static str {
    "\u{2713}"
}
fn warning_mark() -> &'static str {
    "\u{26a0}"
}
fn cross_mark() -> &'static str {
    "\u{2717}"
}

pub(crate) fn format_installed(name: &str) -> String {
    format!(
        "  {} Installed {}{}{}",
        style::styled(GREEN, check_mark()),
        style::BOLD,
        name,
        style::RESET,
    )
}

pub(crate) fn print_installed(name: &str) {
    eprintln!("{}", format_installed(name));
}

pub(crate) fn format_installed_multi(names: &[String]) -> String {
    let mut out = String::new();
    out.push('\n');
    out.push_str(&format!(
        "  {} Installed {} skills:\n",
        style::styled(GREEN, check_mark()),
        names.len(),
    ));
    for name in names {
        out.push_str(&format!("    {}\n", name));
    }
    out
}

pub(crate) fn print_installed_multi(names: &[String]) {
    eprint!("{}", format_installed_multi(names));
}

pub(crate) fn format_updated(
    name: &str,
    version: &str,
    old_commit: &str,
    new_commit: &str,
) -> String {
    if version.is_empty() {
        format!(
            "  {} Updated: {} ({} -> {})",
            style::styled(GREEN, check_mark()),
            name,
            old_commit,
            new_commit,
        )
    } else {
        format!(
            "  {} Updated: {} {} ({} -> {})",
            style::styled(GREEN, check_mark()),
            name,
            version,
            old_commit,
            new_commit,
        )
    }
}

pub(crate) fn print_updated(name: &str, version: &str, old_commit: &str, new_commit: &str) {
    println!("{}", format_updated(name, version, old_commit, new_commit));
}

pub(crate) fn format_up_to_date(name: &str, commit_or_marker: &str) -> String {
    if commit_or_marker.is_empty() || commit_or_marker == "file" {
        format!(
            "  {} Already up to date: {} (file)",
            style::styled(DIM, check_mark()),
            name,
        )
    } else {
        format!(
            "  {} Already up to date: {} ({})",
            style::styled(DIM, check_mark()),
            name,
            commit_or_marker,
        )
    }
}

pub(crate) fn print_up_to_date(name: &str, commit_or_marker: &str) {
    println!("{}", format_up_to_date(name, commit_or_marker));
}

pub(crate) fn format_file_updated(name: &str) -> String {
    format!(
        "  {} Updated: {} (file)",
        style::styled(GREEN, check_mark()),
        name,
    )
}

pub(crate) fn print_file_updated(name: &str) {
    println!("{}", format_file_updated(name));
}

pub(crate) fn format_failed(name: &str, action: &str, detail: &str) -> String {
    format!(
        "  {} Failed to {} {}: {}",
        style::styled(YELLOW, cross_mark()),
        action,
        name,
        detail,
    )
}

pub(crate) fn print_failed(name: &str, action: &str, detail: &str) {
    eprintln!("{}", format_failed(name, action, detail));
}

pub(crate) fn format_removed(name: &str) -> String {
    format!(
        "  {} Removed {} \u{2014} no longer in repository",
        style::styled(YELLOW, cross_mark()),
        name,
    )
}

pub(crate) fn print_removed(name: &str) {
    eprintln!("{}", format_removed(name));
}

pub(crate) fn format_skipped_tracked(name: &str) -> String {
    format!(
        "  {} {} is already installed (use --force to overwrite)",
        style::styled(DIM, check_mark()),
        name,
    )
}

pub(crate) fn print_skipped_tracked(name: &str) {
    eprintln!("{}", format_skipped_tracked(name));
}

pub(crate) fn format_skipped_existing(name: &str) -> String {
    format!(
        "  {} {} directory exists (not tracked, use --force)",
        style::styled(YELLOW, warning_mark()),
        name,
    )
}

pub(crate) fn print_skipped_existing(name: &str) {
    eprintln!("{}", format_skipped_existing(name));
}

pub(crate) fn format_stale_header(count: usize, names: &[String]) -> String {
    format!(
        "{} Skipping {} stale entries: {}",
        style::styled(YELLOW, warning_mark()),
        count,
        names.join(", "),
    )
}

pub(crate) fn print_stale_header(count: usize, names: &[String]) {
    eprintln!("{}", format_stale_header(count, names));
}

const TERM_WIDTH: usize = 60;

fn format_duration(secs: f64) -> String {
    let time = if secs < 60.0 {
        format!("{:.1} s", secs)
    } else {
        format!("{:.0}m {:.0} s", secs / 60.0, secs % 60.0)
    };
    format!("Completed in {}", time)
}

fn format_summary_line(summary_plain: &str, summary_styled: &str) -> String {
    format!(
        "{}{}",
        CYAN,
        style::box_line(
            summary_plain,
            &format!("{}{}", summary_styled, CYAN),
            TERM_WIDTH,
        )
    )
}

pub(crate) fn print_update_summary(
    total_updated: usize,
    total_skipped: usize,
    total_failed: usize,
    total_removed: usize,
    elapsed: f64,
) {
    let mut parts: Vec<String> = Vec::new();
    if total_updated > 0 {
        parts.push(style::styled(GREEN, &format!("{} updated", total_updated)));
    }
    if total_skipped > 0 {
        parts.push(style::styled(DIM, &format!("{} up to date", total_skipped)));
    }
    if total_removed > 0 {
        parts.push(style::styled(YELLOW, &format!("{} removed", total_removed)));
    }
    if total_failed > 0 {
        parts.push(style::styled(YELLOW, &format!("{} failed", total_failed)));
    }

    let summary_plain = if parts.is_empty() {
        "nothing to update".to_string()
    } else {
        let plain_parts: Vec<String> = vec![
            if total_updated > 0 {
                format!("{} updated", total_updated)
            } else {
                String::new()
            },
            if total_skipped > 0 {
                format!("{} up to date", total_skipped)
            } else {
                String::new()
            },
            if total_removed > 0 {
                format!("{} removed", total_removed)
            } else {
                String::new()
            },
            if total_failed > 0 {
                format!("{} failed", total_failed)
            } else {
                String::new()
            },
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
        plain_parts.join(" \u{00b7} ")
    };
    let summary_styled = parts.join(" \u{00b7} ");

    let time_str = format_duration(elapsed);

    println!(
        "{}{}\n{}\n{}{}\n{}{}",
        CYAN,
        style::box_top("Result", TERM_WIDTH),
        format_summary_line(&summary_plain, &summary_styled),
        CYAN,
        style::box_line(&time_str, &time_str, TERM_WIDTH),
        CYAN,
        style::box_bottom(TERM_WIDTH),
    );
    println!("{}", style::RESET);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_installed() {
        let out = format_installed("my-skill");
        assert!(out.contains("Installed"));
        assert!(out.contains("my-skill"));
        assert!(out.contains(&style::styled(GREEN, check_mark())));
    }

    #[test]
    fn test_format_installed_bold_name() {
        let out = format_installed("my-skill");
        assert!(out.contains(&format!("{}{}", style::BOLD, "my-skill")));
    }

    #[test]
    fn test_format_installed_multi_single() {
        let names = vec!["alpha".to_string()];
        let out = format_installed_multi(&names);
        assert!(out.contains("Installed 1 skills"));
        assert!(out.contains("    alpha"));
    }

    #[test]
    fn test_format_installed_multi_multiple() {
        let names = vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()];
        let out = format_installed_multi(&names);
        assert!(out.contains("Installed 3 skills"));
        assert!(out.contains("    alpha"));
        assert!(out.contains("    beta"));
        assert!(out.contains("    gamma"));
    }

    #[test]
    fn test_format_installed_multi_starts_with_newline() {
        let out = format_installed_multi(&["a".to_string()]);
        assert!(out.starts_with('\n'));
    }

    #[test]
    fn test_format_installed_multi_ends_with_newline() {
        let out = format_installed_multi(&["a".to_string()]);
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn test_format_updated_no_version() {
        let out = format_updated("my-skill", "", "abc1234", "def5678");
        assert!(out.contains("Updated: my-skill"));
        assert!(out.contains("(abc1234 -> def5678)"));
        assert!(!out.contains("v1"));
    }

    #[test]
    fn test_format_updated_with_version() {
        let out = format_updated("my-skill", "v1.2.3", "abc1234", "def5678");
        assert!(out.contains("Updated: my-skill v1.2.3"));
        assert!(out.contains("(abc1234 -> def5678)"));
    }

    #[test]
    fn test_format_up_to_date_with_commit() {
        let out = format_up_to_date("my-skill", "abc1234");
        assert!(out.contains("Already up to date: my-skill"));
        assert!(out.contains("(abc1234)"));
    }

    #[test]
    fn test_format_up_to_date_with_file_marker() {
        let out = format_up_to_date("my-skill", "file");
        assert!(out.contains("Already up to date: my-skill"));
        assert!(out.contains("(file)"));
        assert!(!out.contains("(file "));
    }

    #[test]
    fn test_format_up_to_date_with_empty_marker() {
        // Empty marker treated as file marker (defensive)
        let out = format_up_to_date("my-skill", "");
        assert!(out.contains("(file)"));
    }

    #[test]
    fn test_format_up_to_date_uses_dim_color() {
        let out = format_up_to_date("my-skill", "abc");
        assert!(out.contains(&style::styled(DIM, check_mark())));
    }

    #[test]
    fn test_format_file_updated() {
        let out = format_file_updated("my-skill");
        assert!(out.contains("Updated: my-skill (file)"));
    }

    #[test]
    fn test_format_failed() {
        let out = format_failed("my-skill", "update", "download failed");
        assert!(out.contains("Failed to update my-skill: download failed"));
        assert!(out.contains(&style::styled(YELLOW, cross_mark())));
    }

    #[test]
    fn test_format_removed() {
        let out = format_removed("dead-skill");
        assert!(out.contains("Removed dead-skill"));
    }

    #[test]
    fn test_format_skipped_tracked() {
        let out = format_skipped_tracked("my-skill");
        assert!(out.contains("my-skill is already installed"));
        assert!(out.contains("--force"));
    }

    #[test]
    fn test_format_skipped_existing() {
        let out = format_skipped_existing("my-skill");
        assert!(out.contains("my-skill directory exists"));
        assert!(out.contains("not tracked"));
        assert!(out.contains("--force"));
    }

    #[test]
    fn test_format_skipped_existing_uses_warning_mark() {
        let out = format_skipped_existing("my-skill");
        assert!(out.contains(&style::styled(YELLOW, warning_mark())));
    }

    #[test]
    fn test_format_stale_header() {
        let names = vec!["a".to_string(), "b".to_string()];
        let out = format_stale_header(2, &names);
        assert!(out.contains("Skipping 2 stale entries"));
        assert!(out.contains("a, b"));
    }

    #[test]
    fn test_summary_line_restores_frame_color_after_styled_segments() {
        let plain = "1 updated \u{00b7} 141 up to date";
        let styled = format!(
            "{} \u{00b7} {}",
            style::styled(GREEN, "1 updated"),
            style::styled(DIM, "141 up to date")
        );
        let rendered = format_summary_line(plain, &styled);
        let content_end = rendered.rfind(style::RESET).unwrap() + style::RESET.len();

        assert!(
            rendered[content_end..].starts_with(CYAN),
            "expected cyan frame after styled content, got {:?}",
            &rendered[content_end..]
        );
    }
}
