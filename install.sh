#!/bin/sh
# pico standalone installer for Linux, macOS and FreeBSD.
# Usage: curl -fsSL https://hakergeniusz.github.io/pico/install.sh | sh
# Env: PICO_VERSION (release tag), PICO_BASE_URL (override release base),
#      PICO_INSTALL_DIR (default: $HOME/.local/bin).
set -eu

say() { printf '%s\n' "$*"; }
die() { printf 'pico installer: %s\n' "$*" >&2; exit 1; }

fetch() {
  if [ -n "${PICO_TOKEN:-}" ]; then
    curl -fsSL -H "Authorization: Bearer $PICO_TOKEN" "$1"
  else
    curl -fsSL "$1"
  fi
}

repo=hakergeniusz/pico
default_base="https://github.com/$repo/releases/download"
base=${PICO_BASE_URL:-$default_base}
install_dir=${PICO_INSTALL_DIR:-$HOME/.local/bin}

os=$(uname -s)
case "$os" in
  Linux) platform=unknown-linux-gnu ;;
  Darwin) platform=apple-darwin ;;
  FreeBSD) platform=unknown-freebsd ;;
  MINGW*|MSYS*|CYGWIN*) die "Windows uses the .zip asset, not this shell installer" ;;
  *) die "unsupported operating system: $os" ;;
esac

machine=$(uname -m)
case "$machine" in
  x86_64|amd64) target="x86_64-$platform" ;;
  aarch64|arm64) target="aarch64-$platform" ;;
  riscv64) target="riscv64gc-$platform" ;;
  *) die "unsupported CPU architecture: $machine" ;;
esac

tag=${PICO_VERSION:-}
if [ -z "$tag" ]; then
  tag=$(fetch "https://api.github.com/repos/$repo/releases/latest" |
    sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)
  [ -n "$tag" ] || die "no releases found for $repo; publish a v* release or set PICO_VERSION"
fi
version=${tag#v}

if [ "$base" = "$default_base" ]; then
  download_base="$base/$tag"
else
  download_base="$base"
fi

archive="pico-$version-$target.tar.gz"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "Downloading pico $version for $target"
fetch "$download_base/$archive" > "$tmp/$archive"
fetch "$download_base/$archive.sha256" > "$tmp/$archive.sha256"

if command -v sha256sum >/dev/null 2>&1; then
  (cd "$tmp" && sha256sum -c "$archive.sha256") || die "checksum verification failed"
elif command -v shasum >/dev/null 2>&1; then
  (cd "$tmp" && shasum -a 256 -c "$archive.sha256") || die "checksum verification failed"
else
  die "need sha256sum or shasum to verify the download"
fi

tar -xzf "$tmp/$archive" -C "$tmp"
mkdir -p "$install_dir"
install -m 0755 "$tmp/pico-$version-$target/pico" "$install_dir/pico"

say "Installed $install_dir/pico"
case ":$PATH:" in
  *":$install_dir:"*) ;;
  *) say "Add $install_dir to your PATH to use pico" ;;
esac
