//! テスト仕様書 6.1〜6.3（text.rs）。関数名の先頭はテストIDに対応する。
use super::*;

fn ndt(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
}

fn item(id: &str, start: &str, end: &str, subject: &str) -> ScheduleItem {
    ScheduleItem {
        id: id.into(),
        record_id: 1,
        subject: subject.into(),
        activity_type: "Meeting".into(),
        start: ndt(start),
        end: ndt(end),
        all_day: false,
        status: "Planned".into(),
        location: None,
        modified_time: String::new(),
        badge: None,
    }
}

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 27).unwrap()
}

fn status(state: SyncState, message: Option<&str>) -> SyncStatus {
    SyncStatus { state, message: message.map(str::to_string), last_updated: None, fetching: false }
}

fn std_item() -> ScheduleItem {
    item("18x1", "2026-10-27 14:00", "2026-10-27 15:00", "件名")
}

// ===== 6.1 fmt_remaining =====

#[test]
fn fr_fmt_remaining() {
    for (id, m, expected) in [
        ("FR-01", -5, "0分"),
        ("FR-02", 0, "0分"),
        ("FR-03", 59, "59分"),
        ("FR-04", 60, "1時間"),
        ("FR-05", 61, "1時間1分"),
        ("FR-06", 125, "2時間5分"),
    ] {
        assert_eq!(fmt_remaining(m), expected, "{id}");
    }
}

// ===== 6.2 notice_text =====

fn text(n: Notice) -> (String, String) {
    notice_text(&n, today())
}

#[test]
fn nt01_added_today() {
    assert_eq!(text(Notice::Added(std_item())), ("新しい予定".into(), "14:00 件名".into()));
}
#[test]
fn nt02_added_all_day() {
    let mut i = item("18x1", "2026-10-27 00:00", "2026-10-28 00:00", "件名");
    i.all_day = true;
    assert_eq!(text(Notice::Added(i)), ("新しい予定".into(), "終日 件名".into()));
}
#[test]
fn nt03_added_other_day() {
    let i = item("18x1", "2026-10-28 09:00", "2026-10-28 10:00", "件名");
    assert_eq!(text(Notice::Added(i)), ("新しい予定".into(), "10/28 09:00 件名".into()));
}
#[test]
fn nt04_changed() {
    let new = item("18x1", "2026-10-27 15:00", "2026-10-27 16:00", "件名");
    assert_eq!(text(Notice::Changed(std_item(), new)), ("予定の時刻が変わりました".into(), "14:00→15:00 件名".into()));
}
#[test]
fn nt05_removed() {
    assert_eq!(text(Notice::Removed(std_item())), ("予定がなくなりました".into(), "14:00 件名".into()));
}
#[test]
fn nt06_reminder_with_location() {
    let mut i = std_item();
    i.location = Some("会議室B".into());
    assert_eq!(
        text(Notice::Reminder(i, 10)),
        ("まもなく開始（あと10分）".into(), "14:00-15:00 件名\n場所: 会議室B".into())
    );
}
#[test]
fn nt07_reminder_without_location() {
    assert_eq!(text(Notice::Reminder(std_item(), 65)), ("まもなく開始（あと1時間5分）".into(), "14:00-15:00 件名".into()));
}
#[test]
fn nt08_empty_subject() {
    let i = item("18x1", "2026-10-27 14:00", "2026-10-27 15:00", "");
    assert_eq!(text(Notice::Added(i)).1, "14:00 （件名なし）");
}
#[test]
fn ni01_focus_id() {
    assert_eq!(notice_item_id(&Notice::Added(std_item())), Some("18x1"));
    let new = item("18x9", "2026-10-27 15:00", "2026-10-27 16:00", "件名");
    assert_eq!(notice_item_id(&Notice::Changed(std_item(), new)), Some("18x9"));
    assert_eq!(notice_item_id(&Notice::Reminder(std_item(), 1)), Some("18x1"));
}
#[test]
fn ni02_removed_has_no_focus() {
    assert_eq!(notice_item_id(&Notice::Removed(std_item())), None);
}

// ===== 6.3 tray_tooltip =====

fn items() -> Vec<ScheduleItem> {
    vec![item("18x0", "2026-10-27 09:00", "2026-10-27 10:00", "朝会"), std_item()]
}

const NEXT: &str = "次: 14:00 件名（あと25分）";
const NONE: &str = "今日はこれ以降の予定はありません";

#[test]
fn tt01_ok_with_next() {
    assert_eq!(tray_tooltip(&items(), ndt("2026-10-27 13:35"), &status(SyncState::Ok, None)), NEXT);
}
#[test]
fn tt02_ok_without_next() {
    assert_eq!(tray_tooltip(&items(), ndt("2026-10-27 14:00"), &status(SyncState::Ok, None)), NONE);
}
#[test]
fn tt03_error_with_next() {
    for state in [SyncState::Error, SyncState::AuthError] {
        assert_eq!(
            tray_tooltip(&items(), ndt("2026-10-27 13:35"), &status(state, Some("接続できません"))),
            format!("⚠ 接続できません\n{NEXT}"),
            "{state:?}"
        );
    }
}
#[test]
fn tt04_error_without_next() {
    for state in [SyncState::Error, SyncState::AuthError] {
        assert_eq!(
            tray_tooltip(&[], ndt("2026-10-27 13:35"), &status(state, Some("接続できません"))),
            format!("⚠ 接続できません\n{NONE}"),
            "{state:?}"
        );
    }
}
#[test]
fn tt05_unconfigured() {
    assert_eq!(tray_tooltip(&items(), ndt("2026-10-27 13:35"), &status(SyncState::Unconfigured, None)), "接続設定をしてください");
}

/// ツールチップがちょうど n 文字になる件名で作る
fn tooltip_of_len(n: usize) -> String {
    // "次: 14:00 " + 件名 + "（あと1時間）" の長さを n にする
    let fixed = "次: 14:00 ".chars().count() + "（あと1時間）".chars().count();
    let subject = "あ".repeat(n - fixed);
    let items = vec![item("18x1", "2026-10-27 14:00", "2026-10-27 15:00", &subject)];
    tray_tooltip(&items, ndt("2026-10-27 13:00"), &status(SyncState::Ok, None))
}

#[test]
fn tt06_exactly_max() {
    let t = tooltip_of_len(TOOLTIP_MAX_CHARS);
    assert_eq!(t.chars().count(), TOOLTIP_MAX_CHARS);
    assert!(!t.ends_with('…'));
}
#[test]
fn tt07_over_max_truncated() {
    for n in [TOOLTIP_MAX_CHARS + 1, 300] {
        let t = tooltip_of_len(n);
        assert_eq!(t.chars().count(), TOOLTIP_MAX_CHARS, "{n}");
        assert!(t.ends_with('…'), "{n}");
        assert!(t.starts_with("次: 14:00 あ"), "{n}");
    }
}
#[test]
fn tt08_error_without_message() {
    assert_eq!(tray_tooltip(&[], ndt("2026-10-27 13:35"), &status(SyncState::Error, None)), format!("⚠ エラー\n{NONE}"));
}
