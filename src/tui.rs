use anyhow::{Result, bail};
use crossterm::{
    Command,
    cursor::MoveTo,
    event::{self, Event, KeyCode, KeyEventKind},
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

pub use crate::style::truncate_with_ellipsis as truncate_str;
const DEFAULT_NAME_WIDTH: usize = 20;
const MAX_NAME_WIDTH: usize = 30;
const NUM_COL_WIDTH: usize = 2;
const DEFAULT_COLS: u16 = 80;
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

pub fn format_item(
    i: usize,
    item: &SelectItem,
    name_width: usize,
    desc_width: usize,
    focused: bool,
) -> String {
    let checkbox = if item.selected { "[x]" } else { "[ ]" };
    let num = format!("{:>width$}.", i + 1, width = NUM_COL_WIDTH);
    let name = truncate_str(&item.name, name_width);
    let desc = truncate_str(&item.description, desc_width);
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
    let mut cursor = 0usize;

    let (cols, _) = size().unwrap_or((DEFAULT_COLS, DEFAULT_ROWS));
    let name_width = calc_name_width(items);
    let desc_width = (cols as usize)
        .saturating_sub(MENU_PREFIX_WIDTH)
        .saturating_sub(name_width)
        .clamp(MIN_DESC_WIDTH, MAX_DESC_WIDTH);

    (|| -> Result<()> {
        loop {
            render_all(items, cursor, desc_width, &mut stdout)?;

            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Release {
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        bail!("Selection cancelled");
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        cursor = cursor.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if cursor < items.len() - 1 {
                            cursor += 1;
                        }
                    }
                    KeyCode::Char(' ') => {
                        items[cursor].selected = !items[cursor].selected;
                    }
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        let all_selected = items.iter().all(|i| i.selected);
                        for item in items.iter_mut() {
                            item.selected = !all_selected;
                        }
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') => {
                        for item in items.iter_mut() {
                            item.selected = false;
                        }
                    }
                    KeyCode::Enter if items.iter().any(|i| i.selected) => {
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
    })()
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

    // === key event kind filtering ===

    fn simulate_key_handler(
        kind: KeyEventKind,
        code: KeyCode,
        items: &mut [SelectItem],
        cursor: &mut usize,
    ) -> Option<Result<()>> {
        if kind == KeyEventKind::Release {
            return Some(Ok(()));
        }
        match code {
            KeyCode::Char('q') | KeyCode::Esc => Some(Err(anyhow::anyhow!("Selection cancelled"))),
            KeyCode::Up | KeyCode::Char('k') => {
                if *cursor > 0 {
                    *cursor -= 1;
                }
                None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if *cursor < items.len() - 1 {
                    *cursor += 1;
                }
                None
            }
            KeyCode::Char(' ') => {
                items[*cursor].selected = !items[*cursor].selected;
                None
            }
            KeyCode::Enter => {
                if items.iter().any(|i| i.selected) {
                    Some(Ok(()))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    #[test]
    fn test_repeat_event_moves_cursor_down() {
        let mut items = vec![
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
            SelectItem {
                name: "c".into(),
                description: "".into(),
                selected: false,
            },
        ];
        let mut cursor = 0usize;

        // Press then Repeat should both move cursor
        simulate_key_handler(KeyEventKind::Press, KeyCode::Down, &mut items, &mut cursor);
        simulate_key_handler(KeyEventKind::Repeat, KeyCode::Down, &mut items, &mut cursor);
        simulate_key_handler(KeyEventKind::Repeat, KeyCode::Down, &mut items, &mut cursor);
        assert_eq!(cursor, 2);
    }

    #[test]
    fn test_repeat_event_moves_cursor_up() {
        let mut items = vec![
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
            SelectItem {
                name: "c".into(),
                description: "".into(),
                selected: false,
            },
        ];
        let mut cursor = 2usize;

        simulate_key_handler(KeyEventKind::Press, KeyCode::Up, &mut items, &mut cursor);
        simulate_key_handler(KeyEventKind::Repeat, KeyCode::Up, &mut items, &mut cursor);
        assert_eq!(cursor, 0);
    }

    #[test]
    fn test_repeat_event_toggles_space() {
        let mut items = vec![SelectItem {
            name: "a".into(),
            description: "".into(),
            selected: false,
        }];
        let mut cursor = 0usize;

        // Press toggles on, Repeat toggles off, Repeat toggles on again
        simulate_key_handler(
            KeyEventKind::Press,
            KeyCode::Char(' '),
            &mut items,
            &mut cursor,
        );
        assert!(items[0].selected);
        simulate_key_handler(
            KeyEventKind::Repeat,
            KeyCode::Char(' '),
            &mut items,
            &mut cursor,
        );
        assert!(!items[0].selected);
        simulate_key_handler(
            KeyEventKind::Repeat,
            KeyCode::Char(' '),
            &mut items,
            &mut cursor,
        );
        assert!(items[0].selected);
    }

    #[test]
    fn test_release_event_ignored() {
        let mut items = vec![SelectItem {
            name: "a".into(),
            description: "".into(),
            selected: false,
        }];
        let mut cursor = 0usize;

        // Release events should be ignored
        let result = simulate_key_handler(
            KeyEventKind::Release,
            KeyCode::Down,
            &mut items,
            &mut cursor,
        );
        assert!(result.is_some());
        assert_eq!(cursor, 0);
        assert!(!items[0].selected);
    }

    #[test]
    fn test_j_k_navigation_with_repeat() {
        let mut items = vec![
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
            SelectItem {
                name: "c".into(),
                description: "".into(),
                selected: false,
            },
        ];
        let mut cursor = 0usize;

        // j and k should work with both Press and Repeat
        simulate_key_handler(
            KeyEventKind::Repeat,
            KeyCode::Char('j'),
            &mut items,
            &mut cursor,
        );
        assert_eq!(cursor, 1);
        simulate_key_handler(
            KeyEventKind::Repeat,
            KeyCode::Char('j'),
            &mut items,
            &mut cursor,
        );
        assert_eq!(cursor, 2);
        simulate_key_handler(
            KeyEventKind::Repeat,
            KeyCode::Char('k'),
            &mut items,
            &mut cursor,
        );
        assert_eq!(cursor, 1);
    }

    #[test]
    fn test_enter_with_repeat() {
        let mut items = vec![SelectItem {
            name: "a".into(),
            description: "".into(),
            selected: true,
        }];
        let mut cursor = 0usize;

        // Enter with Repeat kind should also confirm
        let result = simulate_key_handler(
            KeyEventKind::Repeat,
            KeyCode::Enter,
            &mut items,
            &mut cursor,
        );
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
    }
}
