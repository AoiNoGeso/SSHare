# SShare

SSH 接続したマシン間でファイルとテキストをリアルタイム共有する小さなデスクトップアプリ。

## 機能

- **ファイル共有**: ウィンドウにドラッグ＆ドロップするだけで相手側に転送
- **テキスト共有**: Ctrl+V (Mac は Cmd+V) でクリップボードのテキストを即送信
- **受信ファイル保存**: 「Save」ボタンで `~/Downloads/SShare/` に保存し Finder/Files が自動で開く
- **受信テキスト**: 「Copy」ボタンでクリップボードへ
- **常時最前面**: 小さいウィンドウが常にデスクトップ最前面に表示

## ビルド方法

### Mac (ローカル)

```bash
cargo build --release
cp target/release/sshare /usr/local/bin/   # 任意
```

### Ubuntu (リモート)

```bash
# システム依存ライブラリのインストール
sudo apt update
sudo apt install -y \
  libx11-dev libxcb1-dev libxrandr-dev libxi-dev \
  libxtst-dev libxfixes-dev libxcursor-dev \
  libxinerama-dev libssl-dev pkg-config \
  build-essential

# Rust がなければインストール
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# ビルド
cargo build --release
```

> **Wayland 環境の場合**: `arboard` は X11 が必要なため、
> Wayland セッションでも `DISPLAY=:0` を設定するか、
> XWayland を有効にしてください。

## 使い方

### 基本的な接続手順

1. **リモート (Ubuntu) でサーバーとして起動**:
   ```bash
   ./sshare --port 7878
   ```

2. **ローカル (Mac) でクライアントとして接続**:
   ```bash
   ./sshare --connect <UbuntuのIP>:7878
   ```
   SSH ポートフォワーディング経由の場合:
   ```bash
   # 別ターミナルで
   ssh -L 7878:localhost:7878 user@remote-host
   # その後
   ./sshare --connect localhost:7878
   ```

### 操作

| 操作 | 効果 |
|------|------|
| ウィンドウにファイルをドラッグ＆ドロップ | 相手側に転送 |
| Ctrl+V / Cmd+V | クリップボードのテキストを送信 |
| 受信アイテムの「Copy」 | クリップボードにコピー |
| 受信アイテムの「Save」 | `~/Downloads/SShare/` に保存 |
| 「Clear all」 | 表示リストをクリア |

## セキュリティ注意事項

通信は **暗号化されていません**。  
インターネット経由で使う場合は必ず SSH ポートフォワーディングを使ってください:

```bash
ssh -L 7878:localhost:7878 user@remote-host
```

## ファイルサイズ制限

プロトコル上の上限は 512 MB です。
