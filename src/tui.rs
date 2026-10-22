use crate::style::display_width;
use anyhow::{Context, Result, bail};
use crossterm::{
    Command,
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    style::{Attribute, SetAttribute},
    terminal::{
        Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode, size,
    },
};
use std::io::{self, IsTerminal, Write};

const DEFAULT_NAME_WIDTH: usize = 20;
const MAX_NAME_WIDTH: usize = 30;
const NUM_COL_WIDTH: usize = 3;
const MENU_FALLBACK_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;
const CHROME_ROWS: u16 = 10;
const MAX_QUERY_DISPLAY: usize = 40;
const MIN_COLS: u16 = 24;
const DETAIL_CHROME: usize = 6;
const WIDE_COLS: usize = 72;
const ROW_PREFIX: usize = 6;

pub struct SelectItem {
    pub name: String,
    pub description: String,
    pub selected: bool,
}

#[derive(Clone, Default, Debug)]
pub struct ListItem {
    pub name: String,
    pub description: String,
    pub source: String,
    pub version: String,
    pub status: String,
    pub selected: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ListMode {
    Browse,
    Select,
    Review,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ListOutcome {
    Accepted,
    Cancelled,
    Back,
}

trait Entry {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn selected(&self) -> bool;
    fn set_selected(&mut self, selected: bool);
    fn source(&self) -> &str {
        ""
    }
    fn version(&self) -> &str {
        ""
    }
    fn status(&self) -> &str {
        ""
    }
}
impl Entry for SelectItem {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn selected(&self) -> bool {
        self.selected
    }
    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}
impl Entry for ListItem {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn selected(&self) -> bool {
        self.selected
    }
    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
    fn source(&self) -> &str {
        &self.source
    }
    fn version(&self) -> &str {
        &self.version
    }
    fn status(&self) -> &str {
        &self.status
    }
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

fn apply_filter<T: Entry>(items: &[T], query: &str) -> Vec<usize> {
    let query = query.to_lowercase();
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            let fields = [
                item.name(),
                item.description(),
                item.source(),
                item.version(),
                item.status(),
            ]
            .map(str::to_lowercase);
            query
                .split_whitespace()
                .all(|word| fields.iter().any(|field| field.contains(word)))
        })
        .map(|(index, _)| index)
        .collect()
}

struct TuiState {
    filtered: Vec<usize>,
    cursor: usize,
    offset: usize,
    height: usize,
    filter: FilterState,
    selected_only: bool,
    mode: ListMode,
    title: String,
    cols: u16,
    rows: u16,
    details: bool,
    detail_offset: usize,
    back: bool,
}
impl TuiState {
    fn new(item_count: usize, height: usize) -> Self {
        Self {
            filtered: (0..item_count).collect(),
            cursor: 0,
            offset: 0,
            height,
            filter: FilterState::default(),
            selected_only: false,
            mode: ListMode::Select,
            title: "Select skills to install".into(),
            cols: MENU_FALLBACK_COLS,
            rows: (height as u16).saturating_add(CHROME_ROWS),
            details: false,
            detail_offset: 0,
            back: false,
        }
    }
    fn refilter<T: Entry>(&mut self, items: &[T]) {
        self.filtered = apply_filter(items, &self.filter.query)
            .into_iter()
            .filter(|&i| !self.selected_only || items[i].selected())
            .collect();
        self.filtered.sort_by(|&a, &b| {
            items[a]
                .source()
                .cmp(items[b].source())
                .then_with(|| items[a].name().cmp(items[b].name()))
        });
        self.cursor = 0;
        self.offset = 0;
        self.detail_offset = 0;
    }
    fn goto(&mut self, pos: usize) {
        self.cursor = pos.min(self.filtered.len().saturating_sub(1));
        self.offset = clamp_view_offset(self.cursor, self.offset, self.height, self.filtered.len());
        self.detail_offset = 0;
    }
    fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
        self.height = view_height(rows);
        self.goto(self.cursor);
    }
}
fn clamp_view_offset(cursor: usize, offset: usize, height: usize, total: usize) -> usize {
    if total == 0 || height == 0 {
        return 0;
    }
    let mut next = offset.min(total.saturating_sub(height));
    if cursor < next {
        next = cursor;
    }
    if cursor >= next + height {
        next = cursor + 1 - height;
    }
    next.min(total.saturating_sub(height))
}
fn page_step(cursor: usize, total: usize, height: usize, down: bool) -> usize {
    if total == 0 {
        return 0;
    }
    if down {
        cursor.saturating_add(height).min(total - 1)
    } else {
        cursor.saturating_sub(height)
    }
}
pub fn list_capacity(rows: u16) -> usize {
    view_height(rows)
}
fn view_height(term_rows: u16) -> usize {
    term_rows.saturating_sub(CHROME_ROWS) as usize
}

