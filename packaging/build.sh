#!/usr/bin/env bash
# Build the release binaries and the three Linux packages into dist/.
#
#   packaging/build.sh            # .deb + .rpm + AppImage
#   packaging/build.sh deb rpm    # a subset
#
# Needs: cargo, cargo-deb, cargo-generate-rpm (cargo install cargo-deb cargo-generate-rpm),
# and for the AppImage either `appimagetool` on PATH or network access to fetch it once
# into packaging/.tools/. Nothing here needs root, and nothing is installed.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
VERSION="$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/')"
ARCH="$(uname -m)"
DIST="$ROOT/dist"
TOOLS="$ROOT/packaging/.tools"
mkdir -p "$DIST" "$TOOLS"

targets=("$@")
[ ${#targets[@]} -eq 0 ] && targets=(deb rpm appimage)

echo "== cleanator $VERSION ($ARCH): building release binaries"
cargo build --release --workspace
cargo test --release --workspace --quiet

for t in "${targets[@]}"; do
  case "$t" in
    deb)
      echo "== .deb"
      cargo deb -p cleanator --no-build --quiet
      cp target/debian/cleanator_"$VERSION"-*.deb "$DIST/"
      ;;
    rpm)
      echo "== .rpm"
      cargo generate-rpm -p crates/app
      cp target/generate-rpm/cleanator-"$VERSION"-*.rpm "$DIST/"
      ;;
    appimage)
      echo "== AppImage"
      TOOL="$(command -v appimagetool || true)"
      if [ -z "$TOOL" ]; then
        TOOL="$TOOLS/appimagetool-$ARCH"
        if [ ! -x "$TOOL" ]; then
          curl -sSL -o "$TOOL" \
            "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
          chmod +x "$TOOL"
        fi
      fi
      APPDIR="$ROOT/target/appimage/Cleanator.AppDir"
      rm -rf "$APPDIR"
      mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" \
        "$APPDIR/usr/share/icons/hicolor/256x256/apps" "$APPDIR/usr/share/icons/hicolor/scalable/apps"
      cp target/release/cleanator target/release/cleanator-cli "$APPDIR/usr/bin/"
      cp assets/cleanator.desktop "$APPDIR/usr/share/applications/"
      cp assets/cleanator.desktop "$APPDIR/cleanator.desktop"
      cp assets/icon-256.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/cleanator.png"
      cp assets/icon.svg "$APPDIR/usr/share/icons/hicolor/scalable/apps/cleanator.svg"
      cp assets/icon-256.png "$APPDIR/cleanator.png"
      cp assets/icon-256.png "$APPDIR/.DirIcon"
      ln -sf usr/bin/cleanator "$APPDIR/AppRun"
      # appimagetool is itself an AppImage; extract-and-run avoids needing FUSE.
      APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$ARCH" "$TOOL" --no-appstream \
        "$APPDIR" "$DIST/Cleanator-$VERSION-$ARCH.AppImage" 2>&1 | grep -v '^$' | tail -3
      chmod +x "$DIST/Cleanator-$VERSION-$ARCH.AppImage"
      ;;
    *)
      echo "unknown target: $t (deb, rpm, appimage)" >&2
      exit 2
      ;;
  esac
done

echo
echo "== dist/"
ls -la "$DIST"
(cd "$DIST" && sha256sum ./*) | tee "$DIST/SHA256SUMS"
