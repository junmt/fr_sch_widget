//! テスト仕様書 2章（schedule.rs）。関数名の先頭はテストIDに対応する。
use super::*;
use chrono::FixedOffset;
use serde_json::json;

fn jst() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).unwrap()
}

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn dt(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
}

/// UTC の値でレコードを作る（query の返り値と同じ形）
fn rec(id: &str, ds: &str, ts: &str, de: &str, te: &str) -> Value {
    json!({
        "id": id, "subject": format!("件名{id}"), "activitytype": "Meeting",
        "date_start": ds, "time_start": ts, "due_date": de, "time_end": te,
        "eventstatus": "Planned", "location": "", "modifiedtime": "2026-10-27 00:00:00"
    })
}

/// 標準のレコード（UTC 10/27 00:00-01:00 = JST 09:00-10:00）
fn std_rec() -> Value {
    rec("18x1234", "2026-10-27", "00:00:00", "2026-10-27", "01:00:00")
}

fn item(id: &str, start: &str, end: &str) -> ScheduleItem {
    ScheduleItem {
        id: id.into(),
        record_id: id.rsplit_once('x').map(|(_, n)| n.parse().unwrap()).unwrap_or(0),
        subject: format!("件名{id}"),
        activity_type: "Meeting".into(),
        start: dt(start),
        end: dt(end),
        all_day: false,
        status: "Planned".into(),
        location: None,
        modified_time: String::new(),
        badge: None,
    }
}

fn with_status(mut i: ScheduleItem, s: &str) -> ScheduleItem {
    i.status = s.into();
    i
}

fn ids(items: &[ScheduleItem]) -> Vec<&str> {
    items.iter().map(|i| i.id.as_str()).collect()
}

// ===== 2.1 query_range =====

#[test]
fn qr01_normal() {
    assert_eq!(query_range(d("2026-10-27")), (d("2026-10-26"), d("2026-10-28")));
}
#[test]
fn qr02_month_boundary() {
    assert_eq!(query_range(d("2026-11-01")), (d("2026-10-31"), d("2026-11-02")));
}
#[test]
fn qr03_year_boundary() {
    assert_eq!(query_range(d("2026-01-01")), (d("2025-12-31"), d("2026-01-02")));
}
#[test]
fn qr04_leap_day() {
    assert_eq!(query_range(d("2028-02-29")), (d("2028-02-28"), d("2028-03-01")));
}

// ===== 2.2 build_query =====

const FIELDS: &str =
    "id, subject, activitytype, date_start, time_start, due_date, time_end, eventstatus, location, visibility, modifiedtime";

#[test]
fn bq01_explicit_fields_with_offsets() {
    for offset in [0, 200] {
        assert_eq!(
            build_query("19x5", d("2026-10-26"), d("2026-10-28"), offset, false),
            format!(
                "SELECT {FIELDS} FROM Events WHERE assigned_user_id = '19x5' AND date_start <= '2026-10-28' AND due_date >= '2026-10-26' LIMIT {offset}, 100;"
            )
        );
    }
}
#[test]
fn bq02_select_all() {
    assert_eq!(
        build_query("19x5", d("2026-10-26"), d("2026-10-28"), 0, true),
        "SELECT * FROM Events WHERE assigned_user_id = '19x5' AND date_start <= '2026-10-28' AND due_date >= '2026-10-26' LIMIT 0, 100;"
    );
}
#[test]
fn bq03_quotes_removed_from_user_id() {
    let q = build_query("19x5' OR '1'='1", d("2026-10-26"), d("2026-10-28"), 0, false);
    assert!(q.contains("assigned_user_id = '19x5 OR 1=1' AND"), "{q}");
    assert_eq!(q.matches('\'').count(), 6, "{q}");
}

// ===== 2.3 parse_record =====

