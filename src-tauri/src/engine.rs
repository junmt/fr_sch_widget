//! 取得結果から「表示する一覧・バッジ・通知・次の取得までの間隔」を決める状態機械。
//! Tauri や通信に依存しないので、時刻を渡してテストできる。

use crate::config::{Cache, Settings};
use crate::crm::CrmError;
use crate::schedule::{
    apply_badges, diff, due_reminders, expire_badges, record_badges, Badges, ScheduleItem,
};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime};
use serde::Serialize;
use std::collections::HashSet;
use std::time::Duration;

/// 次の取得結果をどう扱うか
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Baseline {
    /// 比べる相手がない（起動直後・日付の変更・設定の変更）。バッジも通知も出さない
    None,
    /// エラーから回復した直後。バッジだけ付けて通知はしない
    Silent,
    /// 通常。バッジを付けて通知もする
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    /// 接続設定がまだない
    Unconfigured,
    Ok,
    /// 通信エラーなど（自動で再試行する）
    Error,
    /// 認証・権限のエラー（設定を直すまで止める）
    AuthError,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub state: SyncState,
    pub message: Option<String>,
    pub last_updated: Option<DateTime<Local>>,
    pub fetching: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    Added(ScheduleItem),
    Changed(ScheduleItem, ScheduleItem),
    Removed(ScheduleItem),
    Reminder(ScheduleItem, i64),
}

/// 連続失敗時の間隔の上限（分）。設定した間隔がこれより長いときは設定値を上限にする
const MAX_BACKOFF_MINUTES: u32 = 15;

/// 前回のタイマーからこれ以上たっていたら、スリープから復帰したとみなす（秒）
pub const RESUME_GAP_SECONDS: i64 = 60;

/// 前回のタイマーから時間が大きく飛んでいれば、スリープから復帰したとみなす
pub fn is_resumed(last_tick: DateTime<Local>, now: DateTime<Local>) -> bool {
    (now - last_tick).num_seconds() > RESUME_GAP_SECONDS
}

/// 今取得するか。
/// - 手動更新・設定の保存（`force`）なら、止まっていても取得する
/// - 止まっている（認証エラー・未設定）なら取得しない
/// - 予定時刻がない（起動直後・復帰・日付の変更）か、予定時刻を過ぎていれば取得する
pub fn should_fetch(force: bool, paused: bool, next_fetch: Option<DateTime<Local>>, now: DateTime<Local>) -> bool {
    force || (!paused && next_fetch.map_or(true, |t| now >= t))
}

pub struct Engine {
    pub day: NaiveDate,
    items: Vec<ScheduleItem>,
    badges: Badges,
    fired: HashSet<String>,
    pub baseline: Baseline,
    pub status: SyncStatus,
    /// 自動で再試行するエラーが続いた回数
    pub failures: u32,
    /// 再試行する時刻が決まっているとき（CRM の認証の制限）
    pub retry_at: Option<DateTime<Local>>,
}

impl Engine {
    pub fn new(today: NaiveDate, cache: Option<Cache>) -> Self {
        let (items, last_updated) = match cache {
            Some(c) if c.date == today => (c.items, Some(c.last_updated)),
            _ => (Vec::new(), None),
        };
        let mut e = Self {
            day: today,
            items,
            badges: Badges::new(),
            fired: HashSet::new(),
            baseline: Baseline::None,
            status: SyncStatus { state: SyncState::Ok, message: None, last_updated, fetching: false },
            failures: 0,
            retry_at: None,
        };
        for i in &mut e.items {
            i.badge = None;
        }
        e
    }

    /// 画面に渡す一覧
    pub fn items(&self) -> &[ScheduleItem] {
        &self.items
    }

    /// 日付が変わっていたら前日の分を捨てる。変わったら true
    pub fn roll_day(&mut self, today: NaiveDate) -> bool {
        if today == self.day {
            return false;
        }
        self.day = today;
        self.items.clear();
        self.badges.clear();
        self.fired.clear();
        self.baseline = Baseline::None;
        true
    }

    /// 設定（絞り込みなど）が変わったとき。次の取得では差分を通知しない
    pub fn reset_baseline(&mut self) {
        self.baseline = Baseline::None;
        self.badges.clear();
        self.failures = 0;
    }