/// Keep the legacy row formatter for callers; the interactive renderer adapts to its viewport.
pub fn format_item(
    i: usize,
    item: &SelectItem,
    name_width: usize,
    desc_width: usize,
    focused: bool,
) -> String {
    let checkbox = if item.selected { "[x]" } else { "[ ]" };
    let num = format!("{:>width$}.", i + 1, width = NUM_COL_WIDTH);
    let name = clip(&item.name, name_width);
    let desc = clip(&item.description, desc_width);
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

const HINTS_UNFOCUSED: &str =
    "Arrows/j/k: navigate | PgUp/PgDn | Home/End | /: filter | d: details";
const HINTS_FOCUSED: &str = "Type to search | Ctrl-U: clear | Esc/Enter: return to list";

fn clean(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
                ' '
            } else {
                c
            }
        })
        .collect()
}
fn clip(text: &str, width: usize) -> String {
    crate::style::truncate_with_ellipsis(&clean(text), width)
}
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for c in clean(text).chars() {
        let previous_end = line.len();
        line.push(c);
        if display_width(&line) > width && previous_end > 0 {
            line.truncate(previous_end);
            lines.push(std::mem::take(&mut line));
            line.push(c);
        }
    }
    lines.push(line);
    lines
}
fn detail_text<T: Entry>(item: &T) -> String {
    format!(
        "Name: {}  Source: {}  Version: {}  Status: {}  Description: {}",
        item.name(),
        item.source(),
        item.version(),
        item.status(),
        item.description()
    )
}

