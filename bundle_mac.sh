#!/bin/bash
set -e

APP="SShare.app"
CONTENTS="$APP/Contents"
MACOS="$CONTENTS/MacOS"
RESOURCES="$CONTENTS/Resources"

echo "Building release binary..."
cargo build --release

rm -rf "$APP"
mkdir -p "$MACOS" "$RESOURCES"

cp target/release/sshare "$MACOS/SShare"
chmod +x "$MACOS/SShare"

cat > "$CONTENTS/Info.plist" << 'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>SShare</string>
    <key>CFBundleIdentifier</key>
    <string>com.sshare.app</string>
    <key>CFBundleName</key>
    <string>SShare</string>
    <key>CFBundleDisplayName</key>
    <string>SShare</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleVersion</key>
    <string>0.1.0</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.15</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSUIElement</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
</dict>
</plist>
PLIST

echo ""
echo "✓ Created $APP"
echo ""
echo "インストール: cp -r $APP /Applications/"
echo "起動:         open $APP"
echo ""
echo "初回起動時にセットアップ画面が表示されます。"
echo "「ログイン時に自動起動」にチェックを入れると次回ログインから自動起動します。"
