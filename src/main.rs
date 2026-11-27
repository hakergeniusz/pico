mod buffer;

use buffer::Buffer;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::{Frame, Terminal};
use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

const UNDO_LIMIT: usize = 200;
const INDENT: &str = "    ";

struct App {
    buffer: Buffer,
    row: usize,
    col: usize,
    scroll: usize,
    undo: Vec<(Buffer, usize, usize)>,
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
            message: "Ctrl+S save | Ctrl+Z undo | Ctrl+Q or Esc quit".to_string(),
            quit_armed: false,
        }
    }

    fn snapshot(&mut self) {
        self.undo.push((self.buffer.clone(), self.row, self.col));
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
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
            }
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
                self.quit_armed = false;
            }
            (KeyCode::Tab, _) => {
                self.snapshot();
                self.col = self.buffer.insert_str(self.row, self.col, INDENT);
                self.quit_armed = false;
            }
            (KeyCode::Enter, _) => {
                self.snapshot();
                if let Some((row, col)) = self.buffer.split_line(self.row, self.col) {
                    self.row = row;
                    self.col = col;
                }
                self.quit_armed = false;
            }
            (KeyCode::Backspace, _) => {
                self.snapshot();
                if let Some((row, col)) = self.buffer.delete_before(self.row, self.col) {
                    self.row = row;
                    self.col = col;
                }
                self.quit_armed = false;
            }
            (KeyCode::Delete, _) => {
                self.snapshot();
                self.buffer.delete_at(self.row, self.col);
                self.quit_armed = false;
            }
            (KeyCode::Left, _) => {
                self.col = self.col.saturating_sub(1);
            }
            (KeyCode::Right, _) => {
                self.col = (self.col + 1).min(self.buffer.line_len(self.row));
            }
            (KeyCode::Up, _) | (KeyCode::PageUp, _) => {
                let step = if key.code == KeyCode::PageUp {
                    viewport.max(1)
                } else {
                    1
                };
                self.row = self.row.saturating_sub(step);
            }
            (KeyCode::Down, _) | (KeyCode::PageDown, _) => {
                let step = if key.code == KeyCode::PageDown {
                    viewport.max(1)
                } else {
                    1
                };
                self.row = (self.row + step).min(self.buffer.lines.len() - 1);
            }
            (KeyCode::Home, _) => self.col = 0,
            (KeyCode::End, _) => self.col = self.buffer.line_len(self.row),
            _ => {}
        }

        self.clamp();
        self.ensure_visible(viewport);
        true
    }
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let [editor, status_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);

    let lines: Vec<Line> = app
        .buffer
        .lines
        .iter()
        .skip(app.scroll)
        .take(editor.height as usize)
        .map(|line| Line::from(line.iter().collect::<String>()))
        .collect();
    frame.render_widget(Paragraph::new(lines), editor);

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

    if editor.height > 0 && editor.width > 0 {
        let x = editor.x + (app.col as u16).min(editor.right().saturating_sub(1));
        let y = editor.y + (app.row - app.scroll) as u16;
        frame.set_cursor_position((x, y));
    }
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut app: App,
) -> Result<(), Box<dyn Error>> {
    loop {
        let viewport = terminal.size()?.height.saturating_sub(1) as usize;
        terminal.draw(|frame| draw(frame, &app))?;
        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        if let Event::Key(key) = event::read()? {
            if !app.handle_key(key, viewport) {
                break;
            }
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
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run(&mut terminal, app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}
