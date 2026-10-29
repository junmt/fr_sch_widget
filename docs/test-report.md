# F-RevoCRM Schedule Widget テスト結果報告書

- 実施日: 2026-10-27（会議室 F-14 の分は 2026-10-29）
- 対象: テスト仕様書 `docs/test-specification.md` v1.7

## 1. 結果の概要

| 種類 | 件数 | 成功 | 失敗 | 実行方法 |
|---|---|---|---|---|
| ユニットテスト（Rust） | 354 | 354 | 0 | Windows 上で `cargo test --lib` |
| ユニットテスト（TypeScript） | 92 | 92 | 0 | `npm test`（vitest） |
| ミューテーション（Rust） | 65 | 65 検出 | 0 見逃し | 3章 |
| ミューテーション（TypeScript） | 18 | 18 検出 | 0 見逃し | 3章 |
| 結合テスト（手動） | 44 | - | - | **未実施**（4章） |

- テスト仕様書の2〜7章のテストID（454件）は、すべてテストコードの関数名またはテスト名に含まれていることを、スクリプトで確認した。
- リリースビルド（`npm run tauri build`）が通り、インストーラー（NSIS、約1.7MB）ができることを確認した（自動アップデートの署名なし。署名付きのビルドは GitHub Actions のリリースで行う）。ビルド時の警告は0件。

## 2. 実行環境

| 項目 | 内容 |
|---|---|
| OS | Windows（WSL2 から PowerShell 経由で実行） |
| Rust | 1.99.1（x86_64-pc-windows-msvc） |
| C++ ビルドツール | Visual Studio 2026 Community（MSVC 14.51） |
| Node.js | v22（vitest を WSL で実行） / v24（Windows でのビルド） |
| 主なライブラリ | Tauri 2.x、reqwest 0.12、chrono 0.4、mockito 1.x、vitest 5.0 |

### モジュールごとのテスト件数（Rust）

| モジュール | テスト仕様書 | 件数 |
|---|---|---|
| schedule.rs | 2章 | 99 |
| rooms.rs | 2.12〜2.18 | 63 |
| crm.rs | 3章 | 71 |
| config.rs | 4章 | 35 |
| engine.rs | 5章 | 43 |
| text.rs | 6.1〜6.3 | 22 |
| window_pos.rs | 6.4 | 10 |
| notify.rs | 6.5 | 5 |
| updater.rs | 6.6 | 6 |

## 3. ミューテーション（テストの有効性の確認）

実装にわざとバグを1つずつ入れてテストを実行し、テストが失敗する（バグを見つけられる）ことを確認した。毎回、実行後に元のコードに戻した（Rust は Windows 側のコピーで行い、WSL 側のソースには触れていない）。

