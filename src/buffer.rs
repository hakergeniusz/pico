use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub row: usize,
    pub col: usize,
}

impl Pos {
    pub fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Buffer {
    pub lines: Vec<Vec<char>>,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub trailing_newline: bool,
}

impl Buffer {
    pub fn empty() -> Self {
        Self {
            lines: vec![Vec::new()],
            path: None,
            dirty: false,
            trailing_newline: false,
        }
    }

    pub fn new(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            ..Self::empty()
        }
    }

    pub fn from_text(text: String, path: PathBuf) -> Self {
        let trailing_newline = text.ends_with('\n');
        let mut lines: Vec<Vec<char>> = text
            .split('\n')
            .map(|line| line.chars().collect())
            .collect();
        if trailing_newline {
            lines.pop();
        }
        if lines.is_empty() {
            lines.push(Vec::new());
        }
        Self {
            lines,
            path: Some(path),
            dirty: false,
            trailing_newline,
        }
    }

    pub fn open(path: PathBuf) -> io::Result<Self> {
        match fs::read_to_string(&path) {
            Ok(text) => Ok(Self::from_text(text, path)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::new(path)),
            Err(error) => Err(error),
        }
    }

    pub fn save(&mut self) -> io::Result<()> {
        let path = self.path.clone().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "no file path to save to")
        })?;
        if self.lines.len() > 1 || self.lines.first().is_some_and(|line| !line.is_empty()) {
            self.trailing_newline = true;
        }
        fs::write(path, self.as_text())?;
        self.dirty = false;
        Ok(())
    }

    pub fn as_text(&self) -> String {
        let mut text = String::new();
        for (index, line) in self.lines.iter().enumerate() {
            if index > 0 {
                text.push('\n');
            }
            text.extend(line);
        }
        if self.trailing_newline {
            text.push('\n');
        }
        text
    }

    pub fn line_len(&self, row: usize) -> usize {
        self.lines.get(row).map_or(0, Vec::len)
    }

    pub fn char_at(&self, row: usize, col: usize) -> Option<char> {
        self.lines.get(row)?.get(col).copied()
    }

    pub fn insert_char(&mut self, row: usize, col: usize, ch: char) {
        if let Some(line) = self.lines.get_mut(row) {
            let at = col.min(line.len());
            line.insert(at, ch);
            self.dirty = true;
        }
    }

    pub fn insert_str(&mut self, row: usize, col: usize, text: &str) -> usize {
        if let Some(line) = self.lines.get_mut(row) {
            let at = col.min(line.len());
            for (offset, ch) in text.chars().enumerate() {
                line.insert(at + offset, ch);
            }
            self.dirty = true;
            at + text.chars().count()
        } else {
            col
        }
    }

    pub fn delete_before(&mut self, row: usize, col: usize) -> Option<(usize, usize)> {
        let line = self.lines.get_mut(row)?;
        if col > line.len() {
            return None;
        }
        if col > 0 {
            line.remove(col - 1);
            self.dirty = true;
            return Some((row, col - 1));
        }
        if row == 0 {
            return None;
        }
        let current: Vec<char> = self.lines.remove(row);
        let previous = &mut self.lines[row - 1];
        let new_col = previous.len();
        previous.extend(current);
        self.dirty = true;
        Some((row - 1, new_col))
    }

    pub fn delete_at(&mut self, row: usize, col: usize) {
        if row >= self.lines.len() {
            return;
        }
        if col < self.lines[row].len() {
            self.lines[row].remove(col);
            self.dirty = true;
            return;
        }
        if row + 1 < self.lines.len() {
            let next = self.lines.remove(row + 1);
            self.lines[row].extend(next);
            self.dirty = true;
        }
    }

    pub fn split_line(&mut self, row: usize, col: usize) -> Option<(usize, usize)> {
        let line = self.lines.get_mut(row)?;
        let at = col.min(line.len());
        let tail: Vec<char> = line.split_off(at);
        self.lines.insert(row + 1, tail);
        self.dirty = true;
        Some((row + 1, 0))
    }

    pub fn selected_text(&self, start: Pos, end: Pos) -> String {
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let mut text = String::new();
        for row in start.row..=end.row {
            let line = self.lines.get(row).map(Vec::as_slice).unwrap_or(&[]);
            let from = if row == start.row {
                start.col.min(line.len())
            } else {
                0
            };
            let to = if row == end.row {
                end.col.min(line.len())
            } else {
                line.len()
            };
            if from < to {
                text.extend(line[from..to].iter());
            }
            if row < end.row {
                text.push('\n');
            }
        }
        text
    }

    pub fn insert_text(&mut self, row: usize, col: usize, text: &str) -> (usize, usize) {
        let mut row = row;
        let mut col = col;
        let mut parts = text.split('\n').peekable();
        while let Some(part) = parts.next() {
            col = self.insert_str(row, col, part);
            if parts.peek().is_some() {
                if let Some((next_row, next_col)) = self.split_line(row, col) {
                    row = next_row;
                    col = next_col;
                }
            }
        }
        (row, col)
    }
}

