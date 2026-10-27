# F-RevoCRM Schedule Widget

F-RevoCRM から自分の今日の予定（活動）を取得して、デスクトップに常駐表示する Windows 用ウィジェットです。数分おきに取り直し、新しく入った予定や開始が近い予定を通知します。

- 仕様書：[docs/specification.md](docs/specification.md)
- テスト仕様書：[docs/test-specification.md](docs/test-specification.md)
- テスト結果：[docs/test-report.md](docs/test-report.md)

## 構成

| ディレクトリ | 内容 |
|---|---|
| `src/` | 画面（TypeScript）。`main.ts` がウィジェット、`settings.ts` が設定画面、`logic.ts` / `settings-logic.ts` が表示ロジック |
| `src-tauri/src/` | Rust（Tauri 2）。下の表を参照 |
| `scripts/sync-to-windows.sh` | WSL のソースを Windows 側のビルド用ディレクトリへ同期する |

| ファイル | 役割 |
|---|---|
| `schedule.rs` | 予定の変換（UTC → ローカル）・今日の判定・絞り込み・差分・バッジ・リマインド |
| `crm.rs` | Webservice API（getchallenge / login / query / logout）のクライアント |
| `engine.rs` | 取得結果から一覧・通知・再試行の間隔を決める状態機械 |
| `config.rs` | 設定・キャッシュ・資格情報（アクセスキー） |
| `text.rs` | 通知・ツールチップの文言 |
| `window_pos.rs` | ウィジェットを置く位置の計算 |
| `runtime.rs` | アプリの状態・ポーリングのループ・画面から呼ぶコマンド |
| `tray.rs` / `notify.rs` | タスクトレイ / Windows のトースト通知 |
| `updater.rs` | 自動アップデート（新しいバージョンの確認とインストール） |

## 開発環境

Windows 側に次のものが必要です。

- Rust（`x86_64-pc-windows-msvc`）
- Visual Studio Build Tools（C++ によるデスクトップ開発）
- Node.js
- WebView2 ランタイム（Windows 11 には標準で入っています）

WSL 上でソースを編集する場合は、Windows 側にコピーしてからビルドします（Windows のビルドツールは WSL のパス上では遅く、不安定なため）。

```bash
# WSL で
npm install
npm test                         # 画面のロジックのテスト（vitest）
scripts/sync-to-windows.sh       # C:\dev\fr_sch_widget へ同期
```

```powershell
# Windows の PowerShell で
cd C:\dev\fr_sch_widget
npm install
cd src-tauri; cargo test --lib; cd ..   # Rust のテスト
npm run tauri dev                       # 開発用に起動
npm run tauri build -- --config src-tauri/tauri.ci.conf.json   # インストーラーを作る（署名なし。src-tauri\target\release\bundle\nsis）
```

## 使い方

1. 起動すると設定画面が開きます。CRM の URL、ユーザー名、アクセスキーを入れて「接続テスト」→「保存」を押します。
   - アクセスキーは、F-RevoCRM の右上のユーザーメニュー →「マイ設定」→「ユーザーの詳細」にあります。
2. ウィジェットに今日の予定が表示されます。
   - ヘッダーをドラッグすると移動、ダブルクリックすると折りたたみます。
   - 予定をクリックすると、ブラウザで CRM の詳細画面が開きます。
   - 右クリックすると、更新・折りたたみ・最前面・設定・終了などのメニューが出ます。
   - ヘッダーの「‹」「›」で前の日・次の日の予定を見られます（「今日」で戻ります。5分操作しないと今日に戻ります）。
   - ヘッダーの「＋」で、CRM の予定の登録画面が開きます（今日以外を表示しているときは、その日の日付が入ります）。
   - 「NEW」「変更」のバッジはクリックすると消えます（30分たっても消えます）。
3. ウィジェットを閉じてもトレイに残ります。終了はトレイの右クリックメニューから行います。

## データの保存場所

| 内容 | 場所 |
|---|---|
| 設定 | `%APPDATA%\fr-sch-widget\config.json` |
| 前回取得した予定（その日の分だけ使う） | `%APPDATA%\fr-sch-widget\cache.json` |
| ログインに使ったトークン（F-RevoCRM の認証の制限を避けるため。秘密情報ではない） | `%APPDATA%\fr-sch-widget\auth.json` |
| アクセスキー | Windows 資格情報マネージャー（`fr-sch-widget`） |
| ログ | `%LOCALAPPDATA%\fr-sch-widget\logs\` |

## 自動ビルドとリリース（GitHub Actions）

| ワークフロー | タイミング | 内容 |
|---|---|---|
| `.github/workflows/ci.yml` | プルリクエスト（Dependabot の更新など）だけ | 画面のテストと、Windows での Rust のテスト。インストーラーは作らない |
| `.github/workflows/release.yml` | `v*` のタグの push | テストしてからインストーラーを作り、GitHub のリリースに公開する（main への push では何も動かない） |

リリースの手順：

1. `src-tauri/tauri.conf.json` と `src-tauri/Cargo.toml`、`package.json` の `version` を上げる（例：`0.2.0`）
2. コミットして、同じバージョンのタグを push する

```bash
git tag v0.2.0
git push origin main v0.2.0
```

タグと `tauri.conf.json` のバージョンが違うと、リリースのワークフローは止まる。

### 自動アップデート

- アプリは起動時と6時間おきに `https://github.com/junmt/fr_sch_widget/releases/latest/download/latest.json` を見て、新しいバージョンがあれば通知する。ウィジェットのフッターの「⬆ 更新」か、トレイの「v◯ に更新して再起動」で更新できる。
- 更新ファイルは署名して配る。秘密鍵とパスワードは GitHub のシークレット（`TAURI_SIGNING_PRIVATE_KEY`、`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`）に登録してあり、公開鍵は `tauri.conf.json` の `plugins.updater.pubkey` にある。
- **秘密鍵をなくすと、配ったアプリに更新を届けられなくなる。** 鍵を作った PC の `~/.tauri/fr-sch-widget.key`（とパスワードの `.password`）を安全な場所に控えておくこと。
- 手元でインストーラーを作るときは、署名なしでビルドする（`npm run tauri build -- --config src-tauri/tauri.ci.conf.json`）。署名付きで作るときは、上の2つを環境変数に入れてから `npm run tauri build` する。
