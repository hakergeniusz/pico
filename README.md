# micro-rs

A deliberately small terminal text editor in Rust, inspired by
[Micro](https://github.com/micro-editor/micro). One file at a time, no command
bar, no Lua config, no custom keybindings, no plugins.

## Why

`micro` is pleasant but carries a lot of machinery most people never touch.
This is the boring subset: open a file, type, save, quit.

## Build and run

```sh
cargo build --release
./target/release/micro-rs path/to/file.txt
```

With no path it starts with an empty untitled buffer; Ctrl+S needs a path, so
pass one when you intend to save.

## Keys

| Key | Action |
|---|---|
| arrows | move cursor |
| Home / End | start / end of line |
| PageUp / PageDown | move by screenful |
| Enter | split line |
| Backspace | delete before cursor (joins lines at column 0) |
| Delete | delete at cursor |
| Tab | insert four spaces |
| Ctrl+S | save |
| Ctrl+Z | undo (last 200 edits) |
| Ctrl+Q / Esc | quit; press twice if there are unsaved changes |

## Limits

- One file per process, no open/save-as prompts, no command bar.
- Undo is a bounded snapshot stack, not a proper edit tree. No redo.
- No syntax highlighting, search, split panes, mouse, or clipboard integration.
- Very long lines are clipped to the terminal width; horizontal scrolling is not implemented.

## Tests

```sh
cargo test
```

Unit tests cover buffer edits and the open/save round trip, including a trailing
newline.
