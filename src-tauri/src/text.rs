//! 通知・トレイのツールチップに出す文言（純粋な関数）

use crate::engine::{Notice, SyncState, SyncStatus};
use crate::schedule::{next_item, ScheduleItem};
use chrono::{NaiveDate, NaiveDateTime};

/// 今日なら "14:00"、別の日なら "10/28 14:00"
fn fmt_time(dt: NaiveDateTime, today: NaiveDate) -> String {
    if dt.date() == today {
        dt.format("%H:%M").to_string()
    } else {
        format!("{} {}", dt.format("%-m/%-d"), dt.format("%H:%M"))
    }
}

fn fmt_start(item: &ScheduleItem, today: NaiveDate) -> String {
    if item.all_day {
        "終日".into()
    } else {
        fmt_time(item.start, today)
    }
}

/// "25分" / "1時間5分" / "2時間"
pub fn fmt_remaining(minutes: i64) -> String {
    let m = minutes.max(0);
    match (m / 60, m % 60) {
        (0, mm) => format!("{mm}分"),
        (h, 0) => format!("{h}時間"),
        (h, mm) => format!("{h}時間{mm}分"),
    }
}

fn subject(item: &ScheduleItem) -> &str {
    if item.subject.is_empty() {
        "（件名なし）"
    } else {
        &item.subject
    }
}

/// 通知のタイトルと本文
pub fn notice_text(notice: &Notice, today: NaiveDate) -> (String, String) {
    match notice {
        Notice::Added(i) => ("新しい予定".into(), format!("{} {}", fmt_start(i, today), subject(i))),
        Notice::Changed(old, new) => (
            "予定の時刻が変わりました".into(),
            format!("{}→{} {}", fmt_start(old, today), fmt_start(new, today), subject(new)),
        ),
        Notice::Removed(i) => ("予定がなくなりました".into(), format!("{} {}", fmt_start(i, today), subject(i))),
        Notice::Reminder(i, minutes) => {
            let mut body = format!("{}-{} {}", fmt_time(i.start, today), fmt_time(i.end, today), subject(i));
            if let Some(loc) = &i.location {
                body.push_str(&format!("\n場所: {loc}"));
            }
            (format!("まもなく開始（あと{}）", fmt_remaining(*minutes)), body)
        }
    }
}

/// 通知をクリックしたときに強調する予定
pub fn notice_item_id(notice: &Notice) -> Option<&str> {
    match notice {
        Notice::Added(i) | Notice::Changed(_, i) | Notice::Reminder(i, _) => Some(&i.id),
        Notice::Removed(_) => None,
    }
}

/// Windows のトレイのツールチップは 127 文字まで
const TOOLTIP_MAX_CHARS: usize = 120;

pub fn tray_tooltip(items: &[ScheduleItem], now: NaiveDateTime, status: &SyncStatus) -> String {
    let mut s = match next_item(items, now) {
        Some(i) => {
            let minutes = ((i.start - now).num_seconds() + 59) / 60;
            format!("次: {} {}（あと{}）", fmt_time(i.start, now.date()), subject(i), fmt_remaining(minutes))
        }
        None => "今日はこれ以降の予定はありません".into(),
    };
    match status.state {
        SyncState::Ok => {}
        SyncState::Unconfigured => s = "接続設定をしてください".into(),
        SyncState::Error | SyncState::AuthError => {
            s = format!("⚠ {}\n{s}", status.message.as_deref().unwrap_or("エラー"));
        }
    }
    if s.chars().count() > TOOLTIP_MAX_CHARS {
        s = s.chars().take(TOOLTIP_MAX_CHARS - 1).collect::<String>() + "…";
    }
    s
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
