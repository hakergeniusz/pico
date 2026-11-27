use ratatui::style::{Color, Modifier, Style};
use std::path::Path;

const COMMON: &[&str] = &[
    "if", "else", "for", "while", "return", "const", "static", "class", "fn", "def", "import",
    "from", "use", "var", "let", "new", "this", "self", "self", "true", "false", "None", "True",
    "False", "null", "nil", "try", "catch", "except", "finally", "match", "switch", "case",
];

const RUST: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "false",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
    "use", "where", "while",
];

const PYTHON: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif",
    "else", "except", "False", "finally", "for", "from", "global", "if", "import", "in", "is",
    "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "self", "True", "try",
    "while", "with", "yield",
];

const JS: &[&str] = &[
    "async", "await", "break", "case", "catch", "class", "const", "continue", "default", "delete",
    "else", "export", "extends", "false", "finally", "for", "from", "function", "if", "import",
    "in", "instanceof", "let", "new", "null", "of", "return", "super", "switch", "this", "throw",
    "true", "try", "typeof", "undefined", "var", "while",
];

const SHELL: &[&str] = &[
    "case", "do", "done", "elif", "else", "esac", "fi", "for", "function", "if", "in", "local",
    "then", "while",
];

pub fn extension(path: Option<&Path>) -> String {
    path.and_then(Path::extension)
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn keywords(ext: &str) -> &'static [&'static str] {
    match ext {
        "rs" => RUST,
        "py" | "pyi" => PYTHON,
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" => JS,
        "sh" | "bash" | "zsh" | "fish" => SHELL,
        _ => COMMON,
    }
}

fn comment_marker(ext: &str) -> Option<&'static str> {
    match ext {
        "py" | "sh" | "bash" | "zsh" | "fish" | "yaml" | "yml" | "toml" | "ini" | "conf" => {
            Some("#")
        }
        "lua" | "sql" | "hs" => Some("--"),
        _ => Some("//"),
    }
}

fn markdown_styles(line: &[char], styles: &mut [Style]) {
    let mut index = 0;
    while index < line.len() {
        if line[index] == '#' && (index == 0 || line[index - 1].is_whitespace()) {
            let style = Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD);
            for cell in &mut styles[index..] {
                *cell = style;
            }
            return;
        }
        if line[index] == '*' && index + 1 < line.len() && line[index + 1] == '*' {
            if let Some(end) = find_pair(line, index + 2, "**") {
                let style = Style::default().add_modifier(Modifier::BOLD);
                for cell in &mut styles[index..end + 2] {
                    *cell = style;
                }
                index = end + 2;
                continue;
            }
        }
        if line[index] == '`' {
            if let Some(end) = find_pair(line, index + 1, "`") {
                let style = Style::default().fg(Color::Yellow);
                for cell in &mut styles[index..=end] {
                    *cell = style;
                }
                index = end + 1;
                continue;
            }
        }
        if line[index] == '[' {
            if let Some(end) = find_pair(line, index + 1, "](") {
                let style = Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::UNDERLINED);
                for cell in &mut styles[index..end + 2] {
                    *cell = style;
                }
                index = end + 2;
                continue;
            }
        }
        index += 1;
    }
}

fn find_pair(line: &[char], start: usize, marker: &str) -> Option<usize> {
    let chars: Vec<char> = marker.chars().collect();
    if start + chars.len() > line.len() {
        return None;
    }
    (start..=line.len().saturating_sub(chars.len()))
        .find(|&at| line[at..at + chars.len()] == chars[..])
}

pub fn styles_for_line(line: &[char], ext: &str) -> Vec<Style> {
    let mut styles = vec![Style::default(); line.len()];
    if line.is_empty() {
        return styles;
    }
    if ext == "md" || ext == "markdown" || ext == "txt" {
        if ext != "txt" {
            markdown_styles(line, &mut styles);
        }
        return styles;
    }

    let words = keywords(ext);
    let marker: Vec<char> = comment_marker(ext).unwrap_or("//").chars().collect();
    let comments = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::ITALIC);
    let strings = Style::default().fg(Color::LightGreen);
    let numbers = Style::default().fg(Color::LightCyan);
    let keyword_style = Style::default().fg(Color::LightMagenta);

    let mut index = 0;
    while index < line.len() {
        if index + marker.len() <= line.len() && line[index..index + marker.len()] == marker[..] {
            for cell in &mut styles[index..] {
                *cell = comments;
            }
            break;
        }
        let ch = line[index];
        if ch == '"' || ch == '\'' || ch == '`' {
            let start = index;
            index += 1;
            while index < line.len() {
                if line[index] == '\\' {
                    index = (index + 2).min(line.len());
                    continue;
                }
                if line[index] == ch {
                    index += 1;
                    break;
                }
                index += 1;
            }
            for cell in &mut styles[start..index] {
                *cell = strings;
            }
            continue;
        }
        if ch.is_ascii_digit() {
            let start = index;
            while index < line.len()
                && (line[index].is_ascii_alphanumeric() || line[index] == '.' || line[index] == '_')
            {
                index += 1;
            }
            for cell in &mut styles[start..index] {
                *cell = numbers;
            }
            continue;
        }
        if ch.is_alphabetic() || ch == '_' {
            let start = index;
            while index < line.len() && (line[index].is_alphanumeric() || line[index] == '_') {
                index += 1;
            }
            let word: String = line[start..index].iter().collect();
            if words.contains(&word.as_str()) {
                for cell in &mut styles[start..index] {
                    *cell = keyword_style;
                }
            }
            continue;
        }
        index += 1;
    }
    styles
}
