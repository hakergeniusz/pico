# micro-rs

A deliberately small terminal text editor in Rust, inspired by
[Micro](https://github.com/micro-editor/micro). One file at a time, line numbers
on the left, mouse highlighting, copy/paste, no command bar, no Lua config, no
custom keybindings, no plugins.

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
| Click a line number | highlight the entire line |
| Drag across text | select a range of characters |
| Ctrl+C | copy the selection |
| Ctrl+V | paste from the editor's internal clipboard |
| Ctrl+Q / Esc | quit; press twice if there are unsaved changes |

## Clipboard notes

Ctrl+C writes the selection to the terminal clipboard with an OSC52 escape
sequence, so terminals that support OSC52 (including many SSH setups) can paste
it into other apps. In tmux you may need `set -g allow-passthrough on` for the
sequence to reach the outer terminal. Ctrl+V pastes from the editor's own
internal clipboard, because OSC52 cannot read the system clipboard.

## Limits

- One file per process, no open/save-as prompts, no command bar.
- Undo is a bounded snapshot stack, not a proper edit tree. No redo.
- No syntax highlighting, search, split panes, cut, or double-click word selection.
- Editing while text is selected clears the selection instead of replacing it.
- Very long lines are clipped to the terminal width; horizontal scrolling is not implemented.

## Tests

```sh
cargo test
```

Unit tests cover buffer edits and the open/save round trip, including a trailing
newline.