| ID | ファイル | 入れたバグ | 見つけるはずのテスト | 失敗したテストの数 | 結果 |
|---|---|---|---|---|---|
| M01 | schedule.rs | `item.start < day_end && item.end > day_start` → `item.start < day_end && item.end >= day_start` | OD-02 | 1 | 検出 |
| M02 | schedule.rs | `start.time() == NaiveTime::MIN && end.time() == NaiveTime::MIN` → `start.time() == NaiveTime::MIN \|\| end.time() == NaiveTime::MIN` | PR-11 | 1 | 検出 |
| M03 | schedule.rs | `const DEFAULT_DURATION_MINUTES: i64 = 30;` → `const DEFAULT_DURATION_MINUTES: i64 = 60;` | PR-07 | 1 | 検出 |
| M04 | schedule.rs | `if !self.show_held && is_held(item) {` → `if false && is_held(item) {` | FL-01 | 3 | 検出 |
| M05 | schedule.rs | `Some(old) if old.start != item.start \|\| old.end != item.end =>` → `Some(old) if old.start != item.start =>` | DF-04 | 1 | 検出 |
| M06 | schedule.rs | `let keep_new = matches!(badges.get(&item.id), Some((Badge::New, _)));` → `let keep_new = false;` | BD-04 | 1 | 検出 |
| M07 | schedule.rs | `now - *at < Duration::minutes(BADGE_TTL_MINUTES)` → `now - *at <= Duration::minutes(BADGE_TTL_MINUTES)` | EX-02 | 3 | 検出 |
| M08 | schedule.rs | `.filter(\|&&m\| now >= item.start - Duration::minutes(m as i64))` → `.filter(\|&&m\| now > item.start - Duration::minutes(m as i64))` | RM-08 | 8 | 検出 |
| M09 | schedule.rs | `due.push((item.clone(), (secs + 59) / 60));` → `due.push((item.clone(), secs / 60));` | RM-15 | 2 | 検出 |
| M10 | schedule.rs | `AND date_start <= '{}' AND due_date >= '{}'` → `AND date_start < '{}' AND due_date >= '{}'` | BQ-01 | 2 | 検出 |
| M11 | schedule.rs | `.filter(\|i\| seen.insert(i.id.clone()))` → `.filter(\|i\| seen.insert(i.id.clone()) \|\| true)` | BT-04 | 2 | 検出 |
| M12 | crm.rs | `"INVALID_SESSIONID" \| "SESSION_EXPIRED" \| "SESSION_LEFT_IDLE" \|` → `"INVALID_SESSIONID" \| "SESSION_EXPIRED" \|` | EC-01 | 1 | 検出 |
| M13 | crm.rs | `if n < QUERY_PAGE_SIZE {` → `if n <= QUERY_PAGE_SIZE {` | FE-02 | 3 | 検出 |
| M14 | crm.rs | `if reused && !retried {` → `if !retried {` | RL-03 | 1 | 検出 |
| M15 | crm.rs | `self.select_all.store(true, Ordering::Relaxed);` → `（削除）` | FB-01 | 1 | 検出 |
| M16 | crm.rs | `if u.to_ascii_lowercase().ends_with(suffix) {` → `if u.ends_with(suffix) {` | NU-04 | 1 | 検出 |
| M17 | crm.rs | `let hashed = format!("{:x}", md5::compute(format!("{token}{access_key}")));` → `let hashed = format!("{:x}", md5::compute(format!("{access_key}{token}")));` | LG-01 | 1 | 検出 |
| M18 | crm.rs | `Err(e) if e.needs_user_action() => { ⏎                     *session = None;` → `Err(e) if e.needs_user_action() => {` | RL-08 | 1 | 検出 |
| M19 | engine.rs | `let factor = 1u32 << self.failures.min(2);` → `let factor = 1u32 << self.failures.min(3);` | ND-01 | 2 | 検出 |
| M20 | engine.rs | `if self.baseline == Baseline::Full { ⏎             self.baseline = Baseline::Silent;` → `if false { ⏎             self.baseline = Baseline::Silent;` | OE-01 | 4 | 検出 |
| M21 | engine.rs | `if settings.notify_deleted {` → `if settings.notify_new_changed {` | OS-05 | 2 | 検出 |
| M22 | engine.rs | `next_fetch.map_or(true, \|t\| now >= t)` → `next_fetch.map_or(true, \|t\| now > t)` | SF-04 | 2 | 検出 |
| M23 | engine.rs | `(now - last_tick).num_seconds() > RESUME_GAP_SECONDS` → `(now - last_tick).num_seconds() >= RESUME_GAP_SECONDS` | IR-02 | 1 | 検出 |
| M24 | engine.rs | `self.failures = self.failures.saturating_add(1);` → `（削除）` | OE-01 | 4 | 検出 |
| M25 | engine.rs | `self.fired.clear();` → `（削除）` | RD-02 | 1 | 検出 |
| M26 | config.rs | `self.interval_minutes.clamp(INTERVAL_RANGE.0, INTERVAL_RANGE.1)` → `self.interval_minutes.clamp(INTERVAL_RANGE.0, INTERVAL_RANGE.1 + 1)` | NS-01 | 2 | 検出 |
| M27 | config.rs | `None if !old_username.is_empty() && old_username != new_username => KeyAction::Move,` → `None if !old_username.is_empty() => KeyAction::Move,` | KA-03 | 3 | 検出 |
| M28 | config.rs | `self.reminder_minutes.dedup();` → `（削除）` | NS-04 | 1 | 検出 |
| M29 | window_pos.rs | `x >= self.x && x < self.x + self.width as i32` → `x >= self.x && x <= self.x + self.width as i32` | WP-05 | 1 | 検出 |
| M30 | window_pos.rs | `first_run \|\| !monitors` → `!monitors` | WP-01 | 1 | 検出 |
| M31 | text.rs | `if s.chars().count() > TOOLTIP_MAX_CHARS {` → `if s.chars().count() >= TOOLTIP_MAX_CHARS {` | TT-06 | 1 | 検出 |
| M32 | text.rs | `Notice::Added(i) \| Notice::Changed(_, i) \| Notice::Reminder(i, _) => Some(&i.id),` → `Notice::Added(i) \| Notice::Changed(i, _) \| Notice::Reminder(i, _) => Some(&i.id),` | NI-01 | 1 | 検出 |
| M33 | notify.rs | `d.ends_with("target/debug")` → `d.ends_with("debug")` | N-04 | 1 | 検出 |
| M36 | crm.rs | `now < expires_at - 15秒` → `now <= expires_at - 15秒` | CU-02 | 2 | 検出 |
| M37 | crm.rs | `now >= until` → `now > until` | AU-09 | 1 | 検出 |
| M38 | crm.rs | 新しいトークンでの1回目の失敗で、LoginBlocked ではなくすぐ InvalidCredentials を返す | AU-07 | 4 | 検出 |
| M39 | crm.rs | 使い回したトークンでの失敗を InvalidCredentials にせず、新しいトークンを取り直す | AU-02 | 1 | 検出 |
| M34 | updater.rs | `now - t >= Duration::hours(..)` → `now - t > Duration::hours(..)` | UP-03 | 1 | 検出 |
| M35 | updater.rs | `now < t \|\| now - t >= ..` → `now - t >= ..` | UP-05 | 1 | 検出 |
| T01 | logic.ts | `if (start - t <= soonMinutes * 60_000) return "soon";` → `if (start - t < soonMinutes * 60_000) return "soon";` | PH-08 | 1 | 検出 |
| T02 | logic.ts | `>= STALE_MINUTES * 60_000` → `> STALE_MINUTES * 60_000` | IS-03 | 1 | 検出 |
| T03 | logic.ts | `return last > s ? `終日` → `return last >= s ? `終日` | RG-05 | 1 | 検出 |
| T04 | logic.ts | `if (s <= now.getTime()) continue;` → `if (s < now.getTime()) continue;` | NE-02 | 2 | 検出 |
| T05 | logic.ts | `if (status.fetching) return "更新中…";` → `（削除）` | FT-01 | 1 | 検出 |
| T06 | logic.ts | `if (start <= t) return "ongoing";` → `if (start < t) return "ongoing";` | PH-08 | 1 | 検出 |
| T07 | settings-logic.ts | `if (n > MAX_REMINDER_MINUTES) return null;` → `if (n >= MAX_REMINDER_MINUTES) return null;` | PM-05 | 1 | 検出 |
| T08 | settings-logic.ts | `if (t && !result.includes(t)) result.push(t);` → `if (t) result.push(t);` | PT-01 | 1 | 検出 |
| T09 | logic.ts | `timeLabel` の開始日の判定を外す（常に日付なし） | TL-02 | 1 | 検出 |
| T10 | logic.ts | 長さ0の予定でも終了時刻を出す | TL-04 | 1 | 検出 |
| M40 | rooms.rs | `b.start > item.start && b.start <= item.end` → `b.start > item.start && b.start < item.end` | AR-07 | 1以上 | 検出 |
| M41 | rooms.rs | 次の予約の条件から `!is_own(b)` を外す | AR-11 | 1以上 | 検出 |
| M42 | rooms.rs | 次の予約の条件から `!b.all_day` を外す | AR-13 | 1以上 | 検出 |
| M43 | rooms.rs | 次の予約の条件から `!is_not_held(b)` を外す | AR-12 | 1以上 | 検出 |
| M44 | rooms.rs | `b.start > item.start` → `b.start >= item.start` | AR-10 | 1以上 | 検出 |
| M45 | rooms.rs | 会議室の判定の `!b.all_day && same_event(item, b)` → `same_event(item, b)` | AR-01 | 1以上 | 検出 |
| M46 | rooms.rs | `same_event` から件名の比較を外す | AR-03 | 1以上 | 検出 |
| M47 | rooms.rs | `.min_by_key(\|b\| b.start)` → `.max_by_key(\|b\| b.start)` | AR-15 | 1以上 | 検出 |
| M48 | rooms.rs | `item.rooms.clear();` → （削除） | AR-17 | 1以上 | 検出 |
| M49 | rooms.rs | `now < i.end` → `now <= i.end` | RA-04 | 1以上 | 検出 |
| M50 | rooms.rs | `Duration::minutes(ROOM_ALERT_MINUTES)` → `Duration::minutes(ROOM_ALERT_MINUTES + 1)` | RA-02 | 1以上 | 検出 |
| M51 | rooms.rs | 知らせ済みの記録から終了時刻を外す（`room\|{id}`） | RA-11 | 1以上 | 検出 |
| M52 | rooms.rs | `remindable(i) &&` を外す | RA-09 | 1以上 | 検出 |
| M53 | rooms.rs | `r.next.is_some()` → `!r.name.is_empty()` | RA-06 | 1以上 | 検出 |
| M54 | rooms.rs | 名前の候補から名姓（`{first}{last}`）を外す | RR-04 | 1以上 | 検出 |
| M55 | rooms.rs | `name_key` の `.to_lowercase()` を外す | NK-02 | 1以上 | 検出 |
| M56 | rooms.rs | `spec.id.clone().or_else(` → `None.or_else(` | RR-01 | 1以上 | 検出 |
| M57 | rooms.rs | `!name.is_empty() && is_ws_id(id)` → `is_ws_id(id)` | RS-04 | 1以上 | 検出 |
| M58 | rooms.rs | `rsplit_once('=')` → `split_once('=')` | RS-06 | 1以上 | 検出 |
| M59 | rooms.rs | `build_day` で会議室の情報を付けない | BD-02 | 1以上 | 検出 |
| M60 | rooms.rs | `build_day` の自分の予定を、絞り込んだ一覧にする | BD-03 | 1以上 | 検出 |
| M61 | config.rs | 会議室の重複を `name_key` ではなく文字列そのもので判定する | NS-11 | 1以上 | 検出 |
| M62 | engine.rs | `tick` で会議室の知らせを通知に加えない | TK-04 | 1以上 | 検出 |
| M63 | text.rs | 会議室の通知の予約の時刻を、日付なしの `%H:%M` にする | NT-10 | 1以上 | 検出 |
| M64 | crm.rs | `build_query(owner_id, …)` → `build_query(&session.user_id, …)` | FO-01 | 1以上 | 検出 |
| T11 | logic.ts | `return left > 0 &&` → `return left >= 0 &&` | RW-04 | 1以上 | 検出 |
| T12 | logic.ts | `left <= ROOM_ALERT_MINUTES` → `left < ROOM_ALERT_MINUTES` | RW-02 | 1以上 | 検出 |
| T13 | logic.ts | `roomAlertActive` の完了・中止の判定を外す | RW-07 | 1以上 | 検出 |
| T14 | logic.ts | `if (!r.next) continue;` → （削除） | RN-03 | 1以上 | 検出 |
| T15 | logic.ts | `roomNextTexts` で常に日付を付けない | RN-04 | 1以上 | 検出 |
| T16 | logic.ts | `r.next.subject \|\| "（件名なし）"` → `r.next.subject` | RN-05 | 1以上 | 検出 |
| T17 | logic.ts | `roomNames` の区切りを `", "` → `","` | RN-02 | 1以上 | 検出 |
| T18 | logic.ts | `.some((r) => r.next)` → `.some((r) => r.name)` | RW-06 | 1以上 | 検出 |