fn build_render_buffer<T: Entry>(items: &[T], state: &TuiState, _desc_width: usize) -> String {
    let width = usize::from(state.cols.saturating_sub(1));
    let mut buf = String::new();
    Clear(ClearType::All)
        .write_ansi(&mut buf)
        .expect("String write");
    MoveTo(0, 0).write_ansi(&mut buf).expect("String write");
    if state.rows <= CHROME_ROWS || state.cols < MIN_COLS {
        buf.push_str(&clip("Resize terminal; Ctrl-C to cancel", width));
        return buf;
    }
    let selected = items.iter().filter(|i| i.selected()).count();
    let visible_selected = state
        .filtered
        .iter()
        .filter(|&&i| items[i].selected())
        .count();
    let mut lines = Vec::new();
    lines.push(if state.mode == ListMode::Select {
        format!("{} ({}/{}):", state.title, selected, items.len())
    } else {
        format!("{} ({} skills)", state.title, items.len())
    });
    lines.push(if state.mode == ListMode::Select {
        format!(
            "{}/{} | {} selected | {} hidden{}",
            state.filtered.len(),
            items.len(),
            selected,
            selected - visible_selected,
            if state.selected_only {
                " | selected only"
            } else {
                ""
            }
        )
    } else {
        format!("{} matches | {} total", state.filtered.len(), items.len())
    });
    lines.push(if state.filter.focused {
        HINTS_FOCUSED.into()
    } else if width < WIDE_COLS {
        "jk/Up/Dn PgUp/Dn /search d/info".into()
    } else {
        HINTS_UNFOCUSED.into()
    });
    lines.push(match state.mode {
        ListMode::Browse => "Enter/q/Esc: close".into(),
        ListMode::Select if width < WIDE_COLS => "Space toggle a/n all/none Tab selected".into(),
        ListMode::Select => {
            "Space: toggle | a/n: matches all/none | Tab: selected | Enter: next".into()
        }
        ListMode::Review if width < WIDE_COLS => "y remove ALL | Enter/q cancel | b back".into(),
        ListMode::Review => "y: remove ALL reviewed skills | Enter/q: cancel | b: back".into(),
    });
    lines.push(format!(
        "Filter: {}{} [{}/{} shown]",
        clip(
            &state.filter.query,
            MAX_QUERY_DISPLAY.min(width.saturating_sub(ROW_PREFIX))
        ),
        if state.filter.focused { "█" } else { "" },
        state.filtered.len(),
        items.len()
    ));
    if state.details {
        let available = usize::from(state.rows).saturating_sub(DETAIL_CHROME);
        let content = state
            .filtered
            .get(state.cursor)
            .map(|&i| wrap(&detail_text(&items[i]), width))
            .unwrap_or_else(|| vec!["No focused skill".into()]);
        let offset = state
            .detail_offset
            .min(content.len().saturating_sub(available));
        lines.extend(content.iter().skip(offset).take(available).cloned());
        while lines.len() < usize::from(state.rows).saturating_sub(1) {
            lines.push(String::new());
        }
        lines.push(if width < WIDE_COLS {
            format!("Details {}/{} | Esc: back", offset + 1, content.len())
        } else {
            format!(
                "Details {}/{} | arrows/PgUp/PgDn/Home/End | Esc: back",
                offset + 1,
                content.len()
            )
        });
    } else {
        for position in state.offset..state.offset + state.height {
            let row = if let Some(&i) = state.filtered.get(position) {
                let item = &items[i];
                let marker = if position == state.cursor { ">" } else { " " };
                let check = if state.mode == ListMode::Select {
                    if item.selected() { "[x] " } else { "[ ] " }
                } else {
                    ""
                };
                let name_width = if width >= WIDE_COLS {
                    MAX_NAME_WIDTH.min(width / 2)
                } else {
                    width.saturating_sub(ROW_PREFIX)
                };
                let name = clip(item.name(), name_width);
                if width >= WIDE_COLS {
                    let pad = name_width.saturating_sub(display_width(&name));
                    format!(
                        "{marker} {check}{name}{}  {} | {}",
                        " ".repeat(pad),
                        item.status(),
                        if item.source().is_empty() {
                            item.description()
                        } else {
                            item.source()
                        }
                    )
                } else {
                    format!("{marker} {check}{name}")
                }
            } else if position == state.offset && state.filtered.is_empty() {
                if state.selected_only && state.filter.query.is_empty() {
                    "No skills selected. Tab: all skills".into()
                } else {
                    format!(
                        "No matches for \"{}\"; / then Ctrl-U: clear",
                        state.filter.query
                    )
                }
            } else {
                String::new()
            };
            lines.push(row);
        }
        let end = (state.offset + state.height).min(state.filtered.len());
        let mut range = format!(
            "{}-{} of {}",
            if end == 0 { 0 } else { state.offset + 1 },
            end,
            state.filtered.len()
        );
        if state.offset > 0 {
            range.push_str(&format!(" | {} more above", state.offset));
        }
        if end < state.filtered.len() {
            range.push_str(&format!(" | {} more below", state.filtered.len() - end));
        }
        lines.push(range);
        if let Some(&i) = state.filtered.get(state.cursor) {
            let item = &items[i];
            lines.push(format!("{}  [d: full details]", item.name()));
            lines.push(format!(
                "{} | version {} | {}",
                item.source(),
                item.version(),
                item.status()
            ));
            lines.push(item.description().into());
        } else {
            lines.extend([String::new(), String::new(), String::new()]);
        }
        lines.push(match state.mode {
            ListMode::Select if selected == 0 => {
                "Select at least one skill, or press q to cancel".into()
            }
            ListMode::Select => format!(
                "Press Enter to {} {} skill(s)",
                if state.title == "Select skills to install" {
                    "install"
                } else {
                    "continue with"
                },
                selected
            ),
            ListMode::Browse => "Browse skills | /: search | d: full details | q: close".into(),
            ListMode::Review => format!(
                "Remove {} reviewed skill(s)? y confirms; default: No",
                items.len()
            ),
        });
    }
    for (i, line) in lines.iter().take(usize::from(state.rows)).enumerate() {
        if i > 0 {
            buf.push_str("\r\n");
        }
        let focused_row = !state.details
            && i == 5 + state.cursor.saturating_sub(state.offset)
            && state.filtered.get(state.cursor).is_some();
        if focused_row {
            SetAttribute(Attribute::Bold)
                .write_ansi(&mut buf)
                .expect("String write");
        }
        buf.push_str(&clip(line, width));
        if focused_row {
            SetAttribute(Attribute::Reset)
                .write_ansi(&mut buf)
                .expect("String write");
        }
    }
    buf
}

