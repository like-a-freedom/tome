use anyhow::{Result, bail};
use crossterm::{
    Command,
    cursor::MoveTo,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    style::{Attribute, SetAttribute},
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode, size},
};
use std::io::{self, Write};

use crate::style::display_width;

struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

const DEFAULT_NAME_WIDTH: usize = 20;
const MAX_NAME_WIDTH: usize = 30;
const NUM_COL_WIDTH: usize = 2;
const MENU_FALLBACK_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;
const MENU_PREFIX_WIDTH: usize = 13;
const MIN_DESC_WIDTH: usize = 15;
const MAX_DESC_WIDTH: usize = 60;

pub struct SelectItem {
    pub name: String,
    pub description: String,
    pub selected: bool,
}

pub fn calc_name_width(items: &[SelectItem]) -> usize {
    items
        .iter()
        .map(|i| display_width(&i.name))
        .max()
        .unwrap_or(DEFAULT_NAME_WIDTH)
        .min(MAX_NAME_WIDTH)
}

#[derive(Default)]
struct FilterState {
    query: String,
    focused: bool,
}

pub fn apply_filter(items: &[SelectItem], query: &str) -> Vec<usize> {
    let needle = query.to_lowercase();
    if needle.is_empty() {
        return (0..items.len()).collect();
    }
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item.name.to_lowercase().contains(&needle)
                || item.description.to_lowercase().contains(&needle)
        })
        .map(|(index, _)| index)
        .collect()
}

const CHROME_ROWS: u16 = 8;
const MIN_VIEW_HEIGHT: usize = 5;

/// View state of the selection dialog. `cursor`/`offset` index into
/// `filtered` (never into `items`); selection flags stay on `SelectItem`.
struct TuiState {
    filtered: Vec<usize>,
    cursor: usize,
    offset: usize,
    height: usize,
    filter: FilterState,
}

impl TuiState {
    fn new(item_count: usize, height: usize) -> Self {
        Self {
            filtered: (0..item_count).collect(),
            cursor: 0,
            offset: 0,
            height,
            filter: FilterState::default(),
        }
    }

    /// Recompute `filtered` from the current query; reset cursor/offset.
    fn refilter(&mut self, items: &[SelectItem]) {
        self.filtered = apply_filter(items, &self.filter.query);
        self.cursor = 0;
        self.offset = 0;
    }

    /// Move cursor (clamped to the filtered range) and keep it visible.
    fn goto(&mut self, pos: usize) {
        self.cursor = pos.min(self.filtered.len().saturating_sub(1));
        self.offset =
            clamp_view_offset(self.cursor, self.offset, self.height, self.filtered.len());
    }
}

fn clamp_view_offset(cursor: usize, offset: usize, height: usize, total: usize) -> usize {
    if total == 0 || height == 0 {
        return 0;
    }
    let max_offset = total.saturating_sub(height.min(total));
    let mut next = offset.min(max_offset);
    if cursor < next {
        next = cursor;
    }
    if cursor >= next + height {
        next = cursor + 1 - height;
    }
    next.min(max_offset)
}

fn page_step(cursor: usize, total: usize, height: usize, down: bool) -> usize {
    if total == 0 {
        return 0;
    }
    if down {
        (cursor + height).min(total - 1)
    } else {
        cursor.saturating_sub(height)
    }
}

fn view_height(term_rows: u16) -> usize {
    (term_rows.saturating_sub(CHROME_ROWS) as usize).max(MIN_VIEW_HEIGHT)
}

pub fn format_item(
    i: usize,
    item: &SelectItem,
    name_width: usize,
    desc_width: usize,
    focused: bool,
) -> String {
    let checkbox = if item.selected { "[x]" } else { "[ ]" };
    let num = format!("{:>width$}.", i + 1, width = NUM_COL_WIDTH);
    let name = crate::style::truncate_with_ellipsis(&item.name, name_width);
    let desc = crate::style::truncate_with_ellipsis(&item.description, desc_width);
    let marker = if focused { ">" } else { " " };
    let pad = name_width.saturating_sub(display_width(&name));
    format!(
        "{} {} {} {}{} - {}",
        marker,
        num,
        checkbox,
        name,
        " ".repeat(pad),
        desc
    )
}

