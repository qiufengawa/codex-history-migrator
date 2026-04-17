#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:?version is required}"
BINARY_PATH="${2:?path to release binary is required}"
DIST_DIR="${3:-artifacts/release}"
APPIMAGETOOL_BIN="${APPIMAGETOOL:-appimagetool}"
APP_NAME="Codex History Migrator"
APP_ID="codex-history-migrator"
APPDIR="$DIST_DIR/AppDir"
TARBALL_ROOT="$DIST_DIR/codex-history-migrator-v$VERSION-linux-x86_64"

rm -rf "$APPDIR" "$TARBALL_ROOT"
mkdir -p "$APPDIR/usr/bin" \
         "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/icons/hicolor/256x256/apps" \
         "$TARBALL_ROOT"

cp "$BINARY_PATH" "$APPDIR/usr/bin/codex-history-migrator"
chmod +x "$APPDIR/usr/bin/codex-history-migrator"

cat > "$APPDIR/AppRun" <<'EOF'
#!/usr/bin/env bash
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$HERE/usr/bin/codex-history-migrator" "$@"
EOF
chmod +x "$APPDIR/AppRun"

cat > "$APPDIR/$APP_ID.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Codex History Migrator
Exec=codex-history-migrator
Icon=codex-history-migrator
Categories=Utility;Development;
Terminal=false
EOF

cp "$APPDIR/$APP_ID.desktop" "$APPDIR/usr/share/applications/$APP_ID.desktop"
cp "assets/app-icon.png" "$APPDIR/$APP_ID.png"
cp "assets/app-icon.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/$APP_ID.png"

cp "$BINARY_PATH" "$TARBALL_ROOT/codex-history-migrator"
cp "README.md" "$TARBALL_ROOT/README.md"
cp "LICENSE" "$TARBALL_ROOT/LICENSE"
tar -czf "$DIST_DIR/codex-history-migrator-v$VERSION-linux-x86_64.tar.gz" \
  -C "$DIST_DIR" \
  "$(basename "$TARBALL_ROOT")"

ARCH=x86_64 "$APPIMAGETOOL_BIN" \
  "$APPDIR" \
  "$DIST_DIR/codex-history-migrator-v$VERSION-linux-x86_64.AppImage"
