use anyhow::Result;
use crossterm::{
    cursor::MoveTo,
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};
use std::io::{self, Write};

/// RAII guard: disables raw mode on drop.
struct RawGuard;

impl RawGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        Ok(RawGuard)
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

/// Live progress display for update operations.
///
/// Renders a header line ("Updating N skill(s)...") followed by one
/// status line per skill. Each skill line is first shown as "⏳ name"
/// (fetching), then replaced with its final result (✓ or ✗).
///
/// On drop the underlying `RawGuard` restores the terminal to normal
/// mode, so even if the process panics the terminal is recovered.
pub struct Progress {
    _guard: RawGuard,
    stdout: io::Stdout,
    current_line: u16,
}

impl Progress {
    /// Enter raw mode, clear the screen, and print the header.
    pub fn start(total: usize) -> Result<Self> {
        let guard = RawGuard::enter()?;
        let mut stdout = io::stdout();

        execute!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;
        write!(stdout, "Updating {} skill(s)...\r\n\r\n", total)?;
        stdout.flush()?;

        Ok(Progress {
            _guard: guard,
            stdout,
            current_line: 2,
        })
    }

    /// Show that a skill is being fetched/cloned.
    pub fn fetching(&mut self, name: &str) -> Result<()> {
        execute!(self.stdout, MoveTo(0, self.current_line))?;
        execute!(self.stdout, Clear(ClearType::CurrentLine))?;
        write!(self.stdout, "  ⏳ {}", name)?;
        self.stdout.flush()?;
        Ok(())
    }

    /// Skill was updated to a new commit.
    pub fn updated(&mut self, name: &str, old_commit: &str, new_commit: &str) -> Result<()> {
        self.put_result(&format!(
            "✓  {}  {} -> {}",
            name, old_commit, new_commit
        ))
    }

    /// Skill was already at the latest commit.
    pub fn skipped(&mut self, name: &str, commit: &str) -> Result<()> {
        self.put_result(&format!("✓  {}  up to date ({})", name, commit))
    }

    /// Skill update failed with an error.
    pub fn error(&mut self, name: &str, msg: &str) -> Result<()> {
        self.put_result(&format!("✗  {}  {}", name, msg))
    }

    /// Overwrite the current line with a result and advance to the next line.
    fn put_result(&mut self, line: &str) -> Result<()> {
        execute!(self.stdout, MoveTo(0, self.current_line))?;
        execute!(self.stdout, Clear(ClearType::CurrentLine))?;
        // No \r\n — stay on this line; MoveTo will reposition for the
        // next skill's fetching call.
        write!(self.stdout, "{}", line)?;
        self.stdout.flush()?;
        self.current_line += 1;
        Ok(())
    }

    /// Write a final blank line and drop raw mode (via RawGuard).
    pub fn done(mut self) -> Result<()> {
        write!(self.stdout, "\r\n")?;
        self.stdout.flush()?;
        Ok(())
    }
}
