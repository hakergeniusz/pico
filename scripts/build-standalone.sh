#!/bin/sh
# Build a standalone release archive for one Rust target.
# Usage: sh scripts/build-standalone.sh [version] [target]
# Env: PICO_BIN (use an already-built binary), PICO_DIST_DIR, CARGO_TARGET_DIR.
set -eu

version=${1:-}
target=${2:-}

read_version() { sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1; }
if [ -z "$version" ]; then
  version=$(read_version)
fi
case "$version" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) version=$(read_version) ;;
esac
if [ -z "$target" ]; then
  target=$(rustc -vV | sed -n 's/^host: //p')
fi

dist=${PICO_DIST_DIR:-dist}
mkdir -p "$dist"
name="pico-${version}-${target}"
stage="$dist/$name"
mkdir -p "$stage"

bin=${PICO_BIN:-}
if [ -z "$bin" ]; then
  cargo build --release --target "$target"
  target_dir=$(cargo metadata --format-version 1 --no-deps 2>/dev/null |
    sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p' | head -n 1)
  [ -n "$target_dir" ] || target_dir=${CARGO_TARGET_DIR:-target}
  bin="$target_dir/$target/release/pico"
  case "$target" in
    *windows*) bin="$bin.exe" ;;
  esac
fi

case "$target" in
  *windows*)
    cp "$bin" "$stage/pico.exe"
    cp README.md LICENSE "$stage/"
    (cd "$dist" && zip -qr "$name.zip" "$name")
    archive="$name.zip"
    ;;
  *)
    cp "$bin" "$stage/pico"
    cp README.md LICENSE "$stage/"
    (cd "$dist" && tar -czf "$name.tar.gz" "$name")
    archive="$name.tar.gz"
    ;;
esac

(cd "$dist" && sha256sum "$archive" > "$archive.sha256")
echo "$dist/$archive"
