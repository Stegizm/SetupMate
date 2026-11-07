#!/usr/bin/env bash
# Builds SetupMate as an AppImage.
#
# Requirements: Rust (rustup), curl, and either appimagetool on PATH or network
# access to download it. Output goes to dist/ by default.
#
# Usage:
#   ./packaging/build-appimage.sh
#   SETUPMATE_VERSION=0.1.0 OUT_DIR=dist ./packaging/build-appimage.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARCH="${ARCH:-$(uname -m)}"
VERSION="${SETUPMATE_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/crates/setupmate-gui/Cargo.toml" | head -n1)}"
OUT_DIR="${OUT_DIR:-$ROOT/dist}"
APPDIR="$OUT_DIR/SetupMate.AppDir"
TOOL="${APPIMAGETOOL:-$OUT_DIR/appimagetool-$ARCH.AppImage}"
ASSET="$OUT_DIR/SetupMate-$VERSION-$ARCH.AppImage"

if [ -z "$VERSION" ]; then
    echo "Sürüm belirlenemedi. SETUPMATE_VERSION ayarla." >&2
    exit 1
fi

mkdir -p "$OUT_DIR"
rm -rf "$APPDIR"
mkdir -p \
    "$APPDIR/usr/bin" \
    "$APPDIR/usr/share/applications" \
    "$APPDIR/usr/share/icons/hicolor/scalable/apps"

echo "==> Rust release derlemesi"
cargo build --release --locked -p setupmate-gui

install -Dm755 "$ROOT/target/release/setupmate-gui" "$APPDIR/usr/bin/setupmate"
install -Dm644 "$ROOT/packaging/setupmate.desktop" \
    "$APPDIR/usr/share/applications/setupmate.desktop"
install -Dm644 "$ROOT/packaging/setupmate.svg" \
    "$APPDIR/usr/share/icons/hicolor/scalable/apps/setupmate.svg"

# AppImage root-level metadata.
cp "$APPDIR/usr/share/applications/setupmate.desktop" "$APPDIR/setupmate.desktop"
cp "$APPDIR/usr/share/icons/hicolor/scalable/apps/setupmate.svg" "$APPDIR/setupmate.svg"
ln -sf setupmate.svg "$APPDIR/.DirIcon"

cat > "$APPDIR/AppRun" <<'APPRUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/setupmate" "$@"
APPRUN
chmod +x "$APPDIR/AppRun"

if [ ! -x "$TOOL" ]; then
    echo "==> appimagetool indiriliyor ($ARCH)"
    curl -fsSL --retry 3 --connect-timeout 20 \
        -o "$TOOL" \
        "https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-$ARCH.AppImage"
    chmod +x "$TOOL"
fi

echo "==> AppImage oluşturuluyor: $ASSET"
ARCH="$ARCH" "$TOOL" --appimage-extract-and-run "$APPDIR" "$ASSET"

(cd "$OUT_DIR" && sha256sum "$(basename "$ASSET")" > SHA256SUMS)

echo
echo "Hazır:"
echo "  $ASSET"
echo "  $OUT_DIR/SHA256SUMS"