#[cfg(test)]
mod tests {
    use super::{Buffer, Pos};

    #[test]
    fn insert_and_delete_before() {
        let mut buffer = Buffer::empty();
        buffer.insert_char(0, 0, 'a');
        buffer.insert_char(0, 1, 'b');
        assert_eq!(buffer.as_text(), "ab");
        assert_eq!(buffer.delete_before(0, 2), Some((0, 1)));
        assert_eq!(buffer.as_text(), "a");
        assert_eq!(buffer.delete_before(0, 0), None);
    }

    #[test]
    fn split_and_merge_lines() {
        let mut buffer = Buffer::empty();
        buffer.insert_str(0, 0, "abcd");
        assert_eq!(buffer.split_line(0, 2), Some((1, 0)));
        assert_eq!(buffer.as_text(), "ab\ncd");
        assert_eq!(buffer.delete_before(1, 0), Some((0, 2)));
        assert_eq!(buffer.as_text(), "abcd");
        buffer.delete_at(0, 2);
        assert_eq!(buffer.as_text(), "abd");
    }

    #[test]
    fn open_and_save_roundtrip_preserves_trailing_newline() {
        let path = std::env::temp_dir().join(format!(
            "pico-buffer-test-{}.txt",
            std::process::id()
        ));
        std::fs::write(&path, "hello\nworld\n").unwrap();
        let mut buffer = Buffer::open(path.clone()).unwrap();
        assert_eq!(buffer.lines.len(), 2);
        assert!(buffer.trailing_newline);
        buffer.insert_char(1, 5, '!');
        buffer.save().unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello\nworld!\n");
        assert!(!buffer.dirty);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn selection_extracts_single_and_multiline_text() {
        let path = std::env::temp_dir().join(format!(
            "pico-selection-test-{}.txt",
            std::process::id()
        ));
        std::fs::write(&path, "hello\nworld").unwrap();
        let buffer = Buffer::open(path.clone()).unwrap();
        assert_eq!(
            buffer.selected_text(Pos::new(0, 2), Pos::new(0, 5)),
            "llo"
        );
        assert_eq!(
            buffer.selected_text(Pos::new(0, 0), Pos::new(1, 5)),
            "hello\nworld"
        );
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn save_appends_trailing_newline() {
        let path = std::env::temp_dir().join(format!("pico-eof-test-{}.txt", std::process::id()));
        std::fs::write(&path, "no newline").unwrap();
        let mut buffer = Buffer::open(path.clone()).unwrap();
        buffer.save().unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "no newline\n");

        std::fs::write(&path, "").unwrap();
        let mut empty = Buffer::open(path.clone()).unwrap();
        empty.save().unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn insert_text_handles_newlines() {
        let mut buffer = Buffer::empty();
        assert_eq!(buffer.insert_text(0, 0, "one\ntwo"), (1, 3));
        assert_eq!(buffer.as_text(), "one\ntwo");
    }
}