fn build_render_buffer(items: &[SelectItem], cursor: usize, desc_width: usize) -> String {
    let selected_count = items.iter().filter(|i| i.selected).count();

    let mut buf = String::new();
    Clear(ClearType::All)
        .write_ansi(&mut buf)
        .expect("writing to String is infallible");
    MoveTo(0, 0)
        .write_ansi(&mut buf)
        .expect("writing to String is infallible");
    buf.push_str(&format!(
        "Select skills to install ({}/{}):\r\n",
        selected_count,
        items.len()
    ));
    buf.push_str("  Arrows/j/k: navigate | Space: toggle | a: all | n: none | Enter: confirm | q: cancel\r\n");
    buf.push_str("\r\n");

    let name_width = calc_name_width(items);
    for (i, item) in items.iter().enumerate() {
        let focused = i == cursor;
        if focused {
            SetAttribute(Attribute::Bold)
                .write_ansi(&mut buf)
                .expect("writing to String is infallible");
        }
        buf.push_str(&format_item(i, item, name_width, desc_width, focused));
        if focused {
            SetAttribute(Attribute::Reset)
                .write_ansi(&mut buf)
                .expect("writing to String is infallible");
        }
        buf.push_str("\r\n");
    }

    buf.push_str("\r\n");
    if selected_count > 0 {
        buf.push_str(&format!(
            "Press Enter to install {} skill(s)\r\n",
            selected_count
        ));
    } else {
        buf.push_str("Select at least one skill, or press q to cancel\r\n");
    }

    buf
}

fn render_all(
    items: &[SelectItem],
    cursor: usize,
    desc_width: usize,
    stdout: &mut io::Stdout,
) -> Result<()> {
    let buf = build_render_buffer(items, cursor, desc_width);
    write!(stdout, "{}", buf)?;
    stdout.flush()?;
    Ok(())
}

pub fn multi_select(items: &mut [SelectItem]) -> Result<()> {
    if items.is_empty() {
        bail!("No items to select");
    }

    for item in items.iter_mut() {
        item.selected = true;
    }

    let _raw = RawModeGuard::enable()?;
    let mut stdout = io::stdout();
    let mut state = TuiState::new(items.len(), 10);

    let (cols, _) = size().unwrap_or((MENU_FALLBACK_COLS, DEFAULT_ROWS));
    let name_width = calc_name_width(items);
    let desc_width = (cols as usize)
        .saturating_sub(MENU_PREFIX_WIDTH)
        .saturating_sub(name_width)
        .clamp(MIN_DESC_WIDTH, MAX_DESC_WIDTH);

    (|| -> Result<()> {
        loop {
            render_all(items, state.cursor, desc_width, &mut stdout)?;

            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                if let Some(result) = handle_key(key, items, &mut state) {
                    return result;
                }
            }
        }
    })()
}

/// Shared cursor movement for both modes. Returns true when handled.
fn apply_navigation(code: KeyCode, state: &mut TuiState) -> bool {
    match code {
        KeyCode::Up => state.goto(state.cursor.saturating_sub(1)),
        KeyCode::Down => state.goto(state.cursor + 1),
        KeyCode::PageUp => {
            state.goto(page_step(
                state.cursor,
                state.filtered.len(),
                state.height,
                false,
            ));
        }
        KeyCode::PageDown => {
            state.goto(page_step(
                state.cursor,
                state.filtered.len(),
                state.height,
                true,
            ));
        }
        KeyCode::Home => state.goto(0),
        KeyCode::End => state.goto(usize::MAX),
        _ => return false,
    }
    true
}

/// Pure key-policy for the selection loop. Returns `Some(Ok(()))` on confirm,
/// `Some(Err(..))` on cancel, and `None` to keep looping.
fn handle_key(
    event: KeyEvent,
    items: &mut [SelectItem],
    state: &mut TuiState,
) -> Option<Result<()>> {
    if state.filter.focused {
        return handle_filter_key(event, items, state);
    }
    if apply_navigation(event.code, state) {
        return None;
    }
    match event.code {
        KeyCode::Char('q') | KeyCode::Esc => Some(Err(anyhow::anyhow!("Selection cancelled"))),
        KeyCode::Char('/') => {
            state.filter.focused = true;
            None
        }
        KeyCode::Char('k') => {
            state.goto(state.cursor.saturating_sub(1));
            None
        }
        KeyCode::Char('j') => {
            state.goto(state.cursor + 1);
            None
        }
        KeyCode::Char(' ') => {
            if let Some(&index) = state.filtered.get(state.cursor) {
                items[index].selected = !items[index].selected;
            }
            None
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            let all_selected = state.filtered.iter().all(|&i| items[i].selected);
            for &i in state.filtered.iter() {
                items[i].selected = !all_selected;
            }
            None
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            for &i in state.filtered.iter() {
                items[i].selected = false;
            }
            None
        }
        KeyCode::Enter if items.iter().any(|i| i.selected) => Some(Ok(())),
        _ => None,
    }
}