#[test]
fn pr01_invalid_id_returns_none() {
    for (case, val) in [("a", json!(null)), ("b", json!("")), ("c", json!("1234")), ("d", json!("18xabc")), ("e", json!("18x")), ("f", json!(1234))] {
        let mut r = std_rec();
        r["id"] = val;
        assert!(parse_record(&r, CrmTimeZone::Utc, &jst()).is_none(), "PR-01{case}");
    }
}
#[test]
fn pr02_invalid_date_start_returns_none() {
    for (case, val) in [("a", json!(null)), ("b", json!("")), ("c", json!("2026/10/27")), ("d", json!("2026-13-01")), ("e", json!("2026-02-30"))] {
        let mut r = std_rec();
        r["date_start"] = val;
        assert!(parse_record(&r, CrmTimeZone::Utc, &jst()).is_none(), "PR-02{case}");
    }
}
#[test]
fn pr03_valid_record_converted_to_local() {
    let i = parse_record(&std_rec(), CrmTimeZone::Utc, &jst()).unwrap();
    assert_eq!(i.start, dt("2026-10-27 09:00"));
    assert_eq!(i.end, dt("2026-10-27 10:00"));
    assert!(!i.all_day);
    assert_eq!(i.subject, "件名18x1234");
    assert_eq!(i.activity_type, "Meeting");
    assert_eq!(i.status, "Planned");
    assert_eq!(i.modified_time, "2026-10-27 00:00:00");
    assert_eq!(i.badge, None);
}
#[test]
fn pr04_time_in_hh_mm() {
    let i = parse_record(&rec("18x1", "2026-10-27", "01:30", "2026-10-27", "02:00"), CrmTimeZone::Utc, &jst()).unwrap();
    assert_eq!(i.start, dt("2026-10-27 10:30"));
    assert_eq!(i.end, dt("2026-10-27 11:00"));
}
#[test]
fn pr05_empty_or_invalid_time_start_is_utc_midnight() {
    for ts in ["", "25:00:00"] {
        let i = parse_record(&rec("18x1", "2026-10-27", ts, "2026-10-27", "01:00:00"), CrmTimeZone::Utc, &jst()).unwrap();
        assert_eq!(i.start, dt("2026-10-27 09:00"), "{ts:?}");
    }
}
#[test]
fn pr06_invalid_due_date_uses_start_date() {
    for de in ["", "xx"] {
        let i = parse_record(&rec("18x1", "2026-10-27", "01:00:00", de, "02:00:00"), CrmTimeZone::Utc, &jst()).unwrap();
        assert_eq!(i.end, dt("2026-10-27 11:00"), "{de:?}");
    }
}
#[test]
fn pr07_invalid_time_end_is_start_plus_30_minutes() {
    for te in [json!(""), json!(null), json!("99:99")] {
        let mut r = rec("18x1", "2026-10-27", "01:00:00", "2026-10-28", "");
        r["time_end"] = te.clone();
        let i = parse_record(&r, CrmTimeZone::Utc, &jst()).unwrap();
        // due_date が別の日でも、終了時刻がなければ開始 + 30分
        assert_eq!(i.end, dt("2026-10-27 10:30"), "{te}");
    }
}
#[test]
fn pr08_end_before_start_is_clamped() {
    let i = parse_record(&rec("18x1", "2026-10-27", "02:00:00", "2026-10-27", "01:00:00"), CrmTimeZone::Utc, &jst()).unwrap();
    assert_eq!(i.start, dt("2026-10-27 11:00"));
    assert_eq!(i.end, dt("2026-10-27 11:00"));
}
#[test]
fn pr09_local_midnight_both_is_all_day() {
    // JST 10/27 00:00 - 10/28 00:00 = UTC 10/26 15:00 - 10/27 15:00
    let i = parse_record(&rec("18x1", "2026-10-26", "15:00:00", "2026-10-27", "15:00:00"), CrmTimeZone::Utc, &jst()).unwrap();
    assert!(i.all_day);
    assert_eq!(i.start, dt("2026-10-27 00:00"));
    assert_eq!(i.end, dt("2026-10-28 00:00"));
}
#[test]
fn pr10_utc_midnight_is_not_all_day() {
    let i = parse_record(&rec("18x1", "2026-10-27", "00:00:00", "2026-10-28", "00:00:00"), CrmTimeZone::Utc, &jst()).unwrap();
    assert!(!i.all_day);
}
#[test]
fn pr11_only_start_at_local_midnight_is_not_all_day() {
    let i = parse_record(&rec("18x1", "2026-10-26", "15:00:00", "2026-10-26", "16:00:00"), CrmTimeZone::Utc, &jst()).unwrap();
    assert_eq!(i.start, dt("2026-10-27 00:00"));
    assert!(!i.all_day);
}
#[test]
fn pr12_early_morning_is_previous_utc_day() {
    let i = parse_record(&rec("18x1", "2026-10-26", "23:00:00", "2026-10-26", "23:30:00"), CrmTimeZone::Utc, &jst()).unwrap();
    assert_eq!(i.start, dt("2026-10-27 08:00"));
    assert_eq!(i.end, dt("2026-10-27 08:30"));
}
#[test]
fn pr13_html_entities_are_decoded() {
    let mut r = std_rec();
    r["subject"] = json!("A&amp;B &lt;定例&gt;");
    r["location"] = json!("会議室&amp;B");
    r["activitytype"] = json!("&quot;Call&quot;");
    r["eventstatus"] = json!("Not&nbsp;Held");
    let i = parse_record(&r, CrmTimeZone::Utc, &jst()).unwrap();
    assert_eq!(i.subject, "A&B <定例>");
    assert_eq!(i.location.as_deref(), Some("会議室&B"));
    assert_eq!(i.activity_type, "\"Call\"");
    assert_eq!(i.status, "Not Held");
}
#[test]
fn pr14_empty_location_is_none() {
    assert_eq!(parse_record(&std_rec(), CrmTimeZone::Utc, &jst()).unwrap().location, None);
    let mut r = std_rec();
    r["location"] = json!("   ");
    assert_eq!(parse_record(&r, CrmTimeZone::Utc, &jst()).unwrap().location, None);
}
#[test]
fn pr15_non_string_fields_are_empty() {
    let mut r = std_rec();
    r["subject"] = json!(123);
    r["activitytype"] = json!(null);
    let i = parse_record(&r, CrmTimeZone::Utc, &jst()).unwrap();
    assert_eq!(i.subject, "");
    assert_eq!(i.activity_type, "");
}
#[test]
fn pr16_surrounding_spaces_are_trimmed() {
    let i = parse_record(&rec(" 18x1 ", " 2026-10-27 ", " 00:00:00 ", " 2026-10-27 ", " 01:00:00 "), CrmTimeZone::Utc, &jst()).unwrap();
    assert_eq!(i.id, "18x1");
    assert_eq!(i.start, dt("2026-10-27 09:00"));
    assert_eq!(i.end, dt("2026-10-27 10:00"));
}
#[test]
fn pr17_record_id_is_number_after_x() {
    let i = parse_record(&std_rec(), CrmTimeZone::Utc, &jst()).unwrap();
    assert_eq!(i.id, "18x1234");
    assert_eq!(i.record_id, 1234);
}

