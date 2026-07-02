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

/// Wrap `s` in ANSI escape codes: `{code}{s}{RESET}`.
pub fn styled(code: &str, s: &str) -> String {
    format!("{}{}{}", code, s, RESET)
}

// ── Box-drawing helpers ──────────────────────────────────────────

pub fn char_display_width(c: char) -> usize {
    let cp = c as u32;
    if (0x1100..=0x115F).contains(&cp)
        || (0x2E80..=0x303E).contains(&cp)
        || (0x3040..=0x33BF).contains(&cp)
        || (0x3400..=0x4DBF).contains(&cp)
        || (0x4E00..=0x9FFF).contains(&cp)
        || (0xA000..=0xA4CF).contains(&cp)
        || (0xAC00..=0xD7AF).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0xFE30..=0xFE6F).contains(&cp)
        || (0xFF01..=0xFF60).contains(&cp)
        || (0xFFE0..=0xFFE6).contains(&cp)
        || (0x20000..=0x2FA1F).contains(&cp)
    {
        2
    } else {
        1
    }
}

pub fn display_width(s: &str) -> usize {
    s.chars().map(char_display_width).sum()
}

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
    let pad = inner.saturating_sub(display_width(plain));
    format!("│  {}{}  │", styled_text, " ".repeat(pad))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_box_line_uses_display_width_for_unicode_punctuation() {
        let plain = "33 updated · 109 up to date";
        let rendered = box_line(plain, plain, 60);

        assert_eq!(display_width(&rendered), 60);
    }
}
