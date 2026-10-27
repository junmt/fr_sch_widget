//! テスト仕様書 5章（engine.rs）。関数名の先頭はテストIDに対応する。
use super::*;
use crate::schedule::Badge;
use chrono::{Duration as CDuration, TimeZone};

fn day(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn ndt(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
}

fn local(s: &str) -> DateTime<Local> {
    Local.from_local_datetime(&ndt(s)).single().unwrap()
}

fn item(id: &str, start: &str, end: &str) -> ScheduleItem {
    ScheduleItem {
        id: id.into(),
        record_id: 1,
        subject: id.into(),
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

fn notify(new_changed: bool, deleted: bool) -> Settings {
    Settings { notify_new_changed: new_changed, notify_deleted: deleted, ..Default::default() }
}

fn a() -> ScheduleItem {
    item("18x1", "2026-10-27 13:00", "2026-10-27 14:00")
}
fn a_moved() -> ScheduleItem {
    item("18x1", "2026-10-27 13:30", "2026-10-27 14:00")
}
fn b() -> ScheduleItem {
    item("18x2", "2026-10-27 15:00", "2026-10-27 16:00")
}
fn c() -> ScheduleItem {
    item("18x3", "2026-10-27 17:00", "2026-10-27 18:00")
}

fn kinds(n: &[Notice]) -> Vec<&'static str> {
    n.iter()
        .map(|x| match x {
            Notice::Added(_) => "added",
            Notice::Changed(..) => "changed",
            Notice::Removed(_) => "removed",
            Notice::Reminder(..) => "reminder",
        })
        .collect()
}

fn badges(e: &Engine) -> Vec<(&str, Option<Badge>)> {
    e.items().iter().map(|i| (i.id.as_str(), i.badge)).collect()
}

/// 指定した Baseline にした Engine（一覧は [a, b]）
fn engine_with(baseline: Baseline) -> Engine {
    let mut e = Engine::new(day("2026-10-27"), None);
    e.on_success(vec![a(), b()], local("2026-10-27 09:00"), &Settings::default());
    match baseline {
        Baseline::Full => {}
        Baseline::Silent => e.on_error(&CrmError::Network("x".into())),
        Baseline::None => e.reset_baseline(),
    }
    assert_eq!(e.baseline, baseline);
    e
}

/// 次の取得：a は時刻の変更、b は削除、c は新規
fn next_fetch(e: &mut Engine, s: &Settings) -> Vec<Notice> {
    e.on_success(vec![a_moved(), c()], local("2026-10-27 09:03"), s)
}

fn assert_success_postconditions(e: &Engine) {
    assert_eq!(e.baseline, Baseline::Full);
    assert_eq!(e.failures, 0);
    assert_eq!(e.status.state, SyncState::Ok);
    assert_eq!(e.status.message, None);
    assert_eq!(e.status.last_updated, Some(local("2026-10-27 09:03")));
    assert!(!e.status.fetching);
    assert_eq!(e.items().iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), vec!["18x1", "18x3"]);
}

// ===== 5.1 should_fetch =====

#[test]
fn sf_should_fetch_rules() {
    let now = local("2026-10-27 10:00");
    let past = Some(local("2026-10-27 09:59"));
    let equal = Some(now);
    let future = Some(local("2026-10-27 10:01"));
    assert!(should_fetch(true, true, future, now), "SF-01");
    assert!(!should_fetch(false, true, None, now), "SF-02");
    assert!(should_fetch(false, false, None, now), "SF-03");
    assert!(should_fetch(false, false, past, now), "SF-04 past");
    assert!(should_fetch(false, false, equal, now), "SF-04 equal");
    assert!(!should_fetch(false, false, future, now), "SF-05");
}
#[test]
fn sf_all_16_combinations() {
    let now = local("2026-10-27 10:00");
    let times = [
        ("none", None),
        ("past", Some(local("2026-10-27 09:59"))),
        ("equal", Some(now)),
        ("future", Some(local("2026-10-27 10:01"))),
    ];
    let mut n = 0;
    for force in [true, false] {
        for paused in [true, false] {
            for (label, t) in times {
                let expected = if force {
                    true // SF-01
                } else if paused {
                    false // SF-02
                } else {
                    label != "future" // SF-03 / SF-04 / SF-05
                };
                assert_eq!(should_fetch(force, paused, t, now), expected, "{force} {paused} {label}");
                n += 1;
            }
        }
    }
    assert_eq!(n, 16);
}

// ===== 5.2 is_resumed =====

#[test]
fn ir_is_resumed() {
    let last = local("2026-10-27 10:00");
    for (id, secs, expected) in [("IR-01", 10, false), ("IR-02", 60, false), ("IR-03", 61, true), ("IR-04", -5, false)] {
        assert_eq!(is_resumed(last, last + CDuration::seconds(secs)), expected, "{id}");
    }
}

// ===== 5.3 Engine::new =====

#[test]
fn en01_today_cache() {
    let mut cached = a();
    cached.badge = Some(Badge::New);
    let cache = Cache { source: String::new(), date: day("2026-10-27"), last_updated: local("2026-10-27 08:00"), items: vec![cached] };
    let e = Engine::new(day("2026-10-27"), Some(cache));
    assert_eq!(e.items().len(), 1);
    assert_eq!(e.items()[0].badge, None);
    assert_eq!(e.status.last_updated, Some(local("2026-10-27 08:00")));
    assert_eq!(e.baseline, Baseline::None);
    assert_eq!(e.status.state, SyncState::Ok);
}
#[test]
fn en02_other_day_cache() {
    let cache = Cache { source: String::new(), date: day("2026-10-26"), last_updated: local("2026-10-26 08:00"), items: vec![a()] };
    let e = Engine::new(day("2026-10-27"), Some(cache));
    assert!(e.items().is_empty());
    assert_eq!(e.status.last_updated, None);
}
#[test]
fn en03_no_cache() {
    let e = Engine::new(day("2026-10-27"), None);
    assert!(e.items().is_empty());
    assert_eq!(e.status.last_updated, None);
    assert_eq!(e.baseline, Baseline::None);
    assert_eq!(e.failures, 0);
}

// ===== 5.4 on_success =====

#[test]
fn os01_baseline_none() {
    let mut e = engine_with(Baseline::None);
    let n = next_fetch(&mut e, &notify(true, true));
    assert!(n.is_empty());
    assert_eq!(badges(&e), vec![("18x1", None), ("18x3", None)]);
    assert_success_postconditions(&e);
}
#[test]
fn os02_baseline_silent() {
    let mut e = engine_with(Baseline::Silent);
    let n = next_fetch(&mut e, &notify(true, true));
    assert!(n.is_empty());
    assert_eq!(badges(&e), vec![("18x1", Some(Badge::Changed)), ("18x3", Some(Badge::New))]);
    assert_success_postconditions(&e);
}
#[test]
fn os03_full_notify_all() {
    let mut e = engine_with(Baseline::Full);
    let n = next_fetch(&mut e, &notify(true, true));
    assert_eq!(kinds(&n), vec!["added", "changed", "removed"]);
    assert_eq!(n[0], Notice::Added(c()));
    assert_eq!(n[1], Notice::Changed(a(), a_moved()));
    assert_eq!(n[2], Notice::Removed(b()));
    assert_eq!(badges(&e), vec![("18x1", Some(Badge::Changed)), ("18x3", Some(Badge::New))]);
    assert_success_postconditions(&e);
}
#[test]
fn os04_full_new_changed_only() {
    let mut e = engine_with(Baseline::Full);
    assert_eq!(kinds(&next_fetch(&mut e, &notify(true, false))), vec!["added", "changed"]);
    assert_eq!(badges(&e), vec![("18x1", Some(Badge::Changed)), ("18x3", Some(Badge::New))]);
    assert_success_postconditions(&e);
}
#[test]
fn os05_full_deleted_only() {
    let mut e = engine_with(Baseline::Full);
    assert_eq!(kinds(&next_fetch(&mut e, &notify(false, true))), vec!["removed"]);
    assert_eq!(badges(&e), vec![("18x1", Some(Badge::Changed)), ("18x3", Some(Badge::New))]);
    assert_success_postconditions(&e);
}
#[test]
fn os06_full_no_notifications() {
    let mut e = engine_with(Baseline::Full);
    assert!(next_fetch(&mut e, &notify(false, false)).is_empty());
    assert_eq!(badges(&e), vec![("18x1", Some(Badge::Changed)), ("18x3", Some(Badge::New))]);
    assert_success_postconditions(&e);
}
#[test]
fn os_first_fetch_after_start() {
    // 起動直後（キャッシュなし）の最初の取得は OS-01 と同じ扱い
    let mut e = Engine::new(day("2026-10-27"), None);
    assert!(e.on_success(vec![a(), b()], local("2026-10-27 09:00"), &notify(true, true)).is_empty());
    assert!(e.items().iter().all(|i| i.badge.is_none()));
}

// ===== 5.5 on_error =====

fn retryable_errors() -> Vec<CrmError> {
    vec![
        CrmError::Network("x".into()),
        CrmError::BadResponse("x".into()),
        CrmError::Api { code: "X".into(), message: "m".into() },
        CrmError::SessionExpired,
    ]
}

fn assert_error_keeps_data(e: &Engine, err: &CrmError) {
    assert_eq!(e.items().len(), 2, "{err:?}");
    assert_eq!(e.status.last_updated, Some(local("2026-10-27 09:00")), "{err:?}");
    assert!(!e.status.fetching, "{err:?}");
    assert_eq!(e.status.message.as_deref(), Some(err.to_string().as_str()), "{err:?}");
}

#[test]
fn oe01_retryable_from_full() {
    for err in retryable_errors() {
        let mut e = engine_with(Baseline::Full);
        e.status.fetching = true;
        e.on_error(&err);
        assert_eq!(e.baseline, Baseline::Silent, "{err:?}");
        assert_eq!(e.status.state, SyncState::Error, "{err:?}");
        assert_eq!(e.failures, 1, "{err:?}");
        assert!(!e.is_paused(), "{err:?}");
        assert_error_keeps_data(&e, &err);
    }
}
#[test]
fn oe02_retryable_from_silent() {
    let mut e = engine_with(Baseline::Silent);
    e.on_error(&CrmError::Network("y".into()));
    assert_eq!(e.baseline, Baseline::Silent);
    assert_eq!(e.failures, 2);
    assert_error_keeps_data(&e, &CrmError::Network("y".into()));
}
#[test]
fn oe03_retryable_from_none() {
    let mut e = engine_with(Baseline::None);
    e.on_error(&CrmError::Network("x".into()));
    assert_eq!(e.baseline, Baseline::None);
    assert_eq!(e.status.state, SyncState::Error);
    assert_eq!(e.failures, 1);
}
#[test]
fn oe04_invalid_credentials() {
    let mut e = engine_with(Baseline::Full);
    e.on_error(&CrmError::InvalidCredentials);
    assert_eq!(e.status.state, SyncState::AuthError);
    assert_eq!(e.failures, 0);
    assert_eq!(e.baseline, Baseline::Silent);
    assert!(e.is_paused());
    assert_error_keeps_data(&e, &CrmError::InvalidCredentials);
}
#[test]
fn oe05_access_denied() {
    let mut e = engine_with(Baseline::Full);
    e.on_error(&CrmError::AccessDenied);
    assert_eq!(e.status.state, SyncState::AuthError);
    assert_eq!(e.failures, 0);
    assert!(e.is_paused());
    assert_error_keeps_data(&e, &CrmError::AccessDenied);
}

// ===== 5.6 is_paused =====

#[test]
fn pa_is_paused() {
    for (id, state, expected) in [
        ("PA-01", SyncState::Ok, false),
        ("PA-02", SyncState::Error, false),
        ("PA-03", SyncState::AuthError, true),
        ("PA-04", SyncState::Unconfigured, true),
    ] {
        let mut e = Engine::new(day("2026-10-27"), None);
        e.status.state = state;
        assert_eq!(e.is_paused(), expected, "{id}");
    }
}

// ===== 5.7 next_delay =====

#[test]
fn nd_next_delay_table() {
    let table: [(&str, u32, [u64; 5]); 6] = [
        ("ND-01", 3, [3, 6, 12, 12, 12]),
        ("ND-02", 1, [1, 2, 4, 4, 4]),
        ("ND-03", 5, [5, 10, 15, 15, 15]),
        ("ND-04", 15, [15, 15, 15, 15, 15]),
        ("ND-05", 30, [30, 30, 30, 30, 30]),
        ("ND-06", 0, [1, 2, 4, 4, 4]),
    ];
    for (id, interval, expected) in table {
        for (failures, minutes) in [0u32, 1, 2, 3, 100].into_iter().zip(expected) {
            let mut e = Engine::new(day("2026-10-27"), None);
            e.failures = failures;
            assert_eq!(e.next_delay(interval).as_secs(), minutes * 60, "{id} failures={failures}");
        }
    }
}
#[test]
fn nd07_reset_after_success() {
    let mut e = Engine::new(day("2026-10-27"), None);
    for _ in 0..3 {
        e.on_error(&CrmError::Network("x".into()));
    }
    assert_eq!(e.next_delay(3).as_secs(), 12 * 60);
    e.on_success(vec![], local("2026-10-27 09:00"), &Settings::default());
    assert_eq!(e.next_delay(3).as_secs(), 3 * 60);
}

// ===== 5.8 日付の変更・設定の変更・タイマー =====

#[test]
fn rd01_same_day() {
    let mut e = engine_with(Baseline::Full);
    assert!(!e.roll_day(day("2026-10-27")));
    assert_eq!(e.items().len(), 2);
    assert_eq!(e.baseline, Baseline::Full);
}
#[test]
fn rd02_new_day_clears() {
    let mut e = engine_with(Baseline::Full);
    next_fetch(&mut e, &Settings::default()); // バッジを付ける
    e.tick(ndt("2026-10-27 13:20"), &Settings::default()); // リマインドを記録する
    assert!(!e.fired.is_empty());
    assert!(e.roll_day(day("2026-10-28")));
    assert_eq!(e.day, day("2026-10-28"));
    assert!(e.items().is_empty());
    assert!(e.badges.is_empty());
    assert!(e.fired.is_empty());
    assert_eq!(e.baseline, Baseline::None);
}
#[test]
fn rd03_first_fetch_after_new_day() {
    let mut e = engine_with(Baseline::Full);
    e.roll_day(day("2026-10-28"));
    let n = e.on_success(vec![item("18x5", "2026-10-28 10:00", "2026-10-28 11:00")], local("2026-10-28 00:00"), &notify(true, true));
    assert!(n.is_empty());
    assert_eq!(e.items()[0].badge, None);
}
#[test]
fn rb01_reset_baseline() {
    let mut e = engine_with(Baseline::Full);
    next_fetch(&mut e, &Settings::default());
    e.failures = 2;
    e.reset_baseline();
    assert_eq!(e.baseline, Baseline::None);
    assert!(e.badges.is_empty());
    assert_eq!(e.failures, 0);
}
#[test]
fn rb02_first_fetch_after_reset() {
    let mut e = engine_with(Baseline::Full);
    e.reset_baseline();
    assert!(next_fetch(&mut e, &notify(true, true)).is_empty());
    assert_eq!(badges(&e), vec![("18x1", None), ("18x3", None)]);
}
#[test]
fn uc01_set_unconfigured() {
    let mut e = Engine::new(day("2026-10-27"), None);
    e.set_unconfigured();
    assert_eq!(e.status.state, SyncState::Unconfigured);
    assert_eq!(e.status.message.as_deref(), Some("接続設定をしてください"));
}
#[test]
fn tk01_reminder_once() {
    let mut e = engine_with(Baseline::Full);
    let (n, changed) = e.tick(ndt("2026-10-27 12:50"), &Settings::default());
    assert_eq!(n, vec![Notice::Reminder(a(), 10)]);
    assert!(!changed);
    assert!(e.tick(ndt("2026-10-27 12:51"), &Settings::default()).0.is_empty());
}
#[test]
fn tk02_badge_expiry() {
    let mut e = engine_with(Baseline::Full);
    next_fetch(&mut e, &Settings::default()); // 09:03 にバッジ
    assert!(!e.tick(ndt("2026-10-27 09:32"), &Settings::default()).1);
    assert!(e.tick(ndt("2026-10-27 09:33"), &Settings::default()).1);
    assert_eq!(badges(&e), vec![("18x1", None), ("18x3", None)]);
}
#[test]
fn tk03_nothing_happens() {
    let mut e = engine_with(Baseline::Full);
    let (n, changed) = e.tick(ndt("2026-10-27 10:00"), &Settings::default());
    assert!(n.is_empty());
    assert!(!changed);
}
#[test]
fn db01_dismiss_existing() {
    let mut e = engine_with(Baseline::Full);
    next_fetch(&mut e, &Settings::default());
    assert!(e.dismiss_badge("18x3"));
    assert_eq!(badges(&e), vec![("18x1", Some(Badge::Changed)), ("18x3", None)]);
}
#[test]
fn db02_dismiss_missing() {
    let mut e = engine_with(Baseline::Full);
    assert!(!e.dismiss_badge("18x1")); // バッジなし
    assert!(!e.dismiss_badge("unknown"));
}
#[test]
fn db03_dismissed_badge_does_not_return() {
    let mut e = engine_with(Baseline::Full);
    next_fetch(&mut e, &Settings::default());
    e.dismiss_badge("18x3");
    e.on_success(vec![a_moved(), c()], local("2026-10-27 09:06"), &Settings::default());
    assert_eq!(badges(&e), vec![("18x1", Some(Badge::Changed)), ("18x3", None)]);
}
#[test]
fn cc01_no_cache_before_success() {
    assert!(Engine::new(day("2026-10-27"), None).cache("S").is_none());
}
#[test]
fn cc02_cache_after_success() {
    let mut e = engine_with(Baseline::Full);
    next_fetch(&mut e, &Settings::default());
    let c = e.cache("S").unwrap();
    assert_eq!(c.source, "S");
    assert_eq!(c.date, day("2026-10-27"));
    assert_eq!(c.last_updated, local("2026-10-27 09:03"));
    assert_eq!(c.items.len(), 2);
    assert!(c.items.iter().all(|i| i.badge.is_none()));
}

// ===== 5.9 認証の制限で待つ（LoginBlocked） =====

fn blocked(at: &str) -> CrmError {
    CrmError::LoginBlocked { retry_at: local(at) }
}

#[test]
fn lb01_login_blocked_waits_until_retry_time() {
    let mut e = engine_with(Baseline::Full);
    e.on_error(&blocked("2026-10-27 09:05"));
    assert_eq!(e.status.state, SyncState::Error);
    assert!(!e.is_paused());
    assert_eq!(e.failures, 0); // 間隔を延ばす対象にしない
    assert_eq!(e.retry_at, Some(local("2026-10-27 09:05")));
    assert_eq!(e.baseline, Baseline::Silent);
    assert!(e.status.message.as_deref().unwrap().contains("09:05"));
    assert_error_keeps_data(&e, &blocked("2026-10-27 09:05"));
}
#[test]
fn lb02_next_fetch_at_uses_retry_time() {
    let mut e = engine_with(Baseline::Full);
    e.on_error(&blocked("2026-10-27 09:05"));
    assert_eq!(e.next_fetch_at(local("2026-10-27 09:00"), 3), local("2026-10-27 09:05"));
}
#[test]
fn lb03_success_clears_retry_time() {
    let mut e = engine_with(Baseline::Full);
    e.on_error(&blocked("2026-10-27 09:05"));
    next_fetch(&mut e, &Settings::default());
    assert_eq!(e.retry_at, None);
    assert_eq!(e.next_fetch_at(local("2026-10-27 09:05"), 3), local("2026-10-27 09:08"));
}
#[test]
fn lb04_other_error_clears_retry_time_and_backs_off() {
    let mut e = engine_with(Baseline::Full);
    e.on_error(&blocked("2026-10-27 09:05"));
    e.on_error(&CrmError::Network("x".into()));
    assert_eq!(e.retry_at, None);
    assert_eq!(e.failures, 1);
    assert_eq!(e.next_fetch_at(local("2026-10-27 09:05"), 3), local("2026-10-27 09:11"));
}
#[test]
fn lb05_real_credential_error_after_waiting_pauses() {
    let mut e = engine_with(Baseline::Full);
    e.on_error(&blocked("2026-10-27 09:05"));
    e.on_error(&CrmError::InvalidCredentials);
    assert_eq!(e.retry_at, None);
    assert!(e.is_paused());
}
#[test]
fn nf01_next_fetch_at_without_retry_time() {
    let e = engine_with(Baseline::Full);
    assert_eq!(e.next_fetch_at(local("2026-10-27 09:00"), 3), local("2026-10-27 09:03"));
}
