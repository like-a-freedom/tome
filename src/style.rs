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
    unicode_width::UnicodeWidthChar::width(c).unwrap_or(0)
}

pub fn display_width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

pub const ELLIPSIS_LEN: usize = 3;

pub fn truncate_with_ellipsis(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if display_width(s) <= max_width {
        return s.to_string();
    }
    let ellipsis = if max_width >= ELLIPSIS_LEN { "..." } else { "" };
    let budget = max_width.saturating_sub(ellipsis.len());
    let mut end = 0;
    // Measure complete prefixes: variation selectors can change the width of
    // an earlier character, so summing character widths is insufficient.
    for (i, ch) in s.char_indices() {
        let next = i + ch.len_utf8();
        if display_width(&s[..next]) > budget {
            break;
        }
        end = next;
    }
    format!("{}{}", &s[..end], ellipsis)
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

    // === truncate_with_ellipsis ===

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact() {
        assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long() {
        assert_eq!(truncate_with_ellipsis("hello world", 5), "he...");
    }

    #[test]
    fn test_truncate_multibyte_cyrillic() {
        let s = "Привет мир";
        let result = truncate_with_ellipsis(s, 9);
        assert_eq!(result, "Привет...");
        assert!(result.is_char_boundary(0));
    }

    #[test]
    fn test_truncate_multibyte_emoji() {
        let s = "Hello 🌍 World 🎉 Test";
        let result = truncate_with_ellipsis(s, 7);
        assert_eq!(result, "Hell...");
    }

    #[test]
    fn test_truncate_em_dash() {
        let s =
            "Set up Husky pre-commit hooks with lint-staged (Prettier), type checking, and tests";
        let result = truncate_with_ellipsis(s, 47);
        assert_eq!(display_width(&result), 47);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_chinese() {
        let s = "这是一个测试描述用于验证多字节字符的截断功能是否正常工作";
        let result = truncate_with_ellipsis(s, 10);
        assert_eq!(result, "这是一...");
    }

    #[test]
    fn test_truncate_empty() {
        assert_eq!(truncate_with_ellipsis("", 10), "");
    }

    #[test]
    fn test_truncate_zero_max() {
        assert_eq!(truncate_with_ellipsis("hello", 0), "");
    }

    #[test]
    fn test_truncate_mixed_ascii_multibyte() {
        let s = "Hello Привет World";
        let result = truncate_with_ellipsis(s, 11);
        assert_eq!(result, "Hello Пр...");
    }

    #[test]
    fn test_truncate_small_width() {
        assert_eq!(truncate_with_ellipsis("hello", 2), "he");
    }

    #[test]
    fn test_truncate_unicode_small_width() {
        assert_eq!(truncate_with_ellipsis("🚀🚀🚀", 2), "🚀");
    }
    #[test]
    fn scalable_terminal_cells_account_for_emoji_and_combining_marks() {
        assert_eq!(display_width("🚀界e\u{301}"), 5);
        assert_eq!(truncate_with_ellipsis("🚀🚀", 2), "🚀");
    }
    #[test]
    fn review_common_bmp_emoji_have_two_terminal_cells() {
        assert_eq!(display_width("✅☕⌚❌⭐"), 10);
        assert_eq!(
            truncate_with_ellipsis(&"✅".repeat(45), 39),
            format!("{}...", "✅".repeat(18))
        );
        assert_eq!(display_width("✈️"), 2);
        assert_eq!(display_width(&truncate_with_ellipsis("✈️✈️", 1)), 1);
    }
}
