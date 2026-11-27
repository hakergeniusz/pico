mod backup;
mod buffer;
mod syntax;

use base64::Engine;
use buffer::{Buffer, Pos};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, Terminal};
use std::error::Error;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

const UNDO_LIMIT: usize = 200;
const INDENT: &str = "    ";

fn closing_pair(ch: char) -> Option<char> {
    match ch {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        '\'' => Some('\''),
        '`' => Some('`'),
        _ => None,
    }
}

fn is_closing(ch: char) -> bool {
    matches!(ch, ')' | ']' | '}' | '"' | '\'' | '`')
}

fn thumb_bounds(total: usize, viewport: usize, scroll: usize, track: usize) -> (usize, usize) {
    if total == 0 || viewport == 0 || track == 0 {
        return (0, 0);
    }
    if total <= viewport {
        return (0, track);
    }
    let size = ((viewport * track) / total).max(1).min(track);
    let start = ((scroll * track) / total).min(track - size);
    (start, size)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PromptAction {
    Search,
    ReplaceOne,
    ReplaceAll,
    ConfirmQuit,
}

struct Prompt {
    action: PromptAction,
    label: &'static str,
    value: String,
    cursor: usize,
}

impl Prompt {
    fn len(&self) -> usize {
        self.value.chars().count()
    }

    fn insert(&mut self, ch: char) {
        let mut chars: Vec<char> = self.value.chars().collect();
        let at = self.cursor.min(chars.len());
        chars.insert(at, ch);
        self.value = chars.into_iter().collect();
        self.cursor = at + 1;
    }

    fn delete_before(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let mut chars: Vec<char> = self.value.chars().collect();
        let at = self.cursor.min(chars.len());
        chars.remove(at - 1);
        self.value = chars.into_iter().collect();
        self.cursor -= 1;
    }
}

struct App {
    buffer: Buffer,
    row: usize,
    col: usize,
    scroll: usize,
    undo: Vec<(Buffer, usize, usize)>,
    selection: Option<(Pos, Pos)>,
    clipboard: String,
    editor_area: Rect,
    scrollbar_area: Rect,
    gutter_width: usize,
    viewport: usize,
    prompt: Option<Prompt>,
    search_query: String,
    matches: Vec<(Pos, Pos)>,
    current_match: usize,
    case_sensitive: bool,
    message: String,
    should_quit: bool,
}

impl App {
    fn new(buffer: Buffer) -> Self {
        Self {
            buffer,
            row: 0,
            col: 0,
            scroll: 0,
            undo: Vec::new(),
            selection: None,
            clipboard: String::new(),
            editor_area: Rect::new(0, 0, 0, 0),
            scrollbar_area: Rect::new(0, 0, 0, 0),
            gutter_width: 1,
            viewport: 1,
            prompt: None,
            search_query: String::new(),
            matches: Vec::new(),
            current_match: 0,
            case_sensitive: false,
            message: "Ctrl+S save | Ctrl+F search | Ctrl+Z undo | Ctrl+R restore | wheel scroll | Ctrl+C copy | Ctrl+V paste | Ctrl+Q quit".to_string(),
            should_quit: false,
        }
    }

    fn snapshot(&mut self) {
        self.undo.push((self.buffer.clone(), self.row, self.col));
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
    }

    fn normalized_selection(&self) -> Option<(Pos, Pos)> {
        self.selection
            .map(|(a, b)| if a <= b { (a, b) } else { (b, a) })
    }

    fn selection_for_row(&self, row: usize, line_len: usize) -> Option<(usize, usize)> {
        let (start, end) = self.normalized_selection()?;
        if row < start.row || row > end.row {
            return None;
        }
        let from = if row == start.row {
            start.col.min(line_len)
        } else {
            0
        };
        let to = if row == end.row {
            end.col.min(line_len)
        } else {
            line_len
        };
        (from < to).then_some((from, to))
    }

    fn clear_selection(&mut self) {
        self.selection = None;
    }

    fn announce_selection(&mut self) {
        if let Some((start, end)) = self.normalized_selection() {
            let count = self.buffer.selected_text(start, end).chars().count();
            self.message = format!("selected {count} chars (Ctrl+C to copy)");
        }
    }

    fn copy_selection(&mut self) {
        let Some((start, end)) = self.normalized_selection() else {
            self.message = "nothing selected".to_string();
            return;
        };
        let text = self.buffer.selected_text(start, end);
        if text.is_empty() {
            self.message = "nothing selected".to_string();
            return;
        }
        self.clipboard = text.clone();
        let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
        let sequence = format!("\x1b]52;c;{encoded}\x1b\\");
        let _ = io::stdout().write_all(sequence.as_bytes());
        let _ = io::stdout().flush();
        self.message = format!("copied {} chars", text.chars().count());
    }

    fn paste(&mut self) {
        if self.clipboard.is_empty() {
            self.message = "clipboard empty".to_string();
            return;
        }
        self.snapshot();
        let (row, col) = self
            .buffer
            .insert_text(self.row, self.col, &self.clipboard);
        self.row = row;
        self.col = col;
        self.clear_selection();
        self.message = "pasted".to_string();
    }

    fn restore_backup(&mut self) {
        let Some(path) = self.buffer.path.clone() else {
            self.message = "no file path".to_string();
            return;
        };
        match backup::latest(&path) {
            Ok(Some(backup_path)) => match std::fs::read_to_string(&backup_path) {
                Ok(text) => {
                    self.snapshot();
                    self.buffer = Buffer::from_text(text, path);
                    self.row = 0;
                    self.col = 0;
                    self.scroll = 0;
                    self.clear_selection();
                    let name = backup_path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("backup");
                    self.message = format!("restored {name}, Ctrl+S to keep it");
                            }
                Err(error) => self.message = format!("restore failed: {error}"),
            },
            Ok(None) => self.message = "no backups found".to_string(),
            Err(error) => self.message = format!("backup lookup failed: {error}"),
        }
    }

    fn save_file(&mut self) -> bool {
        let backup_result = match self.buffer.path.as_ref() {
            Some(path) => backup::before_save(path),
            None => Ok(PathBuf::new()),
        };
        match backup_result {
            Ok(_) => match self.buffer.save() {
                Ok(()) => {
                    self.message = "saved".to_string();
                    true
                }
                Err(error) => {
                    self.message = format!("save failed: {error}");
                    false
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => match self.buffer.save() {
                Ok(()) => {
                    self.message = "saved".to_string();
                    true
                }
                Err(error) => {
                    self.message = format!("save failed: {error}");
                    false
                }
            },
            Err(error) => {
                self.message = format!("backup failed: {error}; not saved");
                false
            }
        }
    }

    fn open_prompt(&mut self, action: PromptAction, label: &'static str) {
        self.prompt = Some(Prompt {
            action,
            label,
            value: String::new(),
            cursor: 0,
        });
        self.message.clear();
    }

    fn handle_prompt_key(&mut self, key: KeyEvent) -> bool {
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let control = key.modifiers.contains(KeyModifiers::CONTROL);

        if self
            .prompt
            .as_ref()
            .is_some_and(|prompt| prompt.action == PromptAction::ConfirmQuit)
        {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.prompt = None;
                    if self.save_file() {
                        self.should_quit = true;
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.prompt = None;
                    self.should_quit = true;
                }
                KeyCode::Char('q') if control => {
                    self.prompt = None;
                    self.should_quit = true;
                }
                KeyCode::Esc => {
                    self.prompt = None;
                    self.message = "quit cancelled".to_string();
                }
                _ => {}
            }
            return true;
        }

        match key.code {
            KeyCode::Esc => {
                self.prompt = None;
                self.message = "prompt cancelled".to_string();
                return true;
            }
            KeyCode::Enter => {
                if let Some(prompt) = self.prompt.take() {
                    self.accept_prompt(prompt);
                }
                return true;
            }
            KeyCode::Backspace => {
                if let Some(prompt) = self.prompt.as_mut() {
                    prompt.delete_before();
                }
                return true;
            }
            KeyCode::Left => {
                if let Some(prompt) = self.prompt.as_mut() {
                    prompt.cursor = prompt.cursor.saturating_sub(1);
                }
                return true;
            }
            KeyCode::Right => {
                if let Some(prompt) = self.prompt.as_mut() {
                    prompt.cursor = (prompt.cursor + 1).min(prompt.len());
                }
                return true;
            }
            KeyCode::Home => {
                if let Some(prompt) = self.prompt.as_mut() {
                    prompt.cursor = 0;
                }
                return true;
            }
            KeyCode::End => {
                if let Some(prompt) = self.prompt.as_mut() {
                    prompt.cursor = prompt.len();
                }
                return true;
            }
            KeyCode::Char('c') if alt => {
                self.case_sensitive = !self.case_sensitive;
                return true;
            }
            KeyCode::Char(c) if !control => {
                if let Some(prompt) = self.prompt.as_mut() {
                    prompt.insert(c);
                }
                return true;
            }
            _ => {}
        }
        true
    }

    fn accept_prompt(&mut self, prompt: Prompt) {
        match prompt.action {
            PromptAction::Search => {
                self.search_query = prompt.value;
                self.recompute_matches();
                if self.matches.is_empty() {
                    self.message = format!("no matches for '{}'", self.search_query);
                } else {
                    self.current_match = 0;
                    self.goto_match(0);
                    self.message = format!("match 1/{}", self.matches.len());
                }
            }
            PromptAction::ReplaceOne => self.replace_current(&prompt.value),
            PromptAction::ReplaceAll => self.replace_all(&prompt.value),
            PromptAction::ConfirmQuit => {}
        }
    }

    fn recompute_matches(&mut self) {
        self.matches = self
            .buffer
            .find_matches(&self.search_query, self.case_sensitive);
        if self.current_match >= self.matches.len() {
            self.current_match = 0;
        }
    }

    fn goto_match(&mut self, index: usize) {
        if let Some((start, _)) = self.matches.get(index).copied() {
            self.row = start.row;
            self.col = start.col;
            self.clear_selection();
            self.clamp();
            self.ensure_visible(self.viewport);
            self.current_match = index;
        }
    }

    fn next_match(&mut self, delta: isize) {
        if self.matches.is_empty() {
            self.message = "no matches".to_string();
            return;
        }
        let len = self.matches.len() as isize;
        let index = (self.current_match as isize + delta).rem_euclid(len) as usize;
        self.goto_match(index);
        self.message = format!("match {}/{}", index + 1, len);
    }

    fn replace_current(&mut self, text: &str) {
        if self.matches.is_empty() {
            self.message = "no matches".to_string();
            return;
        }
        let index = self.current_match;
        let (start, end) = self.matches[index];
        self.snapshot();
        let (row, col) = self.buffer.replace_range(start, end, text);
        self.row = row;
        self.col = col;
        self.recompute_matches();
        if !self.matches.is_empty() {
            let index = index.min(self.matches.len() - 1);
            self.goto_match(index);
        }
        self.message = "replaced 1 match".to_string();
    }

    fn replace_all(&mut self, text: &str) {
        if self.matches.is_empty() {
            self.message = "no matches".to_string();
            return;
        }
        let count = self.matches.len();
        self.snapshot();
        for (start, end) in self.matches.iter().rev().copied() {
            self.buffer.replace_range(start, end, text);
        }
        self.recompute_matches();
        self.message = format!("replaced {count} matches");
    }

    fn clamp(&mut self) {
        let len = self.buffer.line_len(self.row);
        self.col = self.col.min(len);
    }

    fn ensure_visible(&mut self, viewport: usize) {
        if viewport == 0 {
            return;
        }
        if self.row < self.scroll {
            self.scroll = self.row;
        }
        if self.row >= self.scroll + viewport {
            self.scroll = self.row - viewport + 1;
        }
    }

    fn scroll_lines(&mut self, delta: isize) {
        let max_scroll = self.buffer.lines.len().saturating_sub(self.viewport);
        if delta < 0 {
            self.scroll = self.scroll.saturating_sub(delta.unsigned_abs());
        } else {
            self.scroll = self.scroll.saturating_add(delta as usize).min(max_scroll);
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.scroll_lines(-3);
                self.message = format!("scroll {}", self.scroll + 1);
                return;
            }
            MouseEventKind::ScrollDown => {
                self.scroll_lines(3);
                self.message = format!("scroll {}", self.scroll + 1);
                return;
            }
            _ => {}
        }

        let height = self.editor_area.height as usize;
        let view_row = match mouse.row.checked_sub(self.editor_area.y) {
            Some(value) => value as usize,
            None => return,
        };
        if view_row >= height {
            return;
        }

        let gutter_end = self.editor_area.x + self.gutter_width as u16;
        let in_gutter = mouse.column >= self.editor_area.x && mouse.column < gutter_end;
        let in_scrollbar = mouse.column >= self.scrollbar_area.x
            && mouse.column < self.scrollbar_area.x + self.scrollbar_area.width;

        if in_scrollbar {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left)
                | MouseEventKind::Drag(MouseButton::Left) => {
                    let track = self.scrollbar_area.height as usize;
                    if track > 0 {
                        let total = self.buffer.lines.len();
                        let max_scroll = total.saturating_sub(self.viewport);
                        self.scroll = ((view_row * total) / track).min(max_scroll);
                        self.message = format!("scroll {}", self.scroll + 1);
                    }
                }
                _ => {}
            }
            return;
        }

        let row = self.scroll + view_row;
        if row >= self.buffer.lines.len() {
            return;
        }
        let pos = if in_gutter {
            Pos::new(row, 0)
        } else {
            let col = mouse.column.saturating_sub(gutter_end) as usize;
            Pos::new(row, col.min(self.buffer.line_len(row)))
        };

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let head = if in_gutter {
                    Pos::new(row + 1, 0)
                } else {
                    pos
                };
                self.selection = Some((pos, head));
                self.row = row;
                self.col = pos.col;
                        self.announce_selection();
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some((anchor, _)) = self.selection {
                    let head = if in_gutter {
                        Pos::new(row + 1, 0)
                    } else {
                        pos
                    };
                    self.selection = Some((anchor, head));
                    self.row = pos.row;
                    self.col = pos.col;
                    self.announce_selection();
                }
            }
            _ => {}
        }
        self.clamp();
    }

    fn handle_key(&mut self, key: KeyEvent, viewport: usize) -> bool {
        if key.kind == KeyEventKind::Release {
            return true;
        }
        if self.prompt.is_some() {
            return self.handle_prompt_key(key);
        }
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);

        if control && alt && key.code == KeyCode::Char('h') {
            self.open_prompt(PromptAction::ReplaceAll, "Replace all with");
            return true;
        }
        if key.code == KeyCode::F(3) {
            let backwards = key.modifiers.contains(KeyModifiers::SHIFT);
            self.next_match(if backwards { -1 } else { 1 });
            return true;
        }
        if key.code == KeyCode::BackTab {
            self.next_match(-1);
            return true;
        }

        match (key.code, control) {
            (KeyCode::Char('s'), true) => {
                self.save_file();
            }
            (KeyCode::Char('z'), true) => {
                if let Some((buffer, row, col)) = self.undo.pop() {
                    self.buffer = buffer;
                    self.row = row;
                    self.col = col;
                    self.message = "undo".to_string();
                } else {
                    self.message = "nothing to undo".to_string();
                }
                self.clear_selection();
            }
            (KeyCode::Char('c'), true) => self.copy_selection(),
            (KeyCode::Char('v'), true) => self.paste(),
            (KeyCode::Char('r'), true) => self.restore_backup(),
            (KeyCode::Char('f'), true) => {
                self.open_prompt(PromptAction::Search, "Search")
            }
            (KeyCode::Char('h'), true) => {
                self.open_prompt(PromptAction::ReplaceOne, "Replace with")
            }
            (KeyCode::Char('q'), true) | (KeyCode::Esc, _) => {
                if self.buffer.dirty {
                    self.open_prompt(PromptAction::ConfirmQuit, "Save changes? (y/n/esc)");
                    return true;
                }
                self.should_quit = true;
                return false;
            }
            (KeyCode::Char(c), false) => {
                self.snapshot();
                if is_closing(c) && self.buffer.char_at(self.row, self.col) == Some(c) {
                    self.col += 1;
                } else if let Some(close) = closing_pair(c) {
                    let apostrophe_ok = c != '\''
                        || self.col == 0
                        || self
                            .buffer
                            .char_at(self.row, self.col - 1)
                            .map_or(true, |prev| !prev.is_alphanumeric() && prev != '\'');
                    if apostrophe_ok {
                        let pair = format!("{c}{close}");
                        self.col = self.buffer.insert_str(self.row, self.col, &pair).saturating_sub(1);
                    } else {
                        self.buffer.insert_char(self.row, self.col, c);
                        self.col += 1;
                    }
                } else {
                    self.buffer.insert_char(self.row, self.col, c);
                    self.col += 1;
                }
                self.clear_selection();
                    }
            (KeyCode::Tab, _) => {
                self.snapshot();
                self.col = self.buffer.insert_str(self.row, self.col, INDENT);
                self.clear_selection();
                    }
            (KeyCode::Enter, _) => {
                self.snapshot();
                if let Some((row, col)) = self.buffer.split_line(self.row, self.col) {
                    self.row = row;
                    self.col = col;
                }
                self.clear_selection();
                    }
            (KeyCode::Backspace, _) => {
                self.snapshot();
                if let Some((row, col)) = self.buffer.delete_before(self.row, self.col) {
                    self.row = row;
                    self.col = col;
                }
                self.clear_selection();
                    }
            (KeyCode::Delete, _) => {
                self.snapshot();
                self.buffer.delete_at(self.row, self.col);
                self.clear_selection();
                    }
            (KeyCode::Left, _) => {
                self.col = self.col.saturating_sub(1);
                self.clear_selection();
            }
            (KeyCode::Right, _) => {
                self.col = (self.col + 1).min(self.buffer.line_len(self.row));
                self.clear_selection();
            }
            (KeyCode::Up, _) | (KeyCode::PageUp, _) => {
                let step = if key.code == KeyCode::PageUp {
                    viewport.max(1)
                } else {
                    1
                };
                self.row = self.row.saturating_sub(step);
                self.clear_selection();
            }
            (KeyCode::Down, _) | (KeyCode::PageDown, _) => {
                let step = if key.code == KeyCode::PageDown {
                    viewport.max(1)
                } else {
                    1
                };
                self.row = (self.row + step).min(self.buffer.lines.len() - 1);
                self.clear_selection();
            }
            (KeyCode::Home, _) => {
                self.col = 0;
                self.clear_selection();
            }
            (KeyCode::End, _) => {
                self.col = self.buffer.line_len(self.row);
                self.clear_selection();
            }
            _ => {}
        }

        self.clamp();
        self.ensure_visible(viewport);
        true
    }
}

