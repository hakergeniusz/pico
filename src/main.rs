mod buffer;

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
    message: String,
    quit_armed: bool,
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
            message: "Ctrl+S save | Ctrl+Z undo | wheel scroll | click number / drag select | Ctrl+C copy | Ctrl+V paste | Ctrl+Q quit".to_string(),
            quit_armed: false,
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
        self.quit_armed = false;
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
                self.quit_armed = false;
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
        let control = key.modifiers.contains(KeyModifiers::CONTROL);

        match (key.code, control) {
            (KeyCode::Char('s'), true) => match self.buffer.save() {
                Ok(()) => self.message = "saved".to_string(),
                Err(error) => self.message = format!("save failed: {error}"),
            },
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
            (KeyCode::Char('q'), true) | (KeyCode::Esc, _) => {
                if self.buffer.dirty && !self.quit_armed {
                    self.quit_armed = true;
                    self.message = "unsaved changes: press again to quit".to_string();
                    return true;
                }
                return false;
            }
            (KeyCode::Char(c), false) => {
                self.snapshot();
                self.buffer.insert_char(self.row, self.col, c);
                self.col += 1;
                self.clear_selection();
                self.quit_armed = false;
            }
            (KeyCode::Tab, _) => {
                self.snapshot();
                self.col = self.buffer.insert_str(self.row, self.col, INDENT);
                self.clear_selection();
                self.quit_armed = false;
            }
            (KeyCode::Enter, _) => {
                self.snapshot();
                if let Some((row, col)) = self.buffer.split_line(self.row, self.col) {
                    self.row = row;
                    self.col = col;
                }
                self.clear_selection();
                self.quit_armed = false;
            }
            (KeyCode::Backspace, _) => {
                self.snapshot();
                if let Some((row, col)) = self.buffer.delete_before(self.row, self.col) {
                    self.row = row;
                    self.col = col;
                }
                self.clear_selection();
                self.quit_armed = false;
            }
            (KeyCode::Delete, _) => {
                self.snapshot();
                self.buffer.delete_at(self.row, self.col);
                self.clear_selection();
                self.quit_armed = false;
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

fn highlighted(line: &[char], selection: Option<(usize, usize)>) -> Line<'static> {
    let len = line.len();
    let Some((from, to)) = selection else {
        return Line::from(line.iter().collect::<String>());
    };
    let from = from.min(len);
    let to = to.min(len).max(from);
    let mut spans = Vec::new();
    if from > 0 {
        spans.push(Span::raw(line[..from].iter().collect::<String>()));
    }
    spans.push(Span::styled(
        line[from..to].iter().collect::<String>(),
        Style::default().add_modifier(Modifier::REVERSED),
    ));
    if to < len {
        spans.push(Span::raw(line[to..].iter().collect::<String>()));
    }
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
    let selected_rows = app.normalized_selection();

    let gutter_lines: Vec<Line> = (app.scroll..app.scroll + visible)
        .take_while(|row| *row < app.buffer.lines.len())
        .map(|row| {
            let style = if selected_rows.is_some_and(|(start, end)| row >= start.row && row <= end.row)
            {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            Line::from(Span::styled(
                format!("{:>width$} ", row + 1, width = digits),
                style,
            ))
        })
        .collect();

    let text_lines: Vec<Line> = app
        .buffer
        .lines
        .iter()
        .skip(app.scroll)
        .take(visible)
        .enumerate()
        .map(|(offset, line)| {
            let row = app.scroll + offset;
            highlighted(line, app.selection_for_row(row, line.len()))
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
    let status = format!(
        " {name}{dirty}  L{}:{}  {}",
        app.row + 1,
        app.col + 1,
        app.message
    );
    frame.render_widget(
        Paragraph::new(status).style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        status_area,
    );

    if text_area.height > 0
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
    use super::{thumb_bounds, App, Buffer};

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