#[test]
fn pr18_local_time_zone_is_not_converted() {
    // F-RevoCRM の既定（Asia/Tokyo で保存）: query の値をそのまま使う
    let i = parse_record(&std_rec(), CrmTimeZone::Local, &jst()).unwrap();
    assert_eq!(i.start, dt("2026-10-27 00:00"));
    assert_eq!(i.end, dt("2026-10-27 01:00"));
    assert!(!i.all_day);
}
#[test]
fn pr19_local_all_day_and_defaults() {
    let i = parse_record(&rec("18x1", "2026-10-27", "00:00:00", "2026-10-28", "00:00:00"), CrmTimeZone::Local, &jst()).unwrap();
    assert!(i.all_day);
    let j = parse_record(&rec("18x2", "2026-10-27", "15:00:00", "2026-10-27", ""), CrmTimeZone::Local, &jst()).unwrap();
    assert_eq!((j.start, j.end), (dt("2026-10-27 15:00"), dt("2026-10-27 15:30")));
    assert!(!j.all_day);
}

// ===== 2.4 decode_html =====

#[test]
fn dh01_all_entities() {
    assert_eq!(decode_html("&lt; &gt; &quot; &#039; &#39; &nbsp; &amp;"), "< > \" ' '   &");
}
#[test]
fn dh02_no_double_decode() {
    assert_eq!(decode_html("&amp;lt;"), "&lt;");
}
#[test]
fn dh03_plain_text_unchanged() {
    assert_eq!(decode_html("打合せ"), "打合せ");
}
#[test]
fn dh04_empty() {
    assert_eq!(decode_html(""), "");
}

// ===== 2.5 overlaps_day =====

fn od(start: &str, end: &str) -> bool {
    overlaps_day(&item("18x1", start, end), d("2026-10-27"))
}

#[test]
fn od01_overlaps_today() {
    assert!(od("2026-10-27 09:00", "2026-10-27 10:00"), "a");
    assert!(od("2026-10-26 23:00", "2026-10-27 00:01"), "b");
    assert!(od("2026-10-27 23:59", "2026-10-28 01:00"), "c");
    assert!(od("2026-10-25 09:00", "2026-10-30 18:00"), "d");
    assert!(od("2026-10-27 00:00", "2026-10-28 00:00"), "e");
}
#[test]
fn od02_ends_at_or_before_today_start() {
    assert!(!od("2026-10-26 23:00", "2026-10-27 00:00"), "a");
    assert!(!od("2026-10-26 12:00", "2026-10-26 13:00"), "b");
}
#[test]
fn od03_starts_at_or_after_tomorrow() {
    assert!(!od("2026-10-28 00:00", "2026-10-28 01:00"), "a");
    assert!(!od("2026-10-29 09:00", "2026-10-29 10:00"));
}
// OD-04: 不可能な組み合わせ（テスト仕様書 2.5 の注を参照）
#[test]
fn od05_zero_length_inside_today() {
    assert!(od("2026-10-27 00:00", "2026-10-27 00:00"), "a");
    assert!(od("2026-10-27 23:59", "2026-10-27 23:59"), "b");
}
#[test]
fn od06_zero_length_tomorrow() {
    assert!(!od("2026-10-28 00:00", "2026-10-28 00:00"));
}
#[test]
fn od07_zero_length_yesterday() {
    assert!(!od("2026-10-26 23:59", "2026-10-26 23:59"));
}

