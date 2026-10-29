//! テスト仕様書 4章（config.rs）。関数名の先頭はテストIDに対応する。
use super::*;
use chrono::NaiveDateTime;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fr-sch-widget-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn norm(s: Settings) -> Settings {
    s.normalized()
}

// ===== 4.1 normalized =====

#[test]
fn ns01_interval_bounds() {
    for (input, expected) in [(0, 1), (1, 1), (30, 30), (31, 30)] {
        assert_eq!(norm(Settings { interval_minutes: input, ..Default::default() }).interval_minutes, expected, "{input}");
    }
}
#[test]
fn ns02_opacity_bounds() {
    for (input, expected) in [(49, 50), (50, 50), (100, 100), (101, 100)] {
        assert_eq!(norm(Settings { opacity: input, ..Default::default() }).opacity, expected, "{input}");
    }
}
#[test]
fn ns03_soon_bounds() {
    for (input, expected) in [(0, 0), (1440, 1440), (1441, 1440)] {
        assert_eq!(norm(Settings { soon_minutes: input, ..Default::default() }).soon_minutes, expected, "{input}");
    }
}
#[test]
fn ns04_reminders_filtered_deduped_sorted() {
    let s = norm(Settings { reminder_minutes: vec![1, 10, 10, 1440, 1441, 0], ..Default::default() });
    assert_eq!(s.reminder_minutes, vec![1440, 10, 1, 0]);
}
#[test]
fn ns05_reminders_empty() {
    assert!(norm(Settings { reminder_minutes: vec![], ..Default::default() }).reminder_minutes.is_empty());
}
#[test]
fn ns06_activity_types_trimmed() {
    let s = norm(Settings { activity_types: vec![" Call ".into(), "".into(), "  ".into()], ..Default::default() });
    assert_eq!(s.activity_types, vec!["Call".to_string()]);
}
#[test]
fn ns07_theme() {
    for (input, expected) in [("system", "system"), ("light", "light"), ("dark", "dark"), ("pink", "system"), ("", "system")] {
        assert_eq!(norm(Settings { theme: input.into(), ..Default::default() }).theme, expected, "{input:?}");
    }
}
#[test]
fn ns08_url_normalized() {
    assert_eq!(norm(Settings { crm_url: " https://x/index.php ".into(), ..Default::default() }).crm_url, "https://x");
}
#[test]
fn ns09_username_trimmed() {
    assert_eq!(norm(Settings { username: " jun ".into(), ..Default::default() }).username, "jun");
}
#[test]
fn ns11_meeting_rooms_trimmed_deduped() {
    let s = norm(Settings {
        meeting_rooms: vec![" 会議室A ".into(), "".into(), "  ".into(), "会議室 a".into(), "会議室B=19x12".into(), "会議室A".into()],
        ..Default::default()
    });
    // 空白・大文字小文字の違いだけのものは重複とみなし、最初のものを残す
    assert_eq!(s.meeting_rooms, vec!["会議室A".to_string(), "会議室B=19x12".to_string()]);
}
#[test]
fn ns10_idempotent() {
    let once = norm(Settings {
        interval_minutes: 99,
        reminder_minutes: vec![1, 10],
        crm_url: "https://x/".into(),
        meeting_rooms: vec![" A ".into(), "a".into()],
        ..Default::default()
    });
    assert_eq!(norm(once.clone()), once);
    assert_eq!(norm(Settings::default()), Settings::default());
}

#[test]
fn ds01_defaults_match_specification() {
    let s = Settings::default();
    assert_eq!(s.crm_url, "");
    assert_eq!(s.username, "");
    assert_eq!(s.crm_time_zone, CrmTimeZone::Local);
    assert_eq!(s.interval_minutes, 3);
    assert_eq!(s.reminder_minutes, vec![10]);
    assert_eq!(s.soon_minutes, 15);
    assert!(s.notify_new_changed);
    assert!(!s.notify_deleted);
    assert!(s.activity_types.is_empty());
    assert!(s.show_not_held);
    assert!(s.show_held);
    assert!(s.always_on_top);
    assert_eq!(s.opacity, 90);
    assert_eq!(s.theme, "system");
    assert!(s.autostart);
    assert!(s.auto_update_check);
    assert!(s.meeting_rooms.is_empty());
}

#[test]
fn ic_is_connection_configured() {
    for (id, url, user, expected) in [
        ("IC-01", "https://x", "u", true),
        ("IC-02", "https://x", "", false),
        ("IC-03", "", "u", false),
        ("IC-04", "", "", false),
    ] {
        let s = Settings { crm_url: url.into(), username: user.into(), ..Default::default() };
        assert_eq!(s.is_connection_configured(), expected, "{id}");
    }
}