fn apply_navigation(code: KeyCode, state: &mut TuiState) -> bool {
    match code {
        KeyCode::Up => state.goto(state.cursor.saturating_sub(1)),
        KeyCode::Down => state.goto(state.cursor.saturating_add(1)),
        KeyCode::PageUp => state.goto(page_step(
            state.cursor,
            state.filtered.len(),
            state.height,
            false,
        )),
        KeyCode::PageDown => state.goto(page_step(
            state.cursor,
            state.filtered.len(),
            state.height,
            true,
        )),
        KeyCode::Home => state.goto(0),
        KeyCode::End => state.goto(usize::MAX),
        _ => return false,
    };
    true
}
fn handle_key<T: Entry>(
    event: KeyEvent,
    items: &mut [T],
    state: &mut TuiState,
) -> Option<Result<()>> {
    if event.code == KeyCode::Char('c') && event.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(Err(anyhow::anyhow!("Selection cancelled")));
    }
    if state.cols < MIN_COLS || state.rows <= CHROME_ROWS {
        return match event.code {
            KeyCode::Esc | KeyCode::Char('q') => Some(Err(anyhow::anyhow!("Selection cancelled"))),
            _ => None,
        };
    }
    if state.details {
        match event.code {
            KeyCode::Esc | KeyCode::Char('q' | 'd') => state.details = false,
            KeyCode::Down | KeyCode::Char('j') => {
                state.detail_offset = state.detail_offset.saturating_add(1)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                state.detail_offset = state.detail_offset.saturating_sub(1)
            }
            KeyCode::PageDown => {
                state.detail_offset = state.detail_offset.saturating_add(state.height)
            }
            KeyCode::PageUp => {
                state.detail_offset = state.detail_offset.saturating_sub(state.height)
            }
            KeyCode::Home => state.detail_offset = 0,
            KeyCode::End => state.detail_offset = usize::MAX,
            _ => (),
        }
        if let Some(&i) = state.filtered.get(state.cursor) {
            let count = wrap(
                &detail_text(&items[i]),
                usize::from(state.cols.saturating_sub(1)),
            )
            .len();
            state.detail_offset = state
                .detail_offset
                .min(count.saturating_sub(usize::from(state.rows).saturating_sub(DETAIL_CHROME)));
        }
        return None;
    }
    if state.filter.focused {
        return handle_filter_key(event, items, state);
    }
    if event
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return None;
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
        KeyCode::Char('d') => {
            state.details = true;
            state.detail_offset = 0;
            None
        }
        KeyCode::Char('k') => {
            state.goto(state.cursor.saturating_sub(1));
            None
        }
        KeyCode::Char('j') => {
            state.goto(state.cursor.saturating_add(1));
            None
        }
        KeyCode::Tab if state.mode == ListMode::Select => {
            state.selected_only = !state.selected_only;
            state.refilter(items);
            None
        }
        KeyCode::Char(' ') if state.mode == ListMode::Select => {
            if let Some(&i) = state.filtered.get(state.cursor) {
                items[i].set_selected(!items[i].selected());
            }
            if state.selected_only {
                state.refilter(items);
            }
            None
        }
        KeyCode::Char('a' | 'A' | 'n' | 'N') if state.mode == ListMode::Select => {
            let select = matches!(event.code, KeyCode::Char('a' | 'A'));
            for &i in &state.filtered {
                items[i].set_selected(select);
            }
            if state.selected_only {
                state.refilter(items);
            }
            None
        }
        KeyCode::Char('b') if state.mode == ListMode::Review => {
            state.back = true;
            Some(Ok(()))
        }
        KeyCode::Char('y') if state.mode == ListMode::Review => Some(Ok(())),
        KeyCode::Enter if state.mode == ListMode::Review => {
            Some(Err(anyhow::anyhow!("Removal cancelled")))
        }
        KeyCode::Enter if state.mode == ListMode::Browse || items.iter().any(Entry::selected) => {
            Some(Ok(()))
        }
        _ => None,
    }
}
fn handle_filter_key<T: Entry>(
    event: KeyEvent,
    items: &mut [T],
    state: &mut TuiState,
) -> Option<Result<()>> {
    if apply_navigation(event.code, state) {
        return None;
    }
    match event.code {
        KeyCode::Esc | KeyCode::Enter => state.filter.focused = false,
        KeyCode::Backspace => {
            state.filter.query.pop();
            state.refilter(items);
        }
        KeyCode::Char('u') if event.modifiers.contains(KeyModifiers::CONTROL) => {
            state.filter.query.clear();
            state.refilter(items);
        }
        KeyCode::Char(c)
            if !event
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            state.filter.query.push(c);
            state.refilter(items);
        }
        _ => (),
    };
    None
}