// ===== 2.6 Filter::passes =====

#[derive(Clone, Copy, Debug)]
enum St {
    H,
    Nh,
    P,
}
#[derive(Clone, Copy, Debug)]
enum Ty {
    E,
    M,
    U,
}

fn filter_case(st: St, show_held: bool, show_not_held: bool, ty: Ty) -> bool {
    let status = match st {
        St::H => "Held",
        St::Nh => "Not Held",
        St::P => "Planned",
    };
    let types = match ty {
        Ty::E => vec![],
        Ty::M => vec!["Meeting".to_string()],
        Ty::U => vec!["Call".to_string()],
    };
    let f = Filter { activity_types: types, show_held, show_not_held };
    f.passes(&with_status(item("18x1", "2026-10-27 09:00", "2026-10-27 10:00"), status))
}

#[test]
fn fl01_held_hidden() {
    for nh in [true, false] {
        for ty in [Ty::E, Ty::M, Ty::U] {
            assert!(!filter_case(St::H, false, nh, ty), "{nh} {ty:?}");
        }
    }
}
#[test]
fn fl02_held_shown() {
    for nh in [true, false] {
        for ty in [Ty::E, Ty::M] {
            assert!(filter_case(St::H, true, nh, ty), "{nh} {ty:?}");
        }
    }
}
#[test]
fn fl03_not_held_hidden() {
    for h in [true, false] {
        for ty in [Ty::E, Ty::M, Ty::U] {
            assert!(!filter_case(St::Nh, h, false, ty), "{h} {ty:?}");
        }
    }
}
#[test]
fn fl04_not_held_shown() {
    for h in [true, false] {
        for ty in [Ty::E, Ty::M] {
            assert!(filter_case(St::Nh, h, true, ty), "{h} {ty:?}");
        }
    }
}
#[test]
fn fl05_planned_shown() {
    for (h, nh) in [(true, true), (true, false), (false, true), (false, false)] {
        for ty in [Ty::E, Ty::M] {
            assert!(filter_case(St::P, h, nh, ty), "{h} {nh} {ty:?}");
        }
    }
}
#[test]
fn fl06_type_mismatch_hidden() {
    for st in [St::H, St::Nh, St::P] {
        for (h, nh) in [(true, true), (true, false), (false, true), (false, false)] {
            assert!(!filter_case(st, h, nh, Ty::U), "{st:?} {h} {nh}");
        }
    }
}
#[test]
fn fl07_all_36_combinations_follow_the_rules() {
    let mut n = 0;
    for st in [St::H, St::Nh, St::P] {
        for h in [true, false] {
            for nh in [true, false] {
                for ty in [Ty::E, Ty::M, Ty::U] {
                    // デシジョン表のルールを上から順にあてはめる
                    let expected = match (st, h, nh, ty) {
                        (St::H, false, _, _) => false,  // FL-01
                        (St::Nh, _, false, _) => false, // FL-03
                        (_, _, _, Ty::U) => false,      // FL-06
                        _ => true,                      // FL-02 / FL-04 / FL-05
                    };
                    assert_eq!(filter_case(st, h, nh, ty), expected, "{st:?} {h} {nh} {ty:?}");
                    n += 1;
                }
            }
        }
    }
    assert_eq!(n, 36);
}
#[test]
fn fl08_status_case_and_spaces() {
    let f = Filter { activity_types: vec![], show_held: false, show_not_held: false };
    for s in [" not held ", "NOT HELD", "HELD", " held"] {
        assert!(!f.passes(&with_status(item("18x1", "2026-10-27 09:00", "2026-10-27 10:00"), s)), "{s:?}");
    }
}
#[test]
fn fl09_type_case_and_spaces() {
    let f = Filter { activity_types: vec![" call ".into()], show_held: true, show_not_held: true };
    let mut i = item("18x1", "2026-10-27 09:00", "2026-10-27 10:00");
    i.activity_type = " Call".into();
    assert!(f.passes(&i));
}
#[test]
fn fl10_multiple_types() {
    let f = Filter { activity_types: vec!["Call".into(), "Meeting".into()], show_held: true, show_not_held: true };
    assert!(f.passes(&item("18x1", "2026-10-27 09:00", "2026-10-27 10:00")));
}