## 4. 結合テスト（手動）

テスト仕様書 8章の IT-01〜IT-44 は、実際の F-RevoCRM・デスクトップ・通知を使って確認する必要があるため、**まだ実施していない**。特に次の点は、ユニットテストでは確認できないので、最初に確認することを勧める。

1. IT-04：実際の CRM で、時刻が CRM のカレンダー画面と同じになること
2. IT-14：終日予定が「開始・終了とも 00:00」で保存されているか（仕様書 9章 未決事項 No.2）
3. IT-25：対象にする一番古いバージョンの F-RevoCRM で、フィールドを指定した `query` が使えるか
4. IT-26〜27：自動アップデート（v0.1.0 を入れた状態で v0.1.1 をリリースして確認する）
5. IT-40〜44：会議室。特に、CRM の共有設定で会議室ユーザーの予定が見えるか、Users を一般ユーザーで読めるか（読めなければ `会議室B=19x12` の形で設定する）
6. IT-07：通知をクリックしたときの動作（インストールしたアプリで確認する。開発中の実行ファイルでは、PowerShell の通知として表示される）

## 5. テストの過程で見つけて直したこと

| No | 内容 | 対応 |
|---|---|---|
| 1 | 画面の `phaseOf` の「終了」の判定に、意味のない条件分けがあった（ミューテーション T06 が見逃しになったことで判明。長さ0の予定では `end <= t` と `start <= t` が同じになる） | 条件を `end <= t` だけにした。T06 は、意味のある別のバグ（進行中の判定の境界）に差し替えて検出を確認した |
| 2 | ミューテーション M11 が、最初はコンパイルエラーになっていて確認になっていなかった | コンパイルが通る形に直して再実行し、BT-01・BT-04 が失敗することを確認した |
| 3 | WSL から Windows へ同期するときに、ファイルの更新日時が古い日時に戻り、cargo が変更を無視して古いバイナリでテストすることがあった | `scripts/sync-to-windows.sh` を、中身で比べて更新日時をコピーしない方式に変えた |
| 4 | 使われていない関数（`mask_session`）と、テストでしか使わないメソッド（`Diff::is_empty`）があり、ビルドで警告が出ていた | `mask_session` は削除し、sessionName が通信エラーの文言に出ないことを MS-01 で確認する形にした。`is_empty` はテスト用にした |
| 5 | 判定のロジックが Tauri に依存する場所（ポーリングのループ、設定の保存、ウィンドウの配置など）にあり、テストできなかった | `should_fetch`、`is_resumed`、`fetch_with_relogin`、`key_action`、`validate_url`、`record_url`、`needs_reposition`、`top_right`、`is_dev_build` として切り出し、デシジョン表でテストした |
| 6 | 実際の F-RevoCRM（社内）で、予定の時刻が9時間ずれた。`query` は UTC で返すと考えていたが、実際は CRM サーバーのタイムゾーン（F-RevoCRM の既定は Asia/Tokyo）で返していた。ユニットテストは「UTC で返す」という誤った前提をそのまま検証していたため、見つけられなかった | 設定に「CRM サーバーのタイムゾーン」を追加し、既定を「PC と同じ」にした（v0.1.3）。不具合の再現テスト BT-05 と、PR-18・PR-19・LS-07 を追加した |
| 7 | 実際の F-RevoCRM（社内）で「ログインできません」になった。F-RevoCRM の不具合で、前の認証から5分以内の認証は必ず失敗するため（接続テストの直後の取得、再起動など） | ログインに成功したトークンを覚えて使い回し、新しいトークンでの1回目の失敗は5分待って再試行するようにした（v0.1.4、仕様書 5.2）。テスト CU・AU・LB・NF を追加した |
| 8 | v0.1.4 に更新した直後、9時間ずれた予定が表示された。更新前の版が保存したキャッシュ（ずれた時刻）を、認証の制限で待っている間そのまま表示していた | キャッシュに作った条件を保存し、条件が違うものは使わないようにした（v0.1.5）。CA-05・CA-06・CS-01 を追加した |
| 9 | 会議室の判定 `annotate_rooms` に「予定が終日なら会議室を付けない」という条件があったが、ミューテーション（AR-01）で見逃しになった。終日かどうかは時刻で決まるため、終日の予定と同じ開始・終了になるのは終日の予約だけで、その予約は内側の条件で除いているので、意味のない条件だった | 条件を削除し、内側の条件（終日の予約を除く）を外すミューテーションで AR-01 が失敗することを確認した |
| 10 | ミューテーション NS-11 が、最初はコンパイルエラーになっていて確認になっていなかった | 重複の判定を文字列そのものにする形に直して再実行し、NS-11 が失敗することを確認した |
