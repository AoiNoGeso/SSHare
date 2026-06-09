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

# Icon: convert SVG → icns if rsvg-convert + iconutil are available
if command -v rsvg-convert &>/dev/null; then
  ICONSET="$RESOURCES/SShare.iconset"
  mkdir -p "$ICONSET"
  for size in 16 32 64 128 256 512; do
    rsvg-convert -w $size -h $size assets/icon.svg -o "$ICONSET/icon_${size}x${size}.png"
    rsvg-convert -w $((size*2)) -h $((size*2)) assets/icon.svg -o "$ICONSET/icon_${size}x${size}@2x.png"
  done
  iconutil -c icns -o "$RESOURCES/SShare.icns" "$ICONSET"
  rm -rf "$ICONSET"
  ICON_KEY='    <key>CFBundleIconFile</key>\n    <string>SShare</string>'
  sed -i '' "s|<key>NSSupportsAutomaticGraphicsSwitching</key>|${ICON_KEY}\n    <key>NSSupportsAutomaticGraphicsSwitching</key>|" "$CONTENTS/Info.plist"
  echo "アイコン生成完了 (rsvg-convert 使用)"
else
  echo "ヒント: brew install librsvg でアイコンを自動生成できます"
fi

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
