#!/bin/sh
# Build a Debian .deb from a release binary. Requires dpkg-deb.
# Usage: sh scripts/build-deb.sh [version] [target]
# Env: PICO_BIN, PICO_DIST_DIR, CARGO_TARGET_DIR.
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

case "$target" in
  x86_64-*) deb_arch=amd64 ;;
  aarch64-*) deb_arch=arm64 ;;
  riscv64*) deb_arch=riscv64 ;;
  *)
    echo "unsupported Debian architecture for target: $target" >&2
    exit 1
    ;;
esac

dist=${PICO_DIST_DIR:-dist}
mkdir -p "$dist"

bin=${PICO_BIN:-}
if [ -z "$bin" ]; then
  cargo build --release --target "$target"
  target_dir=$(cargo metadata --format-version 1 --no-deps 2>/dev/null |
    sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p' | head -n 1)
  [ -n "$target_dir" ] || target_dir=${CARGO_TARGET_DIR:-target}
  bin="$target_dir/$target/release/pico"
fi

root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT INT TERM
mkdir -p "$root/DEBIAN" "$root/usr/bin" "$root/usr/share/doc/pico"
cp "$bin" "$root/usr/bin/pico"
chmod 0755 "$root/usr/bin/pico"
cp README.md LICENSE "$root/usr/share/doc/pico/"

cat > "$root/DEBIAN/control" <<EOF
Package: pico
Version: $version
Section: utils
Priority: optional
Architecture: $deb_arch
Maintainer: hakergeniusz <hakergeniusz@localhost>
Description: minimal terminal text editor inspired by Micro
 Single-file terminal editing with a line-number gutter, mouse selection,
 copy/paste and a scrollbar. No command bar or custom keybindings.
EOF

archive="pico_${version}_${deb_arch}.deb"
dpkg-deb --build --root-owner-group "$root" "$dist/$archive"
(cd "$dist" && sha256sum "$archive" > "$archive.sha256")
echo "$dist/$archive"