fn highlighted(
    line: &[char],
    styles: &[Style],
    selection: Option<(usize, usize)>,
    matches: &[(usize, usize, bool)],
) -> Line<'static> {
    let len = line.len();
    if len == 0 {
        return Line::from(String::new());
    }
    let (from, to) = match selection {
        Some((start, end)) => (start.min(len), end.min(len).max(start.min(len))),
        None => (len, len),
    };

    let style_at = |index: usize| {
        let base = styles.get(index).copied().unwrap_or_default();
        let mut style = if (from..to).contains(&index) {
            base.add_modifier(Modifier::REVERSED)
        } else {
            base
        };
        for &(match_from, match_to, is_current) in matches {
            if (match_from..match_to).contains(&index) {
                style = if is_current {
                    style.bg(Color::Yellow)
                } else {
                    style.add_modifier(Modifier::UNDERLINED)
                };
            }
        }
        style
    };

    let mut spans = Vec::new();
    let mut start = 0;
    let mut current = style_at(0);
    for index in 1..len {
        let style = style_at(index);
        if style != current {
            spans.push(Span::styled(
                line[start..index].iter().collect::<String>(),
                current,
            ));
            start = index;
            current = style;
        }
    }
    spans.push(Span::styled(
        line[start..len].iter().collect::<String>(),
        current,
    ));
    Line::from(spans)
}

fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let [editor, status_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);

    let digits = app.buffer.lines.len().max(1).to_string().len();
    let gutter_width = digits + 1;
    app.gutter_width = gutter_width;
    app.editor_area = editor;
    let [gutter_area, text_area, scrollbar_area] = Layout::horizontal([
        Constraint::Length(gutter_width as u16),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(editor);
    app.scrollbar_area = scrollbar_area;

    let visible = text_area.height as usize;
    app.viewport = visible;

    let gutter_lines: Vec<Line> = (app.scroll..app.scroll + visible)
        .take_while(|row| *row < app.buffer.lines.len())
        .map(|row| {
            Line::from(Span::styled(
                format!("{:>width$} ", row + 1, width = digits),
                Style::default().fg(Color::DarkGray),
            ))
        })
        .collect();

    let ext = syntax::extension(app.buffer.path.as_deref());
    let text_lines: Vec<Line> = app
        .buffer
        .lines
        .iter()
        .skip(app.scroll)
        .take(visible)
        .enumerate()
        .map(|(offset, line)| {
            let row = app.scroll + offset;
            let styles = syntax::styles_for_line(line, &ext);
            let row_matches: Vec<(usize, usize, bool)> = app
                .matches
                .iter()
                .enumerate()
                .filter(|(_, (start, end))| start.row == row && end.row == row)
                .map(|(index, (start, end))| (start.col, end.col, index == app.current_match))
                .collect();
            highlighted(
                line,
                &styles,
                app.selection_for_row(row, line.len()),
                &row_matches,
            )
        })
        .collect();

    let track = scrollbar_area.height as usize;
    let (thumb_start, thumb_size) =
        thumb_bounds(app.buffer.lines.len(), visible, app.scroll, track);
    let scrollbar_lines: Vec<Line> = (0..track)
        .map(|row| {
            let (glyph, style) = if row >= thumb_start && row < thumb_start + thumb_size {
                ("█", Style::default().fg(Color::Cyan))
            } else {
                ("│", Style::default().fg(Color::DarkGray))
            };
            Line::from(Span::styled(glyph, style))
        })
        .collect();

    frame.render_widget(Paragraph::new(gutter_lines), gutter_area);
    frame.render_widget(Paragraph::new(text_lines), text_area);
    frame.render_widget(Paragraph::new(scrollbar_lines), scrollbar_area);

    let name = app
        .buffer
        .path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "untitled".to_string());
    let dirty = if app.buffer.dirty { " *" } else { "" };
    let status = match &app.prompt {
        Some(prompt) if prompt.action == PromptAction::ConfirmQuit => {
            format!(" {}   y = save and quit | n = quit | Esc = cancel", prompt.label)
        }
        Some(prompt) => {
            let case = if app.case_sensitive { "Aa" } else { "aa" };
            format!(
                " {}: {}  [{}]  Enter ok | Esc cancel | Alt+C case",
                prompt.label, prompt.value, case
            )
        }
        None => format!(
            " {name}{dirty}  L{}:{}  {}",
            app.row + 1,
            app.col + 1,
            app.message
        ),
    };
    frame.render_widget(
        Paragraph::new(status).style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        status_area,
    );

    if let Some(prompt) = &app.prompt {
        if prompt.action != PromptAction::ConfirmQuit {
            let offset = 2 + prompt.label.chars().count() + prompt.cursor;
            let x = status_area.x + (offset as u16).min(status_area.right().saturating_sub(1));
            frame.set_cursor_position((x, status_area.y));
        }
    } else if text_area.height > 0
        && text_area.width > 0
        && app.row >= app.scroll
        && app.row < app.scroll + visible
    {
        let x = text_area.x + (app.col as u16).min(text_area.right().saturating_sub(1));
        let y = text_area.y + (app.row - app.scroll) as u16;
        frame.set_cursor_position((x, y));
    }
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut app: App,
) -> Result<(), Box<dyn Error>> {
    loop {
        let viewport = terminal.size()?.height.saturating_sub(1) as usize;
        terminal.draw(|frame| draw(frame, &mut app))?;
        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) => {
                if !app.handle_key(key, viewport) {
                    break;
                }
            }
            Event::Mouse(mouse) => app.handle_mouse(mouse),
            _ => {}
        }
        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args().nth(1).map(PathBuf::from);
    let buffer = match path {
        Some(path) => Buffer::open(path)?,
        None => Buffer::empty(),
    };
    let app = App::new(buffer);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run(&mut terminal, app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    result
}