    pub fn set_unconfigured(&mut self) {
        self.status.state = SyncState::Unconfigured;
        self.status.message = Some("接続設定をしてください".into());
    }

    /// 取得に成功した（`new_items` は今日の分を絞り込み・並べ替え済みのもの）
    pub fn on_success(&mut self, mut new_items: Vec<ScheduleItem>, now: DateTime<Local>, settings: &Settings) -> Vec<Notice> {
        let d = diff(&self.items, &new_items);
        let mut notices = Vec::new();
        if self.baseline != Baseline::None {
            record_badges(&mut self.badges, &d, now.naive_local());
        }
        if self.baseline == Baseline::Full {
            if settings.notify_new_changed {
                notices.extend(d.added.iter().cloned().map(Notice::Added));
                notices.extend(d.changed.iter().cloned().map(|(o, n)| Notice::Changed(o, n)));
            }
            if settings.notify_deleted {
                notices.extend(d.removed.iter().cloned().map(Notice::Removed));
            }
        }
        apply_badges(&mut new_items, &self.badges);
        self.items = new_items;
        self.baseline = Baseline::Full;
        self.failures = 0;
        self.retry_at = None;
        self.status = SyncStatus { state: SyncState::Ok, message: None, last_updated: Some(now), fetching: false };
        notices
    }

    pub fn on_error(&mut self, err: &CrmError) {
        if self.baseline == Baseline::Full {
            self.baseline = Baseline::Silent;
        }
        self.status.fetching = false;
        self.status.message = Some(err.to_string());
        self.retry_at = err.retry_at();
        if err.needs_user_action() {
            self.status.state = SyncState::AuthError;
        } else {
            self.status.state = SyncState::Error;
            // 再試行の時刻が決まっているものは、間隔を延ばす対象にしない
            if self.retry_at.is_none() {
                self.failures = self.failures.saturating_add(1);
            }
        }
    }

    /// 認証・権限のエラーで止まっているか（手動更新か設定の保存で再開する）
    pub fn is_paused(&self) -> bool {
        matches!(self.status.state, SyncState::AuthError | SyncState::Unconfigured)
    }

    /// 次に取得する時刻。再試行の時刻が決まっていればそれ、なければ `now + next_delay`
    pub fn next_fetch_at(&self, now: DateTime<Local>, interval_minutes: u32) -> DateTime<Local> {
        self.retry_at
            .unwrap_or_else(|| now + chrono::Duration::from_std(self.next_delay(interval_minutes)).unwrap_or_default())
    }

    /// 次の取得までの間隔。続けて失敗したら 2倍 → 4倍 に延ばす（上限15分）
    pub fn next_delay(&self, interval_minutes: u32) -> Duration {
        let interval = interval_minutes.max(1);
        let factor = 1u32 << self.failures.min(2);
        let cap = MAX_BACKOFF_MINUTES.max(interval);
        Duration::from_secs(u64::from((interval * factor).min(cap)) * 60)
    }

    /// 毎回のタイマー処理。リマインドとバッジの期限切れを見る。
    /// 戻り値は (出すべき通知, 一覧の見た目が変わったか)
    pub fn tick(&mut self, now: NaiveDateTime, settings: &Settings) -> (Vec<Notice>, bool) {
        let reminders = due_reminders(&self.items, now, &settings.reminder_minutes, &mut self.fired)
            .into_iter()
            .map(|(i, m)| Notice::Reminder(i, m))
            .collect();
        let expired = expire_badges(&mut self.badges, now);
        if expired {
            apply_badges(&mut self.items, &self.badges);
        }
        (reminders, expired)
    }

    /// クリックでバッジを消す。消したら true
    pub fn dismiss_badge(&mut self, id: &str) -> bool {
        if self.badges.remove(id).is_none() {
            return false;
        }
        apply_badges(&mut self.items, &self.badges);
        true
    }

    pub fn cache(&self) -> Option<Cache> {
        let last_updated = self.status.last_updated?;
        let mut items = self.items.clone();
        for i in &mut items {
            i.badge = None;
        }
        Some(Cache { date: self.day, last_updated, items })
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
