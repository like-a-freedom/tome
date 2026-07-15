use crate::style::{self, CYAN, DIM, GREEN, YELLOW};

pub(crate) fn print_installed(name: &str) {
    eprintln!(
        "  {} Installed {}{}{}",
        style::styled(GREEN, "\u{2713}"),
        style::BOLD,
        name,
        style::RESET,
    );
}

pub(crate) fn print_installed_multi(names: &[String]) {
    eprintln!();
    eprintln!(
        "  {} Installed {} skills:\n",
        style::styled(GREEN, "\u{2713}"),
        names.len(),
    );
    for name in names {
        eprintln!("    {}", name);
    }
}

pub(crate) fn print_updated(name: &str, version: &str, old_commit: &str, new_commit: &str) {
    if version.is_empty() {
        println!(
            "  {} Updated: {} ({} -> {})",
            style::styled(GREEN, "\u{2713}"),
            name,
            old_commit,
            new_commit,
        );
    } else {
        println!(
            "  {} Updated: {} {} ({} -> {})",
            style::styled(GREEN, "\u{2713}"),
            name,
            version,
            old_commit,
            new_commit,
        );
    }
}

pub(crate) fn print_up_to_date(name: &str, commit: &str) {
    if commit.is_empty() || commit == "file" {
        println!(
            "  {} Already up to date: {} (file)",
            style::styled(DIM, "\u{2713}"),
            name,
        );
    } else {
        println!(
            "  {} Already up to date: {} ({})",
            style::styled(DIM, "\u{2713}"),
            name,
            commit,
        );
    }
}

pub(crate) fn print_file_updated(name: &str) {
    println!(
        "  {} Updated: {} (file)",
        style::styled(GREEN, "\u{2713}"),
        name,
    );
}

pub(crate) fn print_skipped_tracked(name: &str) {
    eprintln!(
        "  {} {} is already installed (use --force to overwrite)",
        style::styled(DIM, "\u{2713}"),
        name,
    );
}

pub(crate) fn print_skipped_existing(name: &str) {
    eprintln!(
        "  {} {} directory exists (not tracked, use --force)",
        style::styled(YELLOW, "\u{26a0}"),
        name,
    );
}

pub(crate) fn print_failed(name: &str, action: &str, detail: &str) {
    eprintln!(
        "  {} Failed to {} {}: {}",
        style::styled(YELLOW, "\u{2717}"),
        action,
        name,
        detail,
    );
}

pub(crate) fn print_failed_action(action: &str, detail: &str) {
    eprintln!(
        "  {} {}: {}",
        style::styled(YELLOW, "\u{2717}"),
        action,
        detail,
    );
}

pub(crate) fn print_removed(name: &str) {
    eprintln!(
        "  {} Removed {} \u{2014} no longer in repository",
        style::styled(YELLOW, "\u{2717}"),
        name,
    );
}

pub(crate) fn print_stale_header(count: usize, names: &[String]) {
    eprintln!(
        "{} Skipping {} stale entries: {}",
        style::styled(YELLOW, "\u{26a0}"),
        count,
        names.join(", "),
    );
}

pub(crate) fn print_cloning_multi(label: &str, idx: usize, count: usize, skill_count: usize) {
    if count > 1 {
        eprintln!(
            "{} [{}/{}] Cloning {} ({} skill{})",
            style::styled(CYAN, "\u{25b8}"),
            idx + 1,
            count,
            label,
            skill_count,
            if skill_count == 1 { "" } else { "s" },
        );
    } else {
        eprintln!("{} Cloning {}", style::styled(CYAN, "\u{25b8}"), label,);
    }
}