// ===== 4.2 validate_url =====

#[test]
fn vu_validate_url() {
    for (id, url, ok) in [
        ("VU-01", "", true),
        ("VU-02", "https://crm.example.jp", true),
        ("VU-03", "http://crm.example.jp", true),
        ("VU-04", "HTTPS://crm.example.jp", true),
        ("VU-05", "ftp://crm.example.jp", false),
        ("VU-06", "crm.example.jp", false),
    ] {
        assert_eq!(validate_url(url).is_ok(), ok, "{id}");
    }
}

// ===== 4.3 key_action =====

#[test]
fn ka01_input_key_is_saved_trimmed() {
    assert_eq!(key_action(Some(" KEY "), "old", "new"), KeyAction::Save("KEY".into()));
    assert_eq!(key_action(Some("KEY"), "", "new"), KeyAction::Save("KEY".into()));
}
#[test]
fn ka02_username_changed_moves_key() {
    assert_eq!(key_action(None, "old", "new"), KeyAction::Move);
}
#[test]
fn ka03_same_username_keeps() {
    assert_eq!(key_action(None, "jun", "jun"), KeyAction::Keep);
}
#[test]
fn ka04_no_old_username_keeps() {
    assert_eq!(key_action(None, "", "new"), KeyAction::Keep);
    assert_eq!(key_action(None, "", ""), KeyAction::Keep);
}
#[test]
fn ka05_blank_key_is_no_input() {
    assert_eq!(key_action(Some("   "), "old", "new"), KeyAction::Move);
    assert_eq!(key_action(Some(""), "jun", "jun"), KeyAction::Keep);
}
#[test]
fn ka_all_16_combinations() {
    let mut n = 0;
    for key in [Some("KEY"), Some(""), Some("  "), None] {
        for old in ["old", ""] {
            for changed in [true, false] {
                let new = if changed { "new" } else { old };
                let has_key = matches!(key, Some(k) if !k.trim().is_empty());
                let expected = if has_key {
                    KeyAction::Save("KEY".into()) // KA-01
                } else if !old.is_empty() && changed {
                    KeyAction::Move // KA-02
                } else {
                    KeyAction::Keep // KA-03 / KA-04
                };
                assert_eq!(key_action(key, old, new), expected, "{key:?} {old:?} {changed}");
                n += 1;
            }
        }
    }
    assert_eq!(n, 16);
}

// ===== 4.4 設定ファイル・キャッシュ =====

