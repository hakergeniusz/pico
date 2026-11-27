#!/bin/sh
# Build a Fedora/RPM package from a release binary. Requires rpmbuild.
# Usage: sh scripts/build-rpm.sh [version] [target]
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
  x86_64-*) rpm_arch=x86_64 ;;
  aarch64-*) rpm_arch=aarch64 ;;
  riscv64*) rpm_arch=riscv64 ;;
  *)
    echo "unsupported RPM architecture for target: $target" >&2
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
mkdir -p "$root/SOURCES" "$root/SPECS" "$root/pico-$version"
cp "$bin" "$root/pico-$version/pico"
cp README.md LICENSE "$root/pico-$version/"
(cd "$root" && tar -czf "SOURCES/pico-$version.tar.gz" "pico-$version")

cat > "$root/SPECS/pico.spec" <<EOF
Name:           pico
Version:        $version
Release:        1%{?dist}
Summary:        Minimal terminal text editor inspired by Micro
License:        MIT
URL:            https://github.com/hakergeniusz/pico
Source0:        pico-$version.tar.gz
BuildArch:      $rpm_arch

%description
Single-file terminal editing with a line-number gutter, mouse selection,
copy/paste and a scrollbar. No command bar or custom keybindings.

%prep
%setup -q -n pico-$version

%build

%install
mkdir -p %{buildroot}%{_bindir}
install -m 0755 pico %{buildroot}%{_bindir}/pico

%files
%license LICENSE
%doc README.md
%{_bindir}/pico

%changelog
* Fri Nov 27 2026 hakergeniusz <hakergeniusz@localhost> - $version-1
- Initial package
EOF

rpmbuild --define "_topdir $root" -bb "$root/SPECS/pico.spec"
built=""
for candidate in "$root"/RPMS/*/*.rpm; do
  if [ -f "$candidate" ]; then
    built=$candidate
    break
  fi
done
[ -n "$built" ] || { echo "no rpm produced" >&2; exit 1; }
cp "$built" "$dist/"
archive=$(basename "$built")
(cd "$dist" && sha256sum "$archive" > "$archive.sha256")
echo "$dist/$archive"