#[cfg(test)]
mod tests {
    use super::{closing_pair, is_closing, thumb_bounds, App, Buffer, Prompt, PromptAction};

    #[test]
    fn prompt_cursor_edits_at_position() {
        let mut prompt = Prompt {
            action: PromptAction::Search,
            label: "Search",
            value: "ab".to_string(),
            cursor: 1,
        };
        prompt.insert('X');
        assert_eq!(prompt.value, "aXb");
        assert_eq!(prompt.cursor, 2);
        prompt.delete_before();
        assert_eq!(prompt.value, "ab");
        assert_eq!(prompt.cursor, 1);
        prompt.cursor = 0;
        prompt.delete_before();
        assert_eq!(prompt.value, "ab");
    }

    #[test]
    fn auto_close_pairs_are_defined() {
        assert_eq!(closing_pair('('), Some(')'));
        assert_eq!(closing_pair('{'), Some('}'));
        assert_eq!(closing_pair('"'), Some('"'));
        assert_eq!(closing_pair('a'), None);
        assert!(is_closing(']'));
        assert!(!is_closing('['));
    }

    #[test]
    fn scrollbar_thumb_tracks_viewport() {
        assert_eq!(thumb_bounds(20, 9, 0, 9), (0, 4));
        assert_eq!(thumb_bounds(20, 9, 11, 9), (4, 4));
        assert_eq!(thumb_bounds(5, 9, 0, 9), (0, 9));
        assert_eq!(thumb_bounds(20, 0, 0, 9), (0, 0));
    }

    #[test]
    fn wheel_scroll_clamps_to_content() {
        let mut app = App::new(Buffer::empty());
        app.buffer.lines = vec![Vec::new(); 10];
        app.viewport = 4;
        app.scroll_lines(3);
        assert_eq!(app.scroll, 3);
        app.scroll_lines(3);
        assert_eq!(app.scroll, 6);
        app.scroll_lines(-2);
        assert_eq!(app.scroll, 4);
        app.scroll_lines(-100);
        assert_eq!(app.scroll, 0);
        app.scroll_lines(100);
        assert_eq!(app.scroll, 6);
    }
}
