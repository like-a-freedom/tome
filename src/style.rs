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