// ===== 2.7 build_today / sort_items =====

fn all() -> Filter {
    Filter { activity_types: vec![], show_held: true, show_not_held: true }
}

#[test]
fn bt01_mixed_records() {
    let records = vec![
        rec("18x3", "2026-10-27", "05:00:00", "2026-10-27", "06:00:00"), // JST 14:00
        rec("18x1", "2026-10-27", "00:00:00", "2026-10-27", "01:00:00"), // JST 09:00
        rec("18x2", "2026-10-26", "15:00:00", "2026-10-27", "15:00:00"), // 終日
        rec("18x9", "2026-10-26", "03:00:00", "2026-10-26", "04:00:00"), // 昨日
        rec("18x8", "2026-10-27", "15:00:00", "2026-10-27", "16:00:00"), // 明日 00:00
        rec("18x1", "2026-10-27", "00:00:00", "2026-10-27", "01:00:00"), // 重複
        json!({"id": "broken"}),
    ];
    let items = build_today(&records, CrmTimeZone::Utc, &jst(), d("2026-10-27"), &all());
    assert_eq!(ids(&items), vec!["18x2", "18x1", "18x3"]);
}
#[test]
fn bt05_local_time_zone_reported_case() {
    // 不具合の再現: CRM の 10/26 15:00-17:30 が、UTC とみなすと 10/27 00:00-02:30 に化けていた
    let records = vec![
        rec("18x1", "2026-10-26", "15:00:00", "2026-10-26", "17:30:00"),
        rec("18x2", "2026-10-27", "11:00:00", "2026-10-27", "12:00:00"),
        rec("18x3", "2026-10-27", "18:00:00", "2026-10-27", "18:30:00"),
    ];
    let items = build_today(&records, CrmTimeZone::Local, &jst(), d("2026-10-27"), &all());
    assert_eq!(ids(&items), vec!["18x2", "18x3"]);
    assert_eq!((items[0].start, items[0].end), (dt("2026-10-27 11:00"), dt("2026-10-27 12:00")));
    assert_eq!(items[1].start, dt("2026-10-27 18:00"));
    // 同じデータを UTC とみなすと9時間ずれる（設定を間違えた場合）
    let wrong = build_today(&records, CrmTimeZone::Utc, &jst(), d("2026-10-27"), &all());
    assert_eq!(ids(&wrong), vec!["18x1", "18x2"]);
    assert_eq!(wrong[0].start, dt("2026-10-27 00:00"));
}
#[test]
fn bt02_empty() {
    assert!(build_today(&[], CrmTimeZone::Utc, &jst(), d("2026-10-27"), &all()).is_empty());
}
#[test]
fn bt03_filter_applied() {
    let mut call = rec("18x2", "2026-10-27", "02:00:00", "2026-10-27", "03:00:00");
    call["activitytype"] = json!("Call");
    let records = vec![std_rec(), call];
    let f = Filter { activity_types: vec!["Call".into()], show_held: true, show_not_held: true };
    assert_eq!(ids(&build_today(&records, CrmTimeZone::Utc, &jst(), d("2026-10-27"), &f)), vec!["18x2"]);
}
#[test]
fn bt04_duplicates_keep_first() {
    let mut second = std_rec();
    second["subject"] = json!("2件目");
    let items = build_today(&[std_rec(), second], CrmTimeZone::Utc, &jst(), d("2026-10-27"), &all());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].subject, "件名18x1234");
}

fn sorted(mut v: Vec<ScheduleItem>) -> Vec<String> {
    sort_items(&mut v);
    v.into_iter().map(|i| i.id).collect()
}

#[test]
fn so01_all_day_first() {
    let mut a = item("18x1", "2026-10-27 00:00", "2026-10-28 00:00");
    a.all_day = true;
    let b = item("18x2", "2026-10-26 23:00", "2026-10-27 08:00"); // 開始は早いが終日ではない
    assert_eq!(sorted(vec![b, a]), vec!["18x1", "18x2"]);
}
#[test]
fn so02_by_start() {
    let a = item("18x1", "2026-10-27 10:00", "2026-10-27 11:00");
    let b = item("18x2", "2026-10-27 09:00", "2026-10-27 12:00");
    assert_eq!(sorted(vec![a, b]), vec!["18x2", "18x1"]);
}
#[test]
fn so03_by_end_when_same_start() {
    let a = item("18x1", "2026-10-27 09:00", "2026-10-27 11:00");
    let b = item("18x2", "2026-10-27 09:00", "2026-10-27 10:00");
    assert_eq!(sorted(vec![a, b]), vec!["18x2", "18x1"]);
}
#[test]
fn so04_by_subject_when_same_time() {
    let mut a = item("18x1", "2026-10-27 09:00", "2026-10-27 10:00");
    a.subject = "い".into();
    let mut b = item("18x2", "2026-10-27 09:00", "2026-10-27 10:00");
    b.subject = "あ".into();
    assert_eq!(sorted(vec![a, b]), vec!["18x2", "18x1"]);
}