#[test]
fn ls01_no_file() {
    assert_eq!(load_settings(&temp_dir("ls01")), Settings::default());
}
#[test]
fn ls02_roundtrip() {
    let dir = temp_dir("ls02");
    let s = Settings {
        crm_url: "https://x".into(),
        username: "u".into(),
        opacity: 70,
        reminder_minutes: vec![10, 1],
        activity_types: vec!["Call".into()],
        theme: "dark".into(),
        autostart: false,
        crm_time_zone: CrmTimeZone::Utc,
        ..Default::default()
    };
    save_settings(&dir, &s).unwrap();
    let text = std::fs::read_to_string(dir.join("config.json")).unwrap();
    assert!(text.contains(r#""crmTimeZone": "utc""#), "{text}");
    assert_eq!(load_settings(&dir), s);
    let _ = std::fs::remove_dir_all(&dir);
}
#[test]
fn ls03_partial_file_uses_defaults() {
    let dir = temp_dir("ls03");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.json"), r#"{"crmUrl":"https://y","username":"u"}"#).unwrap();
    let s = load_settings(&dir);
    assert_eq!(s.crm_url, "https://y");
    assert_eq!(s, Settings { crm_url: "https://y".into(), username: "u".into(), ..Default::default() });
    let _ = std::fs::remove_dir_all(&dir);
}
#[test]
fn ls07_unknown_time_zone_value_falls_back_to_defaults() {
    let dir = temp_dir("ls07");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.json"), r#"{"crmTimeZone":"local","username":"u"}"#).unwrap();
    assert_eq!(load_settings(&dir).crm_time_zone, CrmTimeZone::Local);
    std::fs::write(dir.join("config.json"), r#"{"crmTimeZone":"Asia/Tokyo","username":"u"}"#).unwrap();
    // 読めない値があるとファイル全体が読めないので既定値になる（LS-04 と同じ扱い）
    assert_eq!(load_settings(&dir), Settings::default());
    let _ = std::fs::remove_dir_all(&dir);
}
#[test]
fn ls04_broken_file() {
    let dir = temp_dir("ls04");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.json"), "{broken").unwrap();
    assert_eq!(load_settings(&dir), Settings::default());
    let _ = std::fs::remove_dir_all(&dir);
}
#[test]
fn ls05_out_of_range_normalized() {
    let dir = temp_dir("ls05");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.json"), r#"{"intervalMinutes":100,"opacity":1,"crmUrl":"https://y/"}"#).unwrap();
    let s = load_settings(&dir);
    assert_eq!((s.interval_minutes, s.opacity, s.crm_url.as_str()), (30, 50, "https://y"));
    let _ = std::fs::remove_dir_all(&dir);
}
#[test]
fn ls06_no_temp_file_left() {
    let dir = temp_dir("ls06");
    save_settings(&dir, &Settings::default()).unwrap();
    save_settings(&dir, &Settings::default()).unwrap(); // 上書きも
    assert!(dir.join("config.json").exists());
    assert!(!dir.join("config.json.tmp").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 27).unwrap()
}

const SRC: &str = "v2|https://x|u|Local";

fn sample_cache() -> Cache {
    let item = ScheduleItem {
        id: "18x1".into(),
        record_id: 1,
        subject: "s".into(),
        activity_type: "Call".into(),
        start: NaiveDateTime::parse_from_str("2026-10-27 09:00", "%Y-%m-%d %H:%M").unwrap(),
        end: NaiveDateTime::parse_from_str("2026-10-27 10:00", "%Y-%m-%d %H:%M").unwrap(),
        all_day: false,
        status: "Planned".into(),
        location: Some("A".into()),
        modified_time: String::new(),
        badge: None,
        rooms: Vec::new(),
    };
    Cache { source: SRC.into(), date: today(), last_updated: Local::now(), items: vec![item] }
}

#[test]
fn ca01_no_cache() {
    assert!(load_cache(&temp_dir("ca01"), today(), SRC).is_none());
}
#[test]
fn ca02_same_day() {
    let dir = temp_dir("ca02");
    let c = sample_cache();
    save_cache(&dir, &c).unwrap();
    assert_eq!(load_cache(&dir, today(), SRC), Some(c));
    let _ = std::fs::remove_dir_all(&dir);
}
#[test]
fn ca03_other_day() {
    let dir = temp_dir("ca03");
    save_cache(&dir, &sample_cache()).unwrap();
    assert!(load_cache(&dir, today().succ_opt().unwrap(), SRC).is_none());
    assert!(load_cache(&dir, today().pred_opt().unwrap(), SRC).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
#[test]
fn ca04_broken_cache() {
    let dir = temp_dir("ca04");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("cache.json"), r#"{"date":"2026-10-27"}"#).unwrap();
    assert!(load_cache(&dir, today(), SRC).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
#[test]
fn ca05_cache_from_other_conditions_is_ignored() {
    let dir = temp_dir("ca05");
    save_cache(&dir, &sample_cache()).unwrap();
    for other in ["v2|https://x|u|Utc", "v2|https://y|u|Local", "v2|https://x|other|Local", "v1|https://x|u|Local", ""] {
        assert!(load_cache(&dir, today(), other).is_none(), "{other:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
#[test]
fn ca06_old_cache_without_source_is_ignored() {
    // v0.1.3 より前の版が保存したキャッシュ（source がない）
    let dir = temp_dir("ca06");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("cache.json"),
        r#"{"date":"2026-10-27","lastUpdated":"2026-10-27T16:27:48+09:00","items":[]}"#,
    )
    .unwrap();
    assert!(load_cache(&dir, today(), SRC).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
#[test]
fn cs01_cache_source_reflects_conditions() {
    let s = Settings { crm_url: "https://x".into(), username: "u".into(), ..Default::default() };
    assert_eq!(s.cache_source(), format!("v{CACHE_FORMAT}|https://x|u|Local"));
    let utc = Settings { crm_time_zone: CrmTimeZone::Utc, ..s.clone() };
    assert_ne!(utc.cache_source(), s.cache_source());
}
#[test]
fn fi01_filter_from_settings() {
    let s = Settings { activity_types: vec!["Call".into()], show_held: false, show_not_held: true, ..Default::default() };
    let f = s.filter();
    assert_eq!(f.activity_types, vec!["Call".to_string()]);
    assert!(!f.show_held);
    assert!(f.show_not_held);
    let s2 = Settings { show_held: true, show_not_held: false, ..Default::default() };
    assert!(s2.filter().show_held && !s2.filter().show_not_held);
}
