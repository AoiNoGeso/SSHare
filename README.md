# SSHare

SSH 接続したマシン間でファイルとテキストをリアルタイム共有する小さなデスクトップアプリ。

## 機能

- **ファイル共有**: ウィンドウにドラッグ＆ドロップするだけで相手側に転送
- **テキスト共有**: Cmd+V でクリップボードのテキストを即送信
- **受信ファイル保存**: 「Save」ボタンで `~/Downloads/SShare/` に保存し Finder が自動で開く
- **受信テキスト**: 「Copy」ボタンでクリップボードへ
- **Dock スタイルのウィンドウ**: 左下にカーソルを寄せると滑らかにスライドアップ表示
- **初回セットアップ GUI**: 初回起動時にサーバー / クライアントをワンクリックで選択
- **mDNS 自動検出**: 同じ LAN 上のサーバーをセットアップ画面で自動リスト表示
- **ログイン自動起動**: セットアップ時に設定すれば次回ログインから自動起動

---

## セットアップ手順

### ローカル側 (Mac)

#### 1. ビルドと .app 化

```bash
git clone https://github.com/AoiNoGeso/SSHare.git
cd SSHare

# .app バンドルを生成 (内部で cargo build --release を実行)
./bundle_mac.sh

# アプリをインストール
cp -r SShare.app /Applications/
```

#### 2. 初回起動とセットアップ

```bash
open /Applications/SShare.app
```

初回起動時にセットアップ画面が開きます。

- **モード選択**: 「クライアント」を選択
- **接続先**: Ubuntu 側の IP アドレスとポートを入力（例: `192.168.x.x:7878`）  
  SSH ポートフォワーディングを使う場合は `localhost:7878`
- **ログイン時に自動起動**: チェックを入れると次のログインから自動起動
- 「確定」をクリック → 左下に常駐ウィンドウが表示されます

> 設定は `~/.config/SShare/config.json` に保存されます。  
> 再設定する場合はこのファイルを削除して再起動してください。

---

### リモート側 (Ubuntu)

#### 1. 依存ライブラリのインストール

```bash
sudo apt update
sudo apt install -y \
  libx11-dev libxcb1-dev libxrandr-dev libxi-dev \
  libxtst-dev libxfixes-dev libxcursor-dev \
  libxinerama-dev libssl-dev pkg-config \
  build-essential
```

#### 2. Rust のインストール（未インストールの場合）

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

#### 3. ビルドとインストール

```bash
git clone https://github.com/AoiNoGeso/SSHare.git
cd SSHare
./install_linux.sh
```

インストールスクリプトが以下を自動で行います:
- `~/.local/bin/sshare` にバイナリをインストール
- `~/.local/share/icons/` に SVG アイコンを配置
- `~/.local/share/applications/sshare.desktop` を作成 → アプリランチャーに登録

インストール後はアプリランチャー（GNOME の「アクティビティ」など）で **SShare** と検索して起動できます。

#### 4. 初回起動とセットアップ

アプリランチャーまたはターミナルから起動します:

```bash
sshare
```

初回起動時にセットアップ画面が開きます。

- **モード選択**: 「サーバー」を選択
- **ポート番号**: デフォルト `7878`（変更する場合は Mac 側と合わせる）
- **ログイン時に自動起動**: チェックを入れると `~/.config/autostart/sshare.desktop` が更新されます
- 「確定」をクリック → 左下に常駐ウィンドウが表示されます

> **Wayland 環境の場合**: `arboard` は X11 が必要です。  
> `DISPLAY=:0` を設定するか XWayland を有効にしてください。

---

## 接続方法

### パターン A: 同じ LAN に接続されている場合

Mac のセットアップ画面で「検出されたサーバー」に Ubuntu が自動表示されるので、クリックするだけで接続先が入力されます。

```
Mac (クライアント) ←── LAN ──→ Ubuntu (サーバー)
```

### パターン B: SSH ポートフォワーディング経由（推奨）

通信を SSH トンネル内に通すため、インターネット経由でも安全に使えます。

**1. Mac のターミナルで SSH トンネルを張る**

```bash
ssh -L 7878:localhost:7878 user@remote-host
# このターミナルは接続中は開いたままにする
```

**2. Mac のセットアップ画面で接続先を入力**

```
localhost:7878
```

```
Mac (クライアント) ──SSH トンネル──→ Ubuntu (サーバー)
```

> SSH トンネルを使う場合、mDNS 自動検出は動作しません（異なるネットワーク上のため）。  
> 接続先は手動で `localhost:7878` と入力してください。

---

## 操作

| 操作 | 効果 |
|------|------|
| 左下にカーソルを寄せる | ウィンドウがスライドアップ |
| ウィンドウにファイルをドラッグ＆ドロップ | 相手側に転送 |
| Cmd+V | クリップボードのテキストを送信 |
| 受信アイテムの「Copy」 | クリップボードにコピー |
| 受信アイテムの「Save」 | `~/Downloads/SShare/` に保存 |
| 「Clear」 | 表示リストをクリア |
| 「✕」 | アプリを終了 |

---

## セキュリティ

通信は**暗号化されていません**。  
インターネット経由で使用する場合は必ず SSH ポートフォワーディング（パターン B）を使ってください。

## ファイルサイズ制限

プロトコル上の上限は 512 MB です。