// ===== 2.8 diff =====

fn base() -> ScheduleItem {
    item("18x1", "2026-10-27 09:00", "2026-10-27 10:00")
}

#[test]
fn df01_added() {
    let d = diff(&[], &[base()]);
    assert_eq!(ids(&d.added), vec!["18x1"]);
    assert!(d.changed.is_empty() && d.removed.is_empty());
}
#[test]
fn df02_removed() {
    let d = diff(&[base()], &[]);
    assert_eq!(ids(&d.removed), vec!["18x1"]);
    assert!(d.added.is_empty() && d.changed.is_empty());
}
#[test]
fn df03_start_changed() {
    let new = item("18x1", "2026-10-27 09:30", "2026-10-27 10:00");
    let d = diff(&[base()], &[new.clone()]);
    assert_eq!(d.changed, vec![(base(), new)]);
    assert!(d.added.is_empty() && d.removed.is_empty());
}
#[test]
fn df04_end_changed() {
    let new = item("18x1", "2026-10-27 09:00", "2026-10-27 10:30");
    assert_eq!(diff(&[base()], &[new.clone()]).changed, vec![(base(), new)]);
}
#[test]
fn df05_both_changed() {
    let new = item("18x1", "2026-10-27 11:00", "2026-10-27 12:00");
    assert_eq!(diff(&[base()], &[new.clone()]).changed, vec![(base(), new)]);
}
#[test]
fn df06_only_other_fields_changed() {
    let mut new = base();
    new.subject = "件名だけ変更".into();
    new.location = Some("場所".into());
    new.status = "Held".into();
    assert!(diff(&[base()], &[new]).is_empty());
}
#[test]
fn df07_both_empty() {
    assert!(diff(&[], &[]).is_empty());
}
#[test]
fn df08_one_side_empty() {
    assert_eq!(diff(&[], &[base()]).added.len(), 1);
    assert_eq!(diff(&[base()], &[]).removed.len(), 1);
}
#[test]
fn df09_idempotent() {
    let list = vec![base(), item("18x2", "2026-10-27 11:00", "2026-10-27 12:00")];
    assert!(diff(&list, &list).is_empty());
    assert!(diff(&list, &list).is_empty());
}

// ===== 2.9 badges =====

fn t(s: &str) -> NaiveDateTime {
    dt(&format!("2026-10-27 {s}"))
}

fn added(i: &ScheduleItem) -> Diff {
    Diff { added: vec![i.clone()], ..Default::default() }
}
fn changed(i: &ScheduleItem) -> Diff {
    Diff { changed: vec![(i.clone(), i.clone())], ..Default::default() }
}
fn removed(i: &ScheduleItem) -> Diff {
    Diff { removed: vec![i.clone()], ..Default::default() }
}

#[test]
fn bd01_added_without_badge() {
    let mut b = Badges::new();
    record_badges(&mut b, &added(&base()), t("10:00"));
    assert_eq!(b.get("18x1"), Some(&(Badge::New, t("10:00"))));
}
#[test]
fn bd02_added_overwrites_existing() {
    let mut b = Badges::from([("18x1".to_string(), (Badge::Changed, t("09:00")))]);
    record_badges(&mut b, &added(&base()), t("10:00"));
    assert_eq!(b.get("18x1"), Some(&(Badge::New, t("10:00"))));
}
#[test]
fn bd03_changed_without_badge() {
    let mut b = Badges::new();
    record_badges(&mut b, &changed(&base()), t("10:00"));
    assert_eq!(b.get("18x1"), Some(&(Badge::Changed, t("10:00"))));
}
#[test]
fn bd04_changed_keeps_new() {
    let mut b = Badges::from([("18x1".to_string(), (Badge::New, t("09:00")))]);
    record_badges(&mut b, &changed(&base()), t("10:00"));
    assert_eq!(b.get("18x1"), Some(&(Badge::New, t("09:00"))));
}
#[test]
fn bd05_changed_again_refreshes_time() {
    let mut b = Badges::from([("18x1".to_string(), (Badge::Changed, t("09:00")))]);
    record_badges(&mut b, &changed(&base()), t("10:00"));
    assert_eq!(b.get("18x1"), Some(&(Badge::Changed, t("10:00"))));
}
#[test]
fn bd06_removed_clears_badge() {
    let mut b = Badges::from([("18x1".to_string(), (Badge::New, t("09:00")))]);
    record_badges(&mut b, &removed(&base()), t("10:00"));
    assert!(b.is_empty());
}
#[test]
fn bd07_removed_without_badge() {
    let mut b = Badges::from([("18x9".to_string(), (Badge::New, t("09:00")))]);
    record_badges(&mut b, &removed(&base()), t("10:00"));
    assert_eq!(b.len(), 1);
}

