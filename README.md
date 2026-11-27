# pico

A deliberately small terminal text editor in Rust, inspired by
[Micro](https://github.com/micro-editor/micro). One file at a time, line numbers
on the left, a scrollbar on the right, mouse highlighting, copy/paste, light
syntax coloring, auto-closing brackets and quotes, no command bar, no Lua config,
no custom keybindings, no plugins.

## Why

`micro` is pleasant but carries a lot of machinery most people never touch.
This is the boring subset: open a file, type, save, quit.

## Build and run

```sh
cargo build --release
./target/release/pico path/to/file.txt
```

With no path it starts with an empty untitled buffer; Ctrl+S needs a path, so
pass one when you intend to save.

## Install with a one-liner

```sh
curl -fsSL https://hakergeniusz.github.io/pico/install.sh | sh
```

The installer puts `pico` in `~/.local/bin` by default. Override with
`PICO_INSTALL_DIR`, pin a release with `PICO_VERSION=0.1.0`, or point at a
different asset host with `PICO_BASE_URL`. It verifies the SHA-256 checksum
before installing. `PICO_TOKEN` is only needed for private forks.

On Windows, use PowerShell:

```powershell
irm https://hakergeniusz.github.io/pico/install.ps1 | iex
```

`install.ps1` installs `pico.exe` to `$env:LOCALAPPDATA\pico\bin` by default.
Pass `-Version`, `-InstallDir`, `-BaseUrl`, or `-Token` (environment variables
`PICO_VERSION`, `PICO_INSTALL_DIR`, `PICO_BASE_URL`, `PICO_TOKEN` also work).
It verifies the SHA-256 checksum before extracting the zip.

## Packages and release assets

A tagged release (`v*`) is built by `.github/workflows/release.yml` and contains:

| Artifact | Targets |
|---|---|
| standalone `.tar.gz` + `.sha256` | Linux x86_64, aarch64, riscv64; macOS x86_64, arm64; FreeBSD arm64 |
| standalone `.zip` + `.sha256` | Windows x86_64 |
| Debian `.deb` | amd64, arm64, riscv64 |
| Fedora `.rpm` | x86_64, aarch64, riscv64 |
| Arch `.pkg.tar.zst` | x86_64, aarch64, riscv64 |

Local builders live in `scripts/`: `build-standalone.sh`, `build-deb.sh`
(needs `dpkg-deb`), `build-rpm.sh` (needs `rpmbuild`), and `build-arch.sh`
(needs `tar --zstd`). An Arch `PKGBUILD` template is in `packaging/arch/`.
Targets other than Linux x86_64 are built in CI only and are not locally tested.

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
| Ctrl+S | save (with automatic backup and trailing newline) |
| Ctrl+Z | undo (last 200 edits) |
| Ctrl+R | restore the newest backup into the buffer |
| Mouse wheel | scroll three lines at a time |
| Click/drag the right scrollbar | jump to a position in the file |
| Click a line number | highlight the entire line |
| Drag across text | select a range of characters |
| Ctrl+C | copy the selection |
| Ctrl+V | paste from the editor's internal clipboard |
| Ctrl+Q / Esc | quit; press twice if there are unsaved changes |

## Highlighting and auto-close

Selecting with the mouse only highlights text. The status line shows how many
characters are selected and reminds you to press Ctrl+C; nothing is copied
until you do. Brackets and quotes (`()`, `[]`, `{}`, `""`, `''`, `` `` ``) close
automatically when typed, and typing a closing character over an existing one
steps over it instead of inserting a duplicate.

Syntax coloring is lightweight and heuristic: comments, strings, numbers and
common keywords are colored, with a few language-specific keyword sets and
basic Markdown styling. It is not a full parser and does not replace a real
syntax plugin.

## Backups and EOF newline

Before every save of an existing file, pico copies the current on-disk version
to `$XDG_DATA_HOME/pico/backups/<hash>/` (or `~/.local/share/pico/backups/` when
`XDG_DATA_HOME` is unset). The ten newest timestamped copies are kept per file,
alongside a `path.txt` marker. If the backup fails, the save is aborted so the
last good copy is never overwritten blindly. First saves of new files do not
need a backup.

Ctrl+R loads the newest backup into the buffer for the current file. It does
not write anything by itself: review the buffer and press Ctrl+S to keep it.
Ctrl+Z undoes a restore. If the on-disk file is so damaged that pico cannot
open it as UTF-8, move it aside, create an empty file with the same name, open
that, then press Ctrl+R to pull the last good copy back.

Saves also add a trailing newline when the file does not already have one.
Empty files stay empty.

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

Unit tests cover buffer edits, selection, scroll clamping, scrollbar thumb
bounds and the open/save round trip, including a trailing newline.

## License

MIT. See `LICENSE`.
