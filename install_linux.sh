#!/bin/bash
set -e

INSTALL_DIR="$HOME/.local/bin"
ICON_DIR="$HOME/.local/share/icons/hicolor/scalable/apps"
DESKTOP_DIR="$HOME/.local/share/applications"

echo "Building release binary..."
cargo build --release

mkdir -p "$INSTALL_DIR" "$ICON_DIR" "$DESKTOP_DIR"

# Binary
cp target/release/sshare "$INSTALL_DIR/sshare"
chmod +x "$INSTALL_DIR/sshare"

# Icon
cp assets/icon.svg "$ICON_DIR/sshare.svg"

# .desktop file
cat > "$DESKTOP_DIR/sshare.desktop" << DESKTOP
[Desktop Entry]
Type=Application
Name=SShare
Comment=SSH接続したマシン間でファイルとテキストを共有
Exec=$INSTALL_DIR/sshare
Icon=sshare
Categories=Utility;Network;
StartupNotify=false
NoDisplay=false
DESKTOP

# Refresh desktop database
update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

echo ""
echo "インストール完了"
echo "  バイナリ : $INSTALL_DIR/sshare"
echo "  アイコン : $ICON_DIR/sshare.svg"
echo "  ランチャー: $DESKTOP_DIR/sshare.desktop"
echo ""
echo "アプリランチャーから「SShare」で検索して起動できます。"
echo "ターミナルからは: sshare"