fn one_badge() -> Badges {
    Badges::from([("18x1".to_string(), (Badge::New, t("10:00")))])
}

#[test]
fn ex01_before_ttl() {
    let mut b = one_badge();
    assert!(!expire_badges(&mut b, t("10:29") + Duration::seconds(59)));
    assert_eq!(b.len(), 1);
}
#[test]
fn ex02_exactly_ttl() {
    let mut b = one_badge();
    assert!(expire_badges(&mut b, t("10:30")));
    assert!(b.is_empty());
}
#[test]
fn ex03_after_ttl() {
    let mut b = one_badge();
    assert!(expire_badges(&mut b, t("10:31")));
    assert!(b.is_empty());
}
#[test]
fn ex04_mixed() {
    let mut b = one_badge();
    b.insert("18x2".into(), (Badge::Changed, t("10:20")));
    assert!(expire_badges(&mut b, t("10:30")));
    assert_eq!(b.keys().collect::<Vec<_>>(), vec!["18x2"]);
}
#[test]
fn ex05_no_badges() {
    let mut b = Badges::new();
    assert!(!expire_badges(&mut b, t("10:30")));
}
#[test]
fn ab01_ab02_apply_badges() {
    let mut old = item("18x2", "2026-10-27 11:00", "2026-10-27 12:00");
    old.badge = Some(Badge::Changed);
    let mut items = vec![base(), old];
    apply_badges(&mut items, &one_badge());
    assert_eq!(items[0].badge, Some(Badge::New)); // AB-01
    assert_eq!(items[1].badge, None); // AB-02
}

// ===== 2.10 due_reminders =====

fn at(s: &str) -> NaiveDateTime {
    t(s)
}

fn upcoming() -> ScheduleItem {
    item("18x1", "2026-10-27 14:00", "2026-10-27 15:00")
}

fn remind(items: &[ScheduleItem], now: NaiveDateTime, offsets: &[u32], fired: &mut HashSet<String>) -> Vec<(String, i64)> {
    due_reminders(items, now, offsets, fired).into_iter().map(|(i, m)| (i.id, m)).collect()
}

