/// ANSI escape codes for styled terminal output.
///
/// These are kept intentionally minimal — no abstraction layer, just the raw
/// codes so they compose naturally with `write!`/`println!`.
pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const CYAN: &str = "\x1b[36m";
pub const RED: &str = "\x1b[31m";

/// Wrap `s` in ANSI escape codes: `{code}{s}{RESET}`.
pub fn styled(code: &str, s: &str) -> String {
    format!("{}{}{}", code, s, RESET)
}

// ── Box-drawing helpers ──────────────────────────────────────────

pub fn line(c: char, width: usize) -> String {
    std::iter::repeat_n(c, width).collect()
}

/// Top border:  ╭── label ── ... ──╮  (padded to `width`)
pub fn box_top(label: &str, width: usize) -> String {
    let prefix = 3; // "╭─ "
    let suffix = 2; // "─╮"
    let inner = width.saturating_sub(prefix + suffix);
    let label_trunc: String = label.chars().take(inner.saturating_sub(1)).collect();
    let pad = inner.saturating_sub(label_trunc.len() + 1);
    format!("╭─ {} {}─╮", label_trunc, line('─', pad))
}

/// Bottom border:  ╰── ... ──╯  (padded to `width`)
pub fn box_bottom(width: usize) -> String {
    format!("╰{}╯", line('─', width.saturating_sub(2)))
}

/// A content line inside a box:  │  text + pad  │
///
/// `plain` is the visible text (no ANSI), `styled` may include ANSI codes.
pub fn box_line(plain: &str, styled_text: &str, width: usize) -> String {
    let inner = width.saturating_sub(6); // "│  " + "  │"
    let pad = inner.saturating_sub(plain.len());
    format!("│  {}{}  │", styled_text, " ".repeat(pad))
}
