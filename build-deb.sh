#!/usr/bin/env bash
# Build a single .deb for Debian/Ubuntu/Pop!_OS — no cargo-deb, no debhelper.
# Usage (from repo root):  ./scripts/build-deb.sh
# Or:                        make package

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

need() { command -v "$1" >/dev/null 2>&1 || { echo "missing: $1" >&2; exit 1; }; }

need cargo
need dpkg-deb

cargo build --release

VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"

if command -v dpkg-architecture >/dev/null 2>&1; then
  ARCH="$(dpkg-architecture -qDEB_HOST_ARCH)"
else
  case "$(uname -m)" in
    x86_64) ARCH=amd64 ;;
    aarch64) ARCH=arm64 ;;
    *) ARCH="$(uname -m)" ;;
  esac
fi

OUT_DIR="$ROOT/dist"
mkdir -p "$OUT_DIR"

STAGE="$(mktemp -d "${TMPDIR:-/tmp}/ctk-deb.XXXXXX")"
cleanup() { rm -rf "$STAGE"; }
trap cleanup EXIT

mkdir -p "$STAGE/DEBIAN" "$STAGE/usr/bin" "$STAGE/usr/share/applications"

install -Dm755 "$ROOT/target/release/cosmic-touch-keyboard" "$STAGE/usr/bin/cosmic-touch-keyboard"
install -Dm644 "$ROOT/cosmic-touch-keyboard.desktop" "$STAGE/usr/share/applications/cosmic-touch-keyboard.desktop"

cat >"$STAGE/DEBIAN/control" <<EOF
Package: cosmic-touch-keyboard
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${ARCH}
Maintainer: Satya Neriyanuru <cosmic-touch-keyboard@local>
Homepage: https://github.com/pop-os/libcosmic
Depends: libc6, libwayland-client0, libxkbcommon0
Description: Touch on-screen keyboard for COSMIC (Wayland)
 Native Wayland layer-shell on-screen keyboard. Optional auto-show when
 text fields request zwp_input_method_v2.
EOF

DEB_OUT="$OUT_DIR/cosmic-touch-keyboard_${VERSION}_${ARCH}.deb"
rm -f "$DEB_OUT"

# -Zxz: reproducible-ish smaller archive; --root-owner-group: owned root:root without fakeroot if root
dpkg-deb --build --root-owner-group -Zxz "$STAGE" "$DEB_OUT"

echo "Built: $DEB_OUT"
echo "Install: sudo apt install \"$DEB_OUT\""
