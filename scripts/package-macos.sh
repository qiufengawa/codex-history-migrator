#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:?version is required}"
UNIVERSAL_BINARY="${2:?path to universal binary is required}"
DIST_DIR="${3:-artifacts/release}"
APP_NAME="Codex History Migrator"
APP_BUNDLE="$DIST_DIR/$APP_NAME.app"
CONTENTS_DIR="$APP_BUNDLE/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

cp "$UNIVERSAL_BINARY" "$MACOS_DIR/codex-history-migrator"
chmod +x "$MACOS_DIR/codex-history-migrator"
cp "assets/app-icon.png" "$RESOURCES_DIR/app-icon.png"

cat > "$CONTENTS_DIR/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>Codex History Migrator</string>
  <key>CFBundleExecutable</key>
  <string>codex-history-migrator</string>
  <key>CFBundleIdentifier</key>
  <string>com.qiufengawa.codex-history-migrator</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>Codex History Migrator</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>__VERSION__</string>
  <key>CFBundleVersion</key>
  <string>__VERSION__</string>
  <key>LSMinimumSystemVersion</key>
  <string>12.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
EOF

python - <<PY
from pathlib import Path
info = Path(r"$CONTENTS_DIR/Info.plist")
info.write_text(info.read_text(encoding="utf-8").replace("__VERSION__", "$VERSION"), encoding="utf-8")
PY

ditto -c -k --sequesterRsrc --keepParent "$APP_BUNDLE" \
  "$DIST_DIR/codex-history-migrator-v$VERSION-macos-universal2.zip"

hdiutil create \
  -volname "$APP_NAME" \
  -srcfolder "$APP_BUNDLE" \
  -ov \
  -format UDZO \
  "$DIST_DIR/codex-history-migrator-v$VERSION-macos-universal2.dmg"