pub fn interactive_available() -> bool {
    io::stdin().is_terminal()
        && io::stderr().is_terminal()
        && std::env::var("TERM").is_ok_and(|t| t != "dumb")
}
pub fn automatic_available() -> bool {
    interactive_available() && io::stdout().is_terminal()
}
pub fn require_interactive() -> Result<()> {
    if !interactive_available() {
        bail!(
            "Interactive selection requires a terminal on stdin/stderr and TERM other than dumb; provide an explicit skill name instead"
        );
    }
    Ok(())
}
struct TerminalSession;
impl TerminalSession {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("Failed to enable terminal raw mode")?;
        let guard = Self;
        crossterm::execute!(io::stderr(), EnterAlternateScreen, Hide)
            .context("Failed to enter skill browser")?;
        Ok(guard)
    }
}
impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = crossterm::execute!(
            io::stderr(),
            SetAttribute(Attribute::Reset),
            Show,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
    }
}
fn run_loop<T: Entry>(title: &str, items: &mut [T], mode: ListMode) -> Result<ListOutcome> {
    require_interactive()?;
    let _session = TerminalSession::enter()?;
    let (cols, rows) = size().unwrap_or((MENU_FALLBACK_COLS, DEFAULT_ROWS));
    let mut state = TuiState::new(items.len(), view_height(rows));
    state.resize(cols, rows);
    state.title = title.into();
    state.mode = mode;
    state.filtered.sort_by(|&a, &b| {
        items[a]
            .source()
            .cmp(items[b].source())
            .then_with(|| items[a].name().cmp(items[b].name()))
    });
    let mut stderr = io::stderr();
    loop {
        write!(stderr, "{}", build_render_buffer(items, &state, 0))
            .context("Failed to draw skill browser")?;
        stderr.flush().context("Failed to flush skill browser")?;
        match event::read().context("Failed to read terminal event")? {
            Event::Resize(cols, rows) => state.resize(cols, rows),
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if let Some(result) = handle_key(key, items, &mut state) {
                    return Ok(if result.is_err() {
                        ListOutcome::Cancelled
                    } else if state.back {
                        ListOutcome::Back
                    } else {
                        ListOutcome::Accepted
                    });
                }
            }
            _ => (),
        }
    }
}
pub fn run_list(title: &str, items: &mut [ListItem], mode: ListMode) -> Result<ListOutcome> {
    run_loop(title, items, mode)
}
pub fn multi_select(items: &mut [SelectItem]) -> Result<()> {
    if items.is_empty() {
        bail!("No items to select");
    }
    if run_loop("Select skills to install", items, ListMode::Select)? != ListOutcome::Accepted {
        bail!("Selection cancelled");
    }
    Ok(())
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
            .saturating_sub(14)
            .saturating_sub(name_width)
            .clamp(15, 60);

        let lines: Vec<String> = items
            .iter()
            .enumerate()
            .map(|(i, item)| format_item(i, item, name_width, desc_width, i == 0))
            .collect();

        let widths: Vec<usize> = lines.iter().map(|l| display_width(l)).collect();
        let expected = 14 + name_width + desc_width;
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

    fn render_at(items: &[SelectItem], cursor: usize) -> String {
        let mut state = TuiState::new(items.len(), items.len().max(1));
        state.cursor = cursor;
        build_render_buffer(items, &state, 40)
    }

    #[test]
    fn test_buffer_starts_with_clear_screen() {
        let items = vec![SelectItem {
            name: "a".into(),
            description: "d".into(),
            selected: true,
        }];
        let buf = render_at(&items, 0);
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
        let buf = render_at(&items, 0);

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
        let buf = render_at(&items, 0);
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
        let buf = render_at(&items, 0);
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
        let buf = render_at(&items, 0);

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
        let buf = render_at(&items, 1);

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
        let buf = render_at(&items, 5);

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
        let buf = render_at(&items, 0);
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
        let buf = render_at(&items, 0);
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
        let buf = render_at(&items, 0);

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
    fn test_handle_key_a_selects_all_and_n_clears() {
        let mut items = two_items();
        let mut state = TuiState::new(items.len(), 10);
        handle_key(key(KeyCode::Char('a')), &mut items, &mut state);
        assert!(items.iter().all(|i| i.selected));
        handle_key(key(KeyCode::Char('A')), &mut items, &mut state);
        assert!(items.iter().all(|i| i.selected));
        handle_key(key(KeyCode::Char('n')), &mut items, &mut state);
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
        assert_eq!(state.filtered, vec![2, 0, 1]);
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

    // === viewport render ===

    #[test]
    fn test_buffer_shows_viewport_window_with_indicators() {
        let items: Vec<SelectItem> = (0..10)
            .map(|i| make_item(&format!("skill-{i:02}"), "desc"))
            .collect();
        let mut state = TuiState::new(items.len(), 4);
        state.cursor = 5;
        state.offset = 3;
        let buf = build_render_buffer(&items, &state, 20);
        assert!(buf.contains("skill-03"));
        assert!(buf.contains("skill-06"));
        assert!(!buf.contains("skill-00"));
        assert!(!buf.contains("skill-09"));
        assert!(buf.contains("more above"));
        assert!(buf.contains("more below"));
    }

    #[test]
    fn test_buffer_no_indicators_when_all_visible() {
        let items = vec![make_item("a", "x"), make_item("b", "y")];
        let state = TuiState::new(items.len(), 10);
        let buf = build_render_buffer(&items, &state, 20);
        assert!(!buf.contains("more above"));
        assert!(!buf.contains("more below"));
    }

    #[test]
    fn test_buffer_shows_filter_line_with_counts() {
        let items = vec![make_item("azure-x", "d"), make_item("other", "d")];
        let mut state = TuiState::new(items.len(), 10);
        state.filter.query = "azure".to_string();
        state.filter.focused = true;
        state.refilter(&items);
        let buf = build_render_buffer(&items, &state, 20);
        assert!(buf.contains("Filter"));
        assert!(buf.contains("azure"));
        assert!(buf.contains("[1/2 shown]"));
    }

    #[test]
    fn test_buffer_focused_hints_differ_from_unfocused() {
        let items = vec![make_item("a", "x")];
        let plain = TuiState::new(items.len(), 10);
        let mut focused_state = TuiState::new(items.len(), 10);
        focused_state.filter.focused = true;
        let a = build_render_buffer(&items, &plain, 20);
        let b = build_render_buffer(&items, &focused_state, 20);
        assert!(a.contains("/: filter"));
        assert!(b.contains("Esc/Enter: return to list"));
        assert!(!b.contains("/: filter"));
    }

    #[test]
    fn test_buffer_empty_filter_shows_no_matches() {
        let items = vec![make_item("a", "x")];
        let mut state = TuiState::new(items.len(), 10);
        state.filter.query = "zzz".to_string();
        state.filter.focused = true;
        state.refilter(&items);
        let buf = build_render_buffer(&items, &state, 20);
        assert!(buf.contains("No matches for \"zzz\""));
    }

    #[test]
    fn test_hints_fit_80_columns() {
        // Guards the CHROME_ROWS budget: a hint wider than the fallback
        // terminal width would wrap to a second row and clip the viewport.
        assert!(display_width(HINTS_UNFOCUSED) <= MENU_FALLBACK_COLS as usize);
        assert!(display_width(HINTS_FOCUSED) <= MENU_FALLBACK_COLS as usize);
    }

    #[test]
    fn test_space_toggles_highlighted_filtered_item() {
        let mut items = three_items();
        let mut state = TuiState::new(items.len(), 10);
        state.filter.query = "qdrant".to_string();
        state.refilter(&items);
        assert_eq!(state.filtered, vec![1]);
        handle_key(key(KeyCode::Char(' ')), &mut items, &mut state);
        assert!(items[1].selected);
        assert!(!items[0].selected);
        assert!(!items[2].selected);
    }

    #[test]
    fn test_format_item_numbering_alignment_for_three_digits() {
        let item = make_item("skill", "d");
        let single = format_item(0, &item, 10, 20, false);
        let triple = format_item(99, &item, 10, 20, false);
        assert_eq!(display_width(&single), display_width(&triple));
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
        assert_eq!(view_height(5), 0);
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
    #[test]
    fn scalable_enter_leaves_search_without_accepting() {
        let mut items = two_items();
        items[0].selected = true;
        let mut state = TuiState::new(items.len(), 10);
        state.filter.focused = true;
        assert!(handle_key(key(KeyCode::Enter), &mut items, &mut state).is_none());
        assert!(!state.filter.focused);
    }

    #[test]
    fn scalable_ctrl_c_cancels_search() {
        let mut items = two_items();
        let mut state = TuiState::new(items.len(), 10);
        state.filter.focused = true;
        assert!(matches!(
            handle_key(key_ctrl('c'), &mut items, &mut state),
            Some(Err(_))
        ));
    }

    #[test]
    fn scalable_filter_words_match_across_fields() {
        let items = vec![make_item("rust-audit", "Security checks")];
        assert_eq!(apply_filter(&items, "CHECKS rust"), vec![0]);
    }

    #[test]
    fn scalable_short_terminal_does_not_overflow() {
        assert_eq!(view_height(3), 0);
    }

    #[test]
    fn scalable_tab_reviews_selection_without_losing_hidden_choices() {
        let mut items: Vec<_> = (0..1000)
            .map(|i| make_item(&format!("skill-{i:04}"), "description"))
            .collect();
        items[990].selected = true;
        items[2].selected = true;
        let mut state = TuiState::new(items.len(), 10);
        handle_key(key(KeyCode::Tab), &mut items, &mut state);
        assert_eq!(state.filtered, vec![2, 990]);
        state.filter.query = "0990".into();
        state.refilter(&items);
        assert_eq!(state.filtered, vec![990]);
        let buf = build_render_buffer(&items, &state, 40);
        assert!(buf.contains("1 hidden"));
        handle_key(key(KeyCode::Char('n')), &mut items, &mut state);
        assert!(items[2].selected);
        assert!(!items[990].selected);
        assert!(state.filtered.is_empty());
    }

    fn plain_frame(frame: &str) -> String {
        let mut plain = String::new();
        let mut chars = frame.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                if chars.next() == Some('[') {
                    for a in chars.by_ref() {
                        if a.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
            } else {
                plain.push(c);
            }
        }
        plain
    }

    #[test]
    fn scalable_renderer_fits_height_and_sanitizes_metadata() {
        let items: Vec<_> = (0..1000)
            .map(|i| make_item(&format!("skill-{i}"), "bad\nline\r\t\x1b[31mtext"))
            .collect();
        let state = TuiState::new(items.len(), view_height(24));
        let frame = build_render_buffer(&items, &state, 40);
        assert!(!frame.contains("bad\nline"));
        let plain = plain_frame(&frame);
        assert!(
            plain.lines().count() <= 24,
            "{} rows",
            plain.lines().count()
        );
        assert!(plain.lines().all(|line| display_width(line) < 80));
    }

    #[test]
    fn scalable_details_reveal_long_description_tail() {
        let mut items = vec![make_item(
            "skill",
            &format!("{} tail-visible", "long description ".repeat(100)),
        )];
        let mut state = TuiState::new(items.len(), 10);
        handle_key(key(KeyCode::Char('d')), &mut items, &mut state);
        handle_key(key(KeyCode::End), &mut items, &mut state);
        let frame = build_render_buffer(&items, &state, 40);
        assert!(frame.contains("tail-visible"));
    }

    #[test]
    fn scalable_metadata_search_preserves_source_order() {
        let items = vec![
            ListItem {
                name: "alpha".into(),
                source: "z/repo".into(),
                description: "security".into(),
                ..ListItem::default()
            },
            ListItem {
                name: "beta".into(),
                source: "a/repo".into(),
                description: "security".into(),
                ..ListItem::default()
            },
        ];
        let mut state = TuiState::new(2, 10);
        state.filter.query = "security repo".into();
        state.refilter(&items);
        assert_eq!(state.filtered, vec![1, 0]);
        state.filter.query = "z/repo alpha".into();
        state.refilter(&items);
        assert_eq!(state.filtered, vec![0]);
    }

    #[test]
    fn scalable_browse_and_review_never_toggle_items() {
        for mode in [ListMode::Browse, ListMode::Review] {
            let mut items = two_items();
            let mut state = TuiState::new(2, 10);
            state.mode = mode;
            for code in [KeyCode::Char(' '), KeyCode::Char('a'), KeyCode::Tab] {
                handle_key(key(code), &mut items, &mut state);
            }
            assert_eq!(
                items.iter().map(|i| i.selected).collect::<Vec<_>>(),
                vec![false, true]
            );
            if mode == ListMode::Review {
                assert!(matches!(
                    handle_key(key(KeyCode::Enter), &mut items, &mut state),
                    Some(Err(_))
                ));
                state.filter.focused = true;
                assert!(handle_key(key(KeyCode::Char('y')), &mut items, &mut state).is_none());
                state.filter.focused = false;
                assert!(matches!(
                    handle_key(key(KeyCode::Char('y')), &mut items, &mut state),
                    Some(Ok(()))
                ));
            }
        }
    }

    #[test]
    fn scalable_frames_fit_resized_terminals() {
        let mut items: Vec<_> = (0..1000)
            .map(|i| ListItem {
                name: format!("скилл-{i}-{}", "界".repeat(30)),
                source: "owner/very-long-repo".into(),
                version: "1.0.0".into(),
                status: "tracked".into(),
                description: "long description ".repeat(50),
                ..ListItem::default()
            })
            .collect();
        let mut state = TuiState::new(1000, 10);
        for (cols, rows) in [(120, 40), (80, 24), (40, 12), (8, 3)] {
            state.resize(cols, rows);
            state.goto(999);
            for details in [false, true] {
                state.details = details;
                let frame = plain_frame(&build_render_buffer(&items, &state, 0));
                assert!(frame.lines().count() <= rows as usize);
                assert!(frame.lines().all(|l| display_width(l) < cols as usize));
            }
            if cols == 8 {
                assert!(handle_key(key(KeyCode::Char('y')), &mut items, &mut state).is_none());
                assert!(matches!(
                    handle_key(key_ctrl('c'), &mut items, &mut state),
                    Some(Err(_))
                ));
            }
        }
    }
    #[test]
    fn scalable_review_requires_unmodified_y() {
        let mut items = two_items();
        let mut state = TuiState::new(2, 10);
        state.mode = ListMode::Review;
        assert!(handle_key(key_ctrl('y'), &mut items, &mut state).is_none());
        assert!(
            handle_key(
                KeyEvent::new(KeyCode::Char('y'), KeyModifiers::ALT),
                &mut items,
                &mut state
            )
            .is_none()
        );
    }

    #[test]
    fn scalable_narrow_summary_keeps_hidden_selection_count() {
        let items: Vec<_> = (0..1000)
            .map(|i| ListItem {
                name: format!("skill-{i:04}"),
                selected: i < 2,
                ..ListItem::default()
            })
            .collect();
        let mut state = TuiState::new(1000, 2);
        state.resize(40, 12);
        state.filter.query = "skill-0000".into();
        state.refilter(&items);
        let frame = plain_frame(&build_render_buffer(&items, &state, 0));
        let summary = frame.lines().nth(1).unwrap();
        assert!(summary.contains("2 selected"), "{summary}");
        assert!(summary.contains("1 hidden"), "{summary}");
    }
    #[test]
    fn scalable_read_only_headers_show_reviewed_count() {
        let items = two_items();
        let mut state = TuiState::new(2, 10);
        for mode in [ListMode::Browse, ListMode::Review] {
            state.mode = mode;
            let frame = plain_frame(&build_render_buffer(&items, &state, 0));
            assert!(frame.lines().next().unwrap().contains("(2 skills)"));
            assert!(!frame.lines().nth(1).unwrap().contains("selected"));
        }
    }
    #[test]
    fn scalable_select_matches_is_idempotent_and_scoped() {
        let mut items = two_items();
        items[0].selected = true;
        let mut state = TuiState::new(2, 10);
        state.filter.query = items[0].name.clone();
        state.refilter(&items);
        handle_key(key(KeyCode::Char('a')), &mut items, &mut state);
        assert!(items.iter().all(|i| i.selected));
        handle_key(key(KeyCode::Char('n')), &mut items, &mut state);
        assert!(!items[0].selected);
        assert!(items[1].selected);
    }

    #[test]
    fn scalable_wide_rows_keep_status_before_long_metadata() {
        let items = vec![ListItem {
            name: "skill".into(),
            source: "very-long-repository/".repeat(20),
            status: "failed".into(),
            ..ListItem::default()
        }];
        let state = TuiState::new(1, 14);
        let frame = plain_frame(&build_render_buffer(&items, &state, 0));
        assert!(frame.lines().nth(5).unwrap().contains("failed"));
    }
    #[test]
    fn scalable_narrow_details_keep_back_action_visible() {
        let items = two_items();
        let mut state = TuiState::new(2, 2);
        state.resize(40, 12);
        state.details = true;
        let frame = plain_frame(&build_render_buffer(&items, &state, 0));
        assert!(frame.lines().last().unwrap().contains("Esc: back"));
    }
    #[test]
    fn review_bmp_emoji_frames_fit_independent_cell_budget() {
        let items = vec![ListItem {
            name: "skill".into(),
            description: "✅".repeat(45),
            ..ListItem::default()
        }];
        let mut state = TuiState::new(1, 2);
        state.resize(40, 12);
        for details in [false, true] {
            state.details = details;
            let frame = plain_frame(&build_render_buffer(&items, &state, 0));
            for line in frame.lines() {
                // Independent fixture oracle: ASCII is one cell; this emoji is two.
                let cells: usize = line.chars().map(|c| if c == '✅' { 2 } else { 1 }).sum();
                assert!(cells < 40, "{cells} cells: {line}");
            }
        }
    }
}