#[test]
fn rm01_all_day_not_reminded() {
    let mut i = item("18x1", "2026-10-28 00:00", "2026-10-29 00:00");
    i.all_day = true;
    let mut f = HashSet::new();
    assert!(remind(&[i], dt("2026-10-27 23:55"), &[10], &mut f).is_empty());
    assert!(f.is_empty());
}
#[test]
fn rm02_held_not_reminded() {
    let mut f = HashSet::new();
    assert!(remind(&[with_status(upcoming(), "Held")], at("13:55"), &[10], &mut f).is_empty());
}
#[test]
fn rm03_not_held_not_reminded() {
    let mut f = HashSet::new();
    assert!(remind(&[with_status(upcoming(), "Not Held")], at("13:55"), &[10], &mut f).is_empty());
}
#[test]
fn rm04_already_started() {
    let mut f = HashSet::new();
    assert!(remind(&[upcoming()], at("14:30"), &[10], &mut f).is_empty());
    assert!(f.is_empty());
}
#[test]
fn rm05_before_window() {
    let mut f = HashSet::new();
    assert!(remind(&[upcoming()], at("13:49"), &[10], &mut f).is_empty());
    assert!(f.is_empty());
}
#[test]
fn rm06_reached_not_fired() {
    let mut f = HashSet::new();
    assert_eq!(remind(&[upcoming()], at("13:50"), &[10], &mut f), vec![("18x1".into(), 10)]);
    assert_eq!(f.len(), 1);
}
#[test]
fn rm07_reached_already_fired() {
    let mut f = HashSet::new();
    remind(&[upcoming()], at("13:50"), &[10], &mut f);
    assert!(remind(&[upcoming()], at("13:51"), &[10], &mut f).is_empty());
}
#[test]
fn rm08_window_boundary() {
    let mut f = HashSet::new();
    assert!(remind(&[upcoming()], at("13:50") - Duration::seconds(1), &[10], &mut f).is_empty());
    assert_eq!(remind(&[upcoming()], at("13:50"), &[10], &mut f).len(), 1);
}
#[test]
fn rm09_multiple_offsets() {
    let mut f = HashSet::new();
    let mut fired_at = vec![];
    for now in ["13:50", "13:55", "13:59"] {
        if !remind(&[upcoming()], at(now), &[10, 1], &mut f).is_empty() {
            fired_at.push(now);
        }
    }
    assert_eq!(fired_at, vec!["13:50", "13:59"]);
}
#[test]
fn rm10_after_sleep_once() {
    let mut f = HashSet::new();
    let now = at("13:59") + Duration::seconds(30);
    assert_eq!(remind(&[upcoming()], now, &[10, 1], &mut f), vec![("18x1".into(), 1)]);
    assert!(remind(&[upcoming()], now + Duration::seconds(10), &[10, 1], &mut f).is_empty());
}
#[test]
fn rm11_start_changed_reminds_again() {
    let mut f = HashSet::new();
    remind(&[upcoming()], at("13:50"), &[10], &mut f);
    let moved = item("18x1", "2026-10-27 14:05", "2026-10-27 15:00");
    assert_eq!(remind(&[moved], at("13:56"), &[10], &mut f).len(), 1);
}
#[test]
fn rm12_no_offsets() {
    let mut f = HashSet::new();
    assert!(remind(&[upcoming()], at("13:59"), &[], &mut f).is_empty());
}
#[test]
fn rm13_offset_zero_never() {
    let mut f = HashSet::new();
    assert!(remind(&[upcoming()], at("13:59"), &[0], &mut f).is_empty());
    assert!(remind(&[upcoming()], at("14:00"), &[0], &mut f).is_empty());
}
#[test]
fn rm14_exactly_at_start() {
    let mut f = HashSet::new();
    assert!(remind(&[upcoming()], at("14:00"), &[10], &mut f).is_empty());
}
#[test]
fn rm15_remaining_minutes_rounded_up() {
    for (now, expected) in [
        (at("13:50"), 10),
        (at("13:50") + Duration::seconds(59), 10),
        (at("13:59") + Duration::seconds(30), 1),
    ] {
        let mut f = HashSet::new();
        assert_eq!(remind(&[upcoming()], now, &[10], &mut f)[0].1, expected, "{now}");
    }
}
#[test]
fn rm16_idempotent() {
    let mut f = HashSet::new();
    assert_eq!(remind(&[upcoming()], at("13:50"), &[10], &mut f).len(), 1);
    assert!(remind(&[upcoming()], at("13:50"), &[10], &mut f).is_empty());
}

// ===== 2.11 next_item =====

#[test]
fn nx01_upcoming_is_candidate() {
    assert_eq!(next_item(&[upcoming()], at("13:00")).map(|i| i.id.as_str()), Some("18x1"));
}
#[test]
fn nx02_not_remindable_excluded() {
    let mut all_day = item("18x0", "2026-10-27 15:00", "2026-10-28 00:00");
    all_day.all_day = true;
    let items = vec![all_day, with_status(upcoming(), "Held"), with_status(item("18x2", "2026-10-27 14:00", "2026-10-27 15:00"), "Not Held")];
    assert_eq!(next_item(&items, at("13:00")), None);
}
#[test]
fn nx03_started_excluded() {
    assert_eq!(next_item(&[upcoming()], at("14:00")), None);
    assert_eq!(next_item(&[upcoming()], at("14:30")), None);
    assert_eq!(next_item(&[upcoming()], at("15:30")), None);
}
#[test]
fn nx04_earliest_of_many() {
    let items = vec![
        item("18x3", "2026-10-27 16:00", "2026-10-27 17:00"),
        item("18x2", "2026-10-27 11:00", "2026-10-27 12:00"),
        item("18x4", "2026-10-27 13:00", "2026-10-27 14:00"),
    ];
    assert_eq!(next_item(&items, at("10:00")).map(|i| i.id.as_str()), Some("18x2"));
}
#[test]
fn nx05_none() {
    assert_eq!(next_item(&[], at("10:00")), None);
}

// ===== シリアライズ（画面・キャッシュとの受け渡し） =====

#[test]
fn item_serializes_with_minute_precision_and_roundtrips() {
    let mut i = base();
    i.badge = Some(Badge::New);
    let v = serde_json::to_value(&i).unwrap();
    assert_eq!(v["start"], "2026-10-27T09:00");
    assert_eq!(v["recordId"], 1);
    assert_eq!(v["allDay"], false);
    assert_eq!(v["badge"], "new");
    assert!(v.get("location").is_none());
    let back: ScheduleItem = serde_json::from_value(v).unwrap();
    assert_eq!(back, i);
}