fn handle_filter_key(
    event: KeyEvent,
    items: &mut [SelectItem],
    state: &mut TuiState,
) -> Option<Result<()>> {
    if apply_navigation(event.code, state) {
        return None;
    }
    match event.code {
        KeyCode::Esc => {
            state.filter.focused = false;
            None
        }
        KeyCode::Backspace => {
            state.filter.query.pop();
            state.refilter(items);
            None
        }
        KeyCode::Char('u') if event.modifiers.contains(KeyModifiers::CONTROL) => {
            state.filter.query.clear();
            state.refilter(items);
            None
        }
        KeyCode::Char(c)
            if !event.modifiers.contains(KeyModifiers::CONTROL)
                && !event.modifiers.contains(KeyModifiers::ALT) =>
        {
            state.filter.query.push(c);
            state.refilter(items);
            None
        }
        KeyCode::Enter if items.iter().any(|i| i.selected) => Some(Ok(())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === calc_name_width ===

    #[test]
    fn test_calc_name_width_empty() {
        let items: Vec<SelectItem> = vec![];
        assert_eq!(calc_name_width(&items), 20);
    }

    #[test]
    fn test_calc_name_width_single() {
        let items = vec![SelectItem {
            name: "my-skill".into(),
            description: "".into(),
            selected: false,
        }];
        assert_eq!(calc_name_width(&items), 8);
    }

    #[test]
    fn test_calc_name_width_max_30() {
        let items = vec![SelectItem {
            name: "a".repeat(40),
            description: "".into(),
            selected: false,
        }];
        assert_eq!(calc_name_width(&items), 30);
    }

    #[test]
    fn test_calc_name_width_multibyte() {
        let items = vec![
            SelectItem {
                name: "skill-a".into(),
                description: "".into(),
                selected: false,
            },
            SelectItem {
                name: "скилл-б".into(),
                description: "".into(),
                selected: false,
            },
        ];
        assert_eq!(calc_name_width(&items), 7);
    }

    // === format_item ===

    #[test]
    fn test_format_item_selected() {
        let item = SelectItem {
            name: "test".into(),
            description: "A test skill".into(),
            selected: true,
        };
        let result = format_item(0, &item, 10, 47, false);
        assert!(result.contains("[x]"));
        assert!(result.contains("test"));
    }

    #[test]
    fn test_format_item_unselected() {
        let item = SelectItem {
            name: "test".into(),
            description: "A test skill".into(),
            selected: false,
        };
        let result = format_item(0, &item, 10, 47, false);
        assert!(result.contains("[ ]"));
    }

    #[test]
    fn test_format_item_focused_marker() {
        let item = SelectItem {
            name: "test".into(),
            description: "desc".into(),
            selected: false,
        };
        let focused = format_item(0, &item, 10, 47, true);
        let unfocused = format_item(0, &item, 10, 47, false);
        assert!(focused.starts_with(">"));
        assert!(unfocused.starts_with(" "));
    }

    #[test]
    fn test_format_item_numbering() {
        let item = SelectItem {
            name: "test".into(),
            description: "desc".into(),
            selected: false,
        };
        assert!(format_item(0, &item, 10, 47, false).contains(" 1."));
        assert!(format_item(9, &item, 10, 47, false).contains("10."));
        assert!(format_item(33, &item, 10, 47, false).contains("34."));
    }

    #[test]
    fn test_format_item_truncation() {
        let item = SelectItem {
            name: "very-long-name-that-should-be-truncated".into(),
            description: "This is a very long description that should definitely be truncated"
                .into(),
            selected: false,
        };
        let result = format_item(0, &item, 10, 47, false);
        assert!(result.contains("very-lo..."));
    }

    #[test]
    fn test_format_item_multibyte_no_panic() {
        let item = SelectItem {
            name: "setup-pre-commit".into(),
            description: "Set up Husky pre-commit hooks with lint-staged (Prettier), type checking, and tests".into(),
            selected: true,
        };
        let result = format_item(0, &item, 20, 47, false);
        assert!(result.contains("[x]"));
    }

    #[test]
    fn test_format_item_cyrillic_name() {
        let item = SelectItem {
            name: "настройка".into(),
            description: "Тестовое описание".into(),
            selected: false,
        };
        let result = format_item(0, &item, 20, 47, false);
        assert!(result.contains("настройка"));
    }

    // === alignment verification ===

    #[test]
    fn test_format_item_alignment() {
        let items = [
            SelectItem { name: "setup-pre-commit".into(), description: "Set up Husky pre-commit hooks with lint-staged (Prettier), type checking, and tests".into(), selected: true },
            SelectItem { name: "improve-codebase-architecture".into(), description: "Scan a codebase for deepening opportunities, produce an architectural refactor plan, and produce tiny forward-commits".into(), selected: true },
            SelectItem { name: "scaffold-exercises".into(), description: "Create exercise directory structures with section files, instructions, and test bases".into(), selected: true },
            SelectItem { name: "git-guardrails-claude-code".into(), description: "Set up Claude Code hooks to block dangerous git operations such as force pushes".into(), selected: true },
        ];

        let name_width = calc_name_width(&items);
        let desc_width = (80usize)
            .saturating_sub(13)
            .saturating_sub(name_width)
            .clamp(15, 60);

        let lines: Vec<String> = items
            .iter()
            .enumerate()
            .map(|(i, item)| format_item(i, item, name_width, desc_width, i == 0))
            .collect();

        let widths: Vec<usize> = lines.iter().map(|l| display_width(l)).collect();
        let expected = 13 + name_width + desc_width;
        for (i, w) in widths.iter().enumerate() {
            assert_eq!(
                *w, expected,
                "Line {} ({}) has display width {} != {}",
                i, lines[i], w, expected
            );
        }

        let sep_pos: Vec<usize> = lines
            .iter()
            .map(|l| {
                let pre = l.split(" - ").next().unwrap();
                display_width(pre)
            })
            .collect();
        for pair in sep_pos.windows(2) {
            assert_eq!(
                pair[0], pair[1],
                "Misaligned separator: '{}' vs '{}'",
                lines[0], lines[1]
            );
        }
    }

    // === build_render_buffer ===

    #[test]
    fn test_buffer_starts_with_clear_screen() {
        let items = vec![SelectItem {
            name: "a".into(),
            description: "d".into(),
            selected: true,
        }];
        let buf = build_render_buffer(&items, 0, 40);
        assert!(
            buf.starts_with("\x1B[2J\x1B[1;1H"),
            "Buffer must start with clear-screen + cursor-home escape"
        );
    }

    #[test]
    fn test_buffer_all_lines_end_with_crlf() {
        let items = vec![
            SelectItem {
                name: "a".into(),
                description: "desc a".into(),
                selected: true,
            },
            SelectItem {
                name: "b".into(),
                description: "desc b".into(),
                selected: false,
            },
        ];
        let buf = build_render_buffer(&items, 0, 40);

        let lf_count = buf.matches('\n').count();
        let crlf_count = buf.matches("\r\n").count();
        assert_eq!(
            lf_count, crlf_count,
            "Every \\n must be part of \\r\\n (no bare LF in raw mode)"
        );
        assert!(crlf_count > 0, "Buffer must contain at least one CRLF");
    }

    #[test]
    fn test_buffer_has_header() {
        let items = vec![SelectItem {
            name: "skill".into(),
            description: "desc".into(),
            selected: true,
        }];
        let buf = build_render_buffer(&items, 0, 40);
        assert!(buf.contains("Select skills to install (1/1):"));
        assert!(buf.contains("Arrows/j/k: navigate"));
    }

    #[test]
    fn test_buffer_header_shows_correct_counts() {
        let items = vec![
            SelectItem {
                name: "a".into(),
                description: "".into(),
                selected: true,
            },
            SelectItem {
                name: "b".into(),
                description: "".into(),
                selected: false,
            },
            SelectItem {
                name: "c".into(),
                description: "".into(),
                selected: true,
            },
        ];
        let buf = build_render_buffer(&items, 0, 40);
        assert!(buf.contains("Select skills to install (2/3):"));
    }

    #[test]
    fn test_buffer_contains_all_items() {
        let items = vec![
            SelectItem {
                name: "alpha".into(),
                description: "first".into(),
                selected: true,
            },
            SelectItem {
                name: "beta".into(),
                description: "second".into(),
                selected: true,
            },
            SelectItem {
                name: "gamma".into(),
                description: "third".into(),
                selected: false,
            },
        ];
        let buf = build_render_buffer(&items, 0, 40);

        for item in &items {
            assert!(
                buf.contains(&item.name),
                "Buffer must contain item name '{}'",
                item.name
            );
            assert!(
                buf.contains(&item.description),
                "Buffer must contain item description '{}'",
                item.description
            );
        }
    }

    #[test]
    fn test_buffer_focused_item_has_bold_codes() {
        let items = vec![
            SelectItem {
                name: "skill-alpha".into(),
                description: "first skill".into(),
                selected: true,
            },
            SelectItem {
                name: "skill-beta".into(),
                description: "second skill".into(),
                selected: true,
            },
            SelectItem {
                name: "skill-gamma".into(),
                description: "third skill".into(),
                selected: true,
            },
        ];
        // Cursor on item 1 (middle)
        let buf = build_render_buffer(&items, 1, 40);

        let bold_on = "\x1B[1m";
        let bold_off = "\x1B[0m";

        let bold_start = buf.find(bold_on).unwrap();
        let bold_end = buf.find(bold_off).unwrap();
        assert!(bold_end > bold_start, "Bold-off must appear after bold-on");

        // The slice between bold codes must be a single item line
        let between = &buf[bold_start + bold_on.len()..bold_end];

        // Must contain the focused item's name (full name, no substring risk)
        assert!(
            between.contains(&items[1].name),
            "Bold span must contain focused item name"
        );

        // Must start with focused marker ">"
        assert!(
            between.starts_with('>'),
            "Bold span must start with '>' marker, got: {:?}",
            &between[..between.len().min(5)]
        );

        // Bold span must be exactly one line — no CR/LF between codes
        assert!(
            !between.contains('\r'),
            "Bold span must not span multiple lines"
        );
        assert!(
            !between.contains('\n'),
            "Bold span must not span multiple lines"
        );
    }

    #[test]
    fn test_buffer_only_one_bold_pair() {
        let items: Vec<SelectItem> = (0..10)
            .map(|i| SelectItem {
                name: format!("skill-{}", i),
                description: format!("desc {}", i),
                selected: true,
            })
            .collect();
        let buf = build_render_buffer(&items, 5, 40);

        assert_eq!(
            buf.matches("\x1B[1m").count(),
            1,
            "Exactly one bold-on code"
        );
        assert_eq!(
            buf.matches("\x1B[0m").count(),
            1,
            "Exactly one bold-off code"
        );
    }

    #[test]
    fn test_buffer_footer_with_selection() {
        let items = vec![
            SelectItem {
                name: "a".into(),
                description: "d".into(),
                selected: true,
            },
            SelectItem {
                name: "b".into(),
                description: "d".into(),
                selected: true,
            },
        ];
        let buf = build_render_buffer(&items, 0, 40);
        assert!(buf.contains("Press Enter to install 2 skill(s)"));
        assert!(!buf.contains("Select at least one skill, or press q to cancel"));
    }

    #[test]
    fn test_buffer_footer_without_selection() {
        let items = vec![SelectItem {
            name: "a".into(),
            description: "d".into(),
            selected: false,
        }];
        let buf = build_render_buffer(&items, 0, 40);
        assert!(buf.contains("Select at least one skill, or press q to cancel"));
        assert!(!buf.contains("Press Enter to install"));
    }

    #[test]
    fn test_buffer_item_line_has_crlf_after_bold_reset() {
        let items = vec![
            SelectItem {
                name: "a".into(),
                description: "d".into(),
                selected: true,
            },
            SelectItem {
                name: "b".into(),
                description: "d".into(),
                selected: true,
            },
        ];
        // Focus first item
        let buf = build_render_buffer(&items, 0, 40);

        // After the bold-off code on the focused line, there must be \r\n
        let reset_pos = buf.find("\x1B[0m").unwrap();
        let after_reset = &buf[reset_pos..];
        assert!(
            after_reset.starts_with("\x1B[0m\r\n"),
            "Bold-off must be immediately followed by CRLF, got: {:?}",
            &after_reset[..after_reset.find('\n').unwrap_or(20) + 1]
        );
    }

    // === cursor navigation logic ===

    #[test]
    fn test_cursor_up_boundary() {
        let mut cursor = 0usize;
        cursor = cursor.saturating_sub(1);
        assert_eq!(cursor, 0);
    }

    #[test]
    fn test_cursor_down_boundary() {
        let len = 5usize;
        let mut cursor = 4usize;
        if cursor < len - 1 {
            cursor += 1;
        }
        assert_eq!(cursor, 4);
    }

    #[test]
    fn test_cursor_up_middle() {
        let mut cursor = 3usize;
        cursor = cursor.saturating_sub(1);
        assert_eq!(cursor, 2);
    }

    #[test]
    fn test_cursor_down_middle() {
        let len = 10usize;
        let mut cursor = 3usize;
        if cursor < len - 1 {
            cursor += 1;
        }
        assert_eq!(cursor, 4);
    }

    // === selection logic ===

    #[test]
    fn test_toggle_selection() {
        let mut items = [
            SelectItem {
                name: "a".into(),
                description: "".into(),
                selected: false,
            },
            SelectItem {
                name: "b".into(),
                description: "".into(),
                selected: true,
            },
        ];
        items[0].selected = !items[0].selected;
        items[1].selected = !items[1].selected;
        assert!(items[0].selected);
        assert!(!items[1].selected);
    }

    #[test]
    fn test_select_all_toggle() {
        let mut items = [
            SelectItem {
                name: "a".into(),
                description: "".into(),
                selected: false,
            },
            SelectItem {
                name: "b".into(),
                description: "".into(),
                selected: false,
            },
        ];
        let all_selected = items.iter().all(|i| i.selected);
        for item in items.iter_mut() {
            item.selected = !all_selected;
        }
        assert!(items.iter().all(|i| i.selected));

        let all_selected = items.iter().all(|i| i.selected);
        for item in items.iter_mut() {
            item.selected = !all_selected;
        }
        assert!(items.iter().all(|i| !i.selected));
    }

    #[test]
    fn test_select_none() {
        let mut items = [
            SelectItem {
                name: "a".into(),
                description: "".into(),
                selected: true,
            },
            SelectItem {
                name: "b".into(),
                description: "".into(),
                selected: true,
            },
        ];
        for item in items.iter_mut() {
            item.selected = false;
        }
        assert!(items.iter().all(|i| !i.selected));
    }

    #[test]
    fn test_any_selected_check() {
        let items = [
            SelectItem {
                name: "a".into(),
                description: "".into(),
                selected: false,
            },
            SelectItem {
                name: "b".into(),
                description: "".into(),
                selected: true,
            },
        ];
        assert!(items.iter().any(|i| i.selected));

        let items2 = [SelectItem {
            name: "a".into(),
            description: "".into(),
            selected: false,
        }];
        assert!(!items2.iter().any(|i| i.selected));
    }

    // === handle_key ===

    fn two_items() -> [SelectItem; 2] {
        [
            SelectItem {
                name: "a".into(),
                description: "".into(),
                selected: false,
            },
            SelectItem {
                name: "b".into(),
                description: "".into(),
                selected: true,
            },
        ]
    }

    #[test]
    fn test_handle_key_cancel() {
        let mut items = two_items();
        let mut state = TuiState::new(items.len(), 10);
        assert!(handle_key(key(KeyCode::Char('q')), &mut items, &mut state).is_some());
        assert!(handle_key(key(KeyCode::Esc), &mut items, &mut state).is_some());
    }

    #[test]
    fn test_handle_key_up_boundary() {
        let mut items = two_items();
        let mut state = TuiState::new(items.len(), 10);
        handle_key(key(KeyCode::Up), &mut items, &mut state);
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_handle_key_up_middle() {
        let mut items = two_items();
        let mut state = TuiState::new(items.len(), 10);
        state.cursor = 1;
        handle_key(key(KeyCode::Char('k')), &mut items, &mut state);
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_handle_key_down_boundary() {
        let mut items = two_items();
        let mut state = TuiState::new(items.len(), 10);
        state.cursor = 1;
        handle_key(key(KeyCode::Down), &mut items, &mut state);
        assert_eq!(state.cursor, 1);
    }

    #[test]
    fn test_handle_key_down_middle() {
        let mut items = two_items();
        let mut state = TuiState::new(items.len(), 10);
        handle_key(key(KeyCode::Char('j')), &mut items, &mut state);
        assert_eq!(state.cursor, 1);
    }

    #[test]
    fn test_handle_key_space_toggles() {
        let mut items = two_items();
        let mut state = TuiState::new(items.len(), 10);
        handle_key(key(KeyCode::Char(' ')), &mut items, &mut state);
        assert!(items[0].selected);
    }

    #[test]
    fn test_handle_key_a_toggles_all() {
        let mut items = two_items();
        let mut state = TuiState::new(items.len(), 10);
        handle_key(key(KeyCode::Char('a')), &mut items, &mut state);
        assert!(items.iter().all(|i| i.selected));
        handle_key(key(KeyCode::Char('A')), &mut items, &mut state);
        assert!(items.iter().all(|i| !i.selected));
    }

    #[test]
    fn test_handle_key_n_clears_all() {
        let mut items = two_items();
        let mut state = TuiState::new(items.len(), 10);
        handle_key(key(KeyCode::Char('n')), &mut items, &mut state);
        assert!(items.iter().all(|i| !i.selected));
    }

    #[test]
    fn test_handle_key_enter_confirms_when_selected() {
        let mut items = two_items();
        let mut state = TuiState::new(items.len(), 10);
        let result = handle_key(key(KeyCode::Enter), &mut items, &mut state);
        assert!(matches!(result, Some(Ok(()))));
    }

    #[test]
    fn test_handle_key_enter_noop_when_none_selected() {
        let mut items = two_items();
        let mut state = TuiState::new(items.len(), 10);
        handle_key(key(KeyCode::Char('n')), &mut items, &mut state);
        assert!(handle_key(key(KeyCode::Enter), &mut items, &mut state).is_none());
    }

    #[test]
    fn test_handle_key_unknown_noop() {
        let mut items = two_items();
        let mut state = TuiState::new(items.len(), 10);
        assert!(handle_key(key(KeyCode::Char('x')), &mut items, &mut state).is_none());
    }

    // === filter-mode keys ===

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn key_ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn three_items() -> [SelectItem; 3] {
        [
            make_item("azure-visualizer", "Azure graphs"),
            make_item("qdrant-search", "Qdrant relevance"),
            make_item("alpha", "first"),
        ]
    }

    #[test]
    fn test_slash_focuses_filter() {
        let mut items = three_items();
        let mut state = TuiState::new(items.len(), 10);
        let result = handle_key(key(KeyCode::Char('/')), &mut items, &mut state);
        assert!(result.is_none());
        assert!(state.filter.focused);
    }

    #[test]
    fn test_typing_q_in_filter_types_it_does_not_cancel() {
        let mut items = three_items();
        let mut state = TuiState::new(items.len(), 10);
        state.filter.focused = true;
        let result = handle_key(key(KeyCode::Char('q')), &mut items, &mut state);
        assert!(result.is_none());
        assert_eq!(state.filter.query, "q");
        assert_eq!(state.filtered, vec![1]);
    }

    #[test]
    fn test_typing_a_in_filter_types_it_does_not_toggle() {
        let mut items = three_items();
        let mut state = TuiState::new(items.len(), 10);
        state.filter.focused = true;
        handle_key(key(KeyCode::Char('a')), &mut items, &mut state);
        assert_eq!(state.filter.query, "a");
        assert!(items.iter().all(|i| !i.selected));
    }

    #[test]
    fn test_space_in_filter_types_space() {
        let mut items = three_items();
        let mut state = TuiState::new(items.len(), 10);
        state.filter.focused = true;
        for c in "nt se".chars() {
            handle_key(key(KeyCode::Char(c)), &mut items, &mut state);
        }
        assert_eq!(state.filter.query, "nt se");
    }

    #[test]
    fn test_backspace_edits_query_and_recomputes() {
        let mut items = three_items();
        let mut state = TuiState::new(items.len(), 10);
        state.filter.focused = true;
        state.filter.query = "qdr".to_string();
        state.refilter(&items);
        handle_key(key(KeyCode::Backspace), &mut items, &mut state);
        assert_eq!(state.filter.query, "qd");
        assert_eq!(state.filtered, vec![1]);
    }

    #[test]
    fn test_ctrl_u_clears_query() {
        let mut items = three_items();
        let mut state = TuiState::new(items.len(), 10);
        state.filter.focused = true;
        state.filter.query = "qdr".to_string();
        state.refilter(&items);
        handle_key(key_ctrl('u'), &mut items, &mut state);
        assert!(state.filter.query.is_empty());
        assert_eq!(state.filtered, vec![0, 1, 2]);
    }

    #[test]
    fn test_esc_in_filter_unfocuses_and_keeps_query() {
        let mut items = three_items();
        let mut state = TuiState::new(items.len(), 10);
        state.filter.focused = true;
        state.filter.query = "q".to_string();
        let result = handle_key(key(KeyCode::Esc), &mut items, &mut state);
        assert!(result.is_none());
        assert!(!state.filter.focused);
        assert_eq!(state.filter.query, "q");
    }

    #[test]
    fn test_esc_unfocused_cancels() {
        let mut items = three_items();
        let mut state = TuiState::new(items.len(), 10);
        let result = handle_key(key(KeyCode::Esc), &mut items, &mut state);
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    #[test]
    fn test_a_toggles_only_filtered() {
        let mut items = three_items();
        let mut state = TuiState::new(items.len(), 10);
        state.filter.query = "azure".to_string();
        state.refilter(&items);
        handle_key(key(KeyCode::Char('a')), &mut items, &mut state);
        assert!(items[0].selected);
        assert!(!items[1].selected);
        assert!(!items[2].selected);
    }

    #[test]
    fn test_n_clears_only_filtered() {
        let mut items = three_items();
        items[0].selected = true;
        items[2].selected = true;
        let mut state = TuiState::new(items.len(), 10);
        state.filter.query = "azure".to_string();
        state.refilter(&items);
        handle_key(key(KeyCode::Char('n')), &mut items, &mut state);
        assert!(!items[0].selected);
        assert!(items[2].selected);
    }

    #[test]
    fn test_arrows_move_in_filter_mode() {
        let mut items = three_items();
        let mut state = TuiState::new(items.len(), 10);
        state.filter.focused = true;
        handle_key(key(KeyCode::Down), &mut items, &mut state);
        assert_eq!(state.cursor, 1);
        handle_key(key(KeyCode::Up), &mut items, &mut state);
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_keys_noop_on_empty_filter() {
        let mut items = three_items();
        let mut state = TuiState::new(items.len(), 10);
        state.filter.focused = true;
        state.filter.query = "zzz".to_string();
        state.refilter(&items);
        assert!(state.filtered.is_empty());
        assert!(handle_key(key(KeyCode::Down), &mut items, &mut state).is_none());
        assert!(handle_key(key(KeyCode::Char(' ')), &mut items, &mut state).is_none());
        assert!(handle_key(key(KeyCode::Enter), &mut items, &mut state).is_none());
        assert_eq!(state.cursor, 0);
        assert!(items.iter().all(|i| !i.selected));
    }

    // === substring filter ===

    fn make_item(name: &str, desc: &str) -> SelectItem {
        SelectItem {
            name: name.to_string(),
            description: desc.to_string(),
            selected: false,
        }
    }

    #[test]
    fn test_apply_filter_empty_query_returns_all() {
        let items = vec![make_item("alpha", "first"), make_item("beta", "second")];
        assert_eq!(apply_filter(&items, ""), vec![0, 1]);
    }

    #[test]
    fn test_apply_filter_matches_name_case_insensitive() {
        let items = vec![
            make_item("azure-visualizer", "x"),
            make_item("qdrant-search", "y"),
        ];
        assert_eq!(apply_filter(&items, "AZURE"), vec![0]);
        assert_eq!(apply_filter(&items, "qdrant"), vec![1]);
    }

    #[test]
    fn test_apply_filter_matches_description() {
        let items = vec![
            make_item("a", "Prompt optimization guide"),
            make_item("b", "other"),
        ];
        assert_eq!(apply_filter(&items, "prompt opt"), vec![0]);
    }

    #[test]
    fn test_apply_filter_no_match_returns_empty() {
        let items = vec![make_item("alpha", "first")];
        assert!(apply_filter(&items, "zzz").is_empty());
    }

    #[test]
    fn test_apply_filter_does_not_touch_selection() {
        let mut items = vec![make_item("alpha", "first"), make_item("beta", "second")];
        items[0].selected = true;
        let _ = apply_filter(&items, "beta");
        assert!(items[0].selected);
        assert!(!items[1].selected);
    }

    // === viewport geometry ===

    #[test]
    fn test_clamp_view_offset_keeps_cursor_visible_down() {
        // 100 items, height 10, cursor moved to 10 while offset 0 → offset becomes 1.
        assert_eq!(clamp_view_offset(10, 0, 10, 100), 1);
    }

    #[test]
    fn test_clamp_view_offset_keeps_cursor_visible_up() {
        assert_eq!(clamp_view_offset(3, 5, 10, 100), 3);
    }

    #[test]
    fn test_clamp_view_offset_never_negative_scroll() {
        assert_eq!(clamp_view_offset(0, 0, 10, 100), 0);
    }

    #[test]
    fn test_clamp_view_offset_clamps_at_end() {
        // Last item visible without overscrolling: 100 - 10 = 90.
        assert_eq!(clamp_view_offset(99, 0, 10, 100), 90);
    }

    #[test]
    fn test_clamp_view_offset_short_list() {
        assert_eq!(clamp_view_offset(2, 0, 10, 3), 0);
    }

    #[test]
    fn test_clamp_view_offset_empty() {
        assert_eq!(clamp_view_offset(0, 0, 10, 0), 0);
    }

    #[test]
    fn test_page_step_down_and_up() {
        assert_eq!(page_step(5, 100, 10, true), 15);
        assert_eq!(page_step(15, 100, 10, false), 5);
    }

    #[test]
    fn test_page_step_clamps_at_bounds() {
        assert_eq!(page_step(95, 100, 10, true), 99);
        assert_eq!(page_step(5, 100, 10, false), 0);
        assert_eq!(page_step(0, 0, 10, true), 0);
    }

    #[test]
    fn test_view_height_subtracts_chrome() {
        assert_eq!(view_height(24), 24 - CHROME_ROWS as usize);
        assert_eq!(view_height(5), MIN_VIEW_HEIGHT);
    }

    #[test]
    fn test_state_new_covers_all_unfocused() {
        let state = TuiState::new(3, 10);
        assert_eq!(state.filtered, vec![0, 1, 2]);
        assert_eq!(state.cursor, 0);
        assert_eq!(state.offset, 0);
        assert_eq!(state.height, 10);
        assert!(!state.filter.focused);
        assert!(state.filter.query.is_empty());
    }

    #[test]
    fn test_goto_moves_and_follows_with_viewport() {
        let mut state = TuiState::new(100, 10);
        state.goto(50);
        assert_eq!(state.cursor, 50);
        assert_eq!(state.offset, 41);
        state.goto(0);
        assert_eq!(state.cursor, 0);
        assert_eq!(state.offset, 0);
    }

    #[test]
    fn test_goto_clamps_past_end() {
        let mut state = TuiState::new(3, 10);
        state.goto(usize::MAX);
        assert_eq!(state.cursor, 2);
        assert_eq!(state.offset, 0);
    }

    #[test]
    fn test_goto_empty_is_safe() {
        let mut state = TuiState::new(0, 10);
        state.goto(5);
        assert_eq!(state.cursor, 0);
        assert_eq!(state.offset, 0);
    }

    #[test]
    fn test_refilter_recomputes_and_resets() {
        let items = vec![make_item("azure-x", "d"), make_item("other", "d")];
        let mut state = TuiState::new(2, 10);
        state.cursor = 1;
        state.filter.query = "azure".to_string();
        state.refilter(&items);
        assert_eq!(state.filtered, vec![0]);
        assert_eq!(state.cursor, 0);
        assert_eq!(state.offset, 0);
    }
}
