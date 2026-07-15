use crate::style::{self, CYAN, DIM, GREEN, YELLOW};

const TERM_WIDTH: usize = 60;
const SHORT_COMMIT_LEN: usize = 7;

pub(crate) fn format_commit(commit: &str) -> String {
    if commit.len() >= SHORT_COMMIT_LEN {
        commit[..SHORT_COMMIT_LEN].to_string()
    } else {
        commit.to_string()
    }
}

pub(crate) fn total_sources(file_skill_count: usize, git_group_count: usize) -> usize {
    file_skill_count + git_group_count
}

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

pub(crate) fn print_summary(
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
    fn test_total_sources_counts_file_skills_and_git_groups() {
        let file_skill_count = 2usize;
        let git_group_count = 3usize;

        assert_eq!(total_sources(file_skill_count, git_group_count), 5);
    }

    #[test]
    fn test_total_sources_is_at_least_one_for_non_empty_update() {
        assert_eq!(total_sources(1, 0), 1);
        assert_eq!(total_sources(0, 1), 1);
    }

    #[test]
    fn test_format_commit() {
        assert_eq!(format_commit("abc1234567890abcdef"), "abc1234");
        assert_eq!(format_commit("abc"), "abc");
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
