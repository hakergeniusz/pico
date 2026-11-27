#!/bin/sh
# Build an Arch .pkg.tar.zst package from a release binary.
# Requires tar with zstd support. Usage: sh scripts/build-arch.sh [version] [target]
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
  x86_64-*) arch=x86_64 ;;
  aarch64-*) arch=aarch64 ;;
  riscv64*) arch=riscv64 ;;
  *)
    echo "unsupported Arch architecture for target: $target" >&2
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
mkdir -p "$root/usr/bin" "$root/usr/share/licenses/pico" "$root/usr/share/doc/pico"
cp "$bin" "$root/usr/bin/pico"
chmod 0755 "$root/usr/bin/pico"
cp LICENSE "$root/usr/share/licenses/pico/LICENSE"
cp README.md "$root/usr/share/doc/pico/README.md"

size=$(du -sb "$root" | cut -f1)
cat > "$root/.PKGINFO" <<EOF
pkgname = pico
pkgver = $version-1
pkgdesc = Minimal terminal text editor inspired by Micro
builddate = $(date +%s)
packager = hakergeniusz <hakergeniusz@localhost>
size = $size
arch = $arch
license = MIT
EOF

archive="pico-$version-1-$arch.pkg.tar.zst"
tar --zstd -cf "$dist/$archive" -C "$root" .
(cd "$dist" && sha256sum "$archive" > "$archive.sha256")
echo "$dist/$archive"
