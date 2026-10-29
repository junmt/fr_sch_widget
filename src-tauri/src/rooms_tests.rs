//! テスト仕様書 2.12〜2.17（rooms.rs）。関数名の先頭はテストIDに対応する。
use super::*;
use chrono::FixedOffset;
use serde_json::json;

fn dt(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
}

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn item(id: &str, subject: &str, start: &str, end: &str) -> ScheduleItem {
    ScheduleItem {
        id: id.into(),
        record_id: id.rsplit_once('x').map(|(_, n)| n.parse().unwrap()).unwrap_or(0),
        subject: subject.into(),
        activity_type: "Meeting".into(),
        start: dt(start),
        end: dt(end),
        all_day: false,
        status: "Planned".into(),
        location: None,
        modified_time: String::new(),
        badge: None,
        rooms: Vec::new(),
    }
}

fn with_status(mut i: ScheduleItem, s: &str) -> ScheduleItem {
    i.status = s.into();
    i
}

fn all_day(mut i: ScheduleItem) -> ScheduleItem {
    i.all_day = true;
    i
}

/// 自分の予定 A（10:00-11:00「定例」）
fn mine() -> ScheduleItem {
    item("18x1", "定例", "2026-10-27 10:00", "2026-10-27 11:00")
}

/// 会議室のユーザーに複製された A（id は違う）
fn copy_of_mine() -> ScheduleItem {
    item("18x901", "定例", "2026-10-27 10:00", "2026-10-27 11:00")
}

/// 同じ会議室の、他の人の予約
fn other(start: &str, end: &str) -> ScheduleItem {
    item("18x902", "他部署MTG", start, end)
}

fn annotate_one(item: ScheduleItem, own: &[ScheduleItem], bookings: Vec<ScheduleItem>) -> Vec<RoomUse> {
    let mut items = vec![item];
    annotate_rooms(&mut items, own, &[("会議室A".into(), bookings)]);
    items.remove(0).rooms
}

fn next_of(start: &str, end: &str) -> Option<RoomNext> {
    Some(RoomNext { subject: "他部署MTG".into(), start: dt(start), end: dt(end) })
}

fn user(id: &str, login: &str, first: &str, last: &str) -> Value {
    json!({"id": id, "user_name": login, "first_name": first, "last_name": last})
}

fn spec(name: &str) -> RoomSpec {
    RoomSpec { name: name.into(), id: None }
}

// ===== 2.12 parse_room_spec / name_key =====

#[test]
fn rs01_name_only() {
    assert_eq!(parse_room_spec("会議室A"), RoomSpec { name: "会議室A".into(), id: None });
}
#[test]
fn rs02_name_with_id() {
    assert_eq!(parse_room_spec("会議室B=19x12"), RoomSpec { name: "会議室B".into(), id: Some("19x12".into()) });
}
#[test]
fn rs03_spaces_trimmed() {
    assert_eq!(parse_room_spec("  会議室B = 19x12  "), RoomSpec { name: "会議室B".into(), id: Some("19x12".into()) });
    assert_eq!(parse_room_spec("  会議室A  "), spec("会議室A"));
}
#[test]
fn rs04_empty_name_is_not_id() {
    assert_eq!(parse_room_spec("=19x12"), spec("=19x12"));
}
#[test]
fn rs05_not_an_id() {
    for s in ["A=abc", "A=19x", "A=x12", "A=1 9x12", "A=19y12", "A=", "A=19x12x3"] {
        assert_eq!(parse_room_spec(s), spec(s), "{s}");
    }
}
#[test]
fn rs06_last_equals_sign_splits() {
    assert_eq!(parse_room_spec("A=B=19x12"), RoomSpec { name: "A=B".into(), id: Some("19x12".into()) });
}
#[test]
fn nk01_spaces_removed() {
    assert_eq!(name_key(" 会議室　A "), "会議室a");
    assert_eq!(name_key("Room A"), "rooma");
}
#[test]
fn nk02_case_insensitive() {
    assert_eq!(name_key("ROOM_a"), name_key("room_A"));
}

// ===== 2.13 resolve_rooms / build_users_query =====

#[test]
fn rr01_id_written_used_as_is() {
    // 同じ名前のユーザーがいても、書いた ID を使う
    let users = [user("19x7", "会議室B", "", "")];
    let (found, missing) = resolve_rooms(&[parse_room_spec("会議室B=19x12")], &users);
    assert_eq!(found, vec![Room { name: "会議室B".into(), id: "19x12".into() }]);
    assert!(missing.is_empty());
}
#[test]
fn rr02_login_name_match() {
    let users = [user("19x3", "taro", "太郎", "山田"), user("19x7", "Room_A", "", "")];
    let (found, missing) = resolve_rooms(&[spec("room_a")], &users);
    assert_eq!(found, vec![Room { name: "room_a".into(), id: "19x7".into() }]);
    assert!(missing.is_empty());
}
#[test]
fn rr03_last_first_match() {
    let users = [user("19x8", "room8", "A", "会議室")];
    let (found, _) = resolve_rooms(&[spec("会議室 A")], &users);
    assert_eq!(found, vec![Room { name: "会議室 A".into(), id: "19x8".into() }]);
}
#[test]
fn rr04_first_last_match() {
    let users = [user("19x9", "room9", "Room", "B")];
    let (found, _) = resolve_rooms(&[spec("Room B")], &users);
    assert_eq!(found, vec![Room { name: "Room B".into(), id: "19x9".into() }]);
}
#[test]
fn rr05_no_match_is_missing() {
    let users = [user("19x3", "taro", "太郎", "山田")];
    let (found, missing) = resolve_rooms(&[spec("会議室Z")], &users);
    assert!(found.is_empty());
    assert_eq!(missing, vec!["会議室Z".to_string()]);
}
#[test]
fn rr06_no_users_and_no_id_is_missing() {
    let (found, missing) = resolve_rooms(&[spec("会議室A")], &[]);
    assert!(found.is_empty());
    assert_eq!(missing, vec!["会議室A".to_string()]);
}
#[test]
fn rr07_mixed_keeps_order() {
    let users = [user("19x7", "会議室A", "", "")];
    let specs = [spec("会議室X"), spec("会議室A"), parse_room_spec("会議室B=19x12"), spec("会議室Y")];
    let (found, missing) = resolve_rooms(&specs, &users);
    assert_eq!(
        found,
        vec![Room { name: "会議室A".into(), id: "19x7".into() }, Room { name: "会議室B".into(), id: "19x12".into() }]
    );
    assert_eq!(missing, vec!["会議室X".to_string(), "会議室Y".to_string()]);
}
#[test]
fn rr08_user_without_id_is_missing() {
    let users = [json!({"user_name": "会議室A"})];
    let (found, missing) = resolve_rooms(&[spec("会議室A")], &users);
    assert!(found.is_empty());
    assert_eq!(missing, vec!["会議室A".to_string()]);
}
#[test]
fn rr09_first_matching_user_wins() {
    let users = [user("19x7", "会議室A", "", ""), user("19x8", "x", "", "会議室A")];
    let (found, _) = resolve_rooms(&[spec("会議室A")], &users);
    assert_eq!(found[0].id, "19x7");
}
#[test]
fn rr10_empty_user_fields_do_not_match_partially() {
    // 姓だけ・名だけのユーザーは、その名前とだけ一致する（空の名前とは一致しない）
    let users = [user("19x7", "", "", "会議室")];
    let (found, missing) = resolve_rooms(&[spec("会議室A")], &users);
    assert!(found.is_empty());
    assert_eq!(missing.len(), 1);
}
#[test]
fn bu01_users_query() {
    assert_eq!(build_users_query(0), "SELECT id, user_name, first_name, last_name FROM Users LIMIT 0, 100;");
    assert_eq!(build_users_query(200), "SELECT id, user_name, first_name, last_name FROM Users LIMIT 200, 100;");
}

// ===== 2.14 annotate_rooms =====

#[test]
fn ar01_all_day_item_has_no_rooms() {
    let a = all_day(item("18x1", "研修", "2026-10-27 00:00", "2026-10-28 00:00"));
    let b = all_day(item("18x901", "研修", "2026-10-27 00:00", "2026-10-28 00:00"));
    assert!(annotate_one(a.clone(), &[a], vec![b]).is_empty());
}
#[test]
fn ar02_no_matching_booking() {
    assert!(annotate_one(mine(), &[mine()], vec![other("2026-10-27 11:00", "2026-10-27 12:00")]).is_empty());
}
#[test]
fn ar03_subject_differs() {
    let b = item("18x901", "定例（別）", "2026-10-27 10:00", "2026-10-27 11:00");
    assert!(annotate_one(mine(), &[mine()], vec![b]).is_empty());
}
#[test]
fn ar04_end_differs() {
    let b = item("18x901", "定例", "2026-10-27 10:00", "2026-10-27 11:30");
    assert!(annotate_one(mine(), &[mine()], vec![b]).is_empty());
}
#[test]
fn ar05_start_differs() {
    let b = item("18x901", "定例", "2026-10-27 09:30", "2026-10-27 11:00");
    assert!(annotate_one(mine(), &[mine()], vec![b]).is_empty());
}
#[test]
fn ar06_match_without_next() {
    let rooms = annotate_one(mine(), &[mine()], vec![copy_of_mine()]);
    assert_eq!(rooms, vec![RoomUse { name: "会議室A".into(), next: None }]);
}
#[test]
fn ar07_next_starts_exactly_at_end() {
    let rooms = annotate_one(mine(), &[mine()], vec![copy_of_mine(), other("2026-10-27 11:00", "2026-10-27 12:00")]);
    assert_eq!(rooms, vec![RoomUse { name: "会議室A".into(), next: next_of("2026-10-27 11:00", "2026-10-27 12:00") }]);
}
#[test]
fn ar08_next_starts_one_minute_after_end() {
    let rooms = annotate_one(mine(), &[mine()], vec![copy_of_mine(), other("2026-10-27 11:01", "2026-10-27 12:00")]);
    assert_eq!(rooms[0].next, None);
}
#[test]
fn ar09_next_overlaps_meeting() {
    let rooms = annotate_one(mine(), &[mine()], vec![copy_of_mine(), other("2026-10-27 10:59", "2026-10-27 12:00")]);
    assert_eq!(rooms[0].next, next_of("2026-10-27 10:59", "2026-10-27 12:00"));
}
#[test]
fn ar10_booking_starting_with_meeting_is_not_next() {
    // 開始が同じ（開始より後ではない）予約は「次」ではない。前から続く予約も同じ
    let bookings = vec![
        copy_of_mine(),
        other("2026-10-27 10:00", "2026-10-27 10:30"),
        item("18x903", "朝の予約", "2026-10-27 09:00", "2026-10-27 10:30"),
    ];
    assert_eq!(annotate_one(mine(), &[mine()], bookings)[0].next, None);
}
#[test]
fn ar11_own_next_meeting_is_not_next() {
    // 同じ会議室で続けて自分の予定がある（その複製）は知らせない
    let my_next = item("18x2", "レビュー", "2026-10-27 11:00", "2026-10-27 12:00");
    let copy = item("18x904", "レビュー", "2026-10-27 11:00", "2026-10-27 12:00");
    let rooms = annotate_one(mine(), &[mine(), my_next], vec![copy_of_mine(), copy]);
    assert_eq!(rooms, vec![RoomUse { name: "会議室A".into(), next: None }]);
}
#[test]
fn ar12_not_held_booking_is_ignored() {
    let b = with_status(other("2026-10-27 11:00", "2026-10-27 12:00"), "Not Held");
    assert_eq!(annotate_one(mine(), &[mine()], vec![copy_of_mine(), b])[0].next, None);
}
#[test]
fn ar13_all_day_booking_is_ignored() {
    let b = all_day(other("2026-10-27 00:00", "2026-10-28 00:00"));
    let b2 = all_day(other("2026-10-27 11:00", "2026-10-27 11:00"));
    assert_eq!(annotate_one(mine(), &[mine()], vec![copy_of_mine(), b, b2])[0].next, None);
}
#[test]
fn ar14_held_booking_counts() {
    // 完了（Held）は予約として残っているので数える
    let b = with_status(other("2026-10-27 11:00", "2026-10-27 12:00"), "Held");
    assert!(annotate_one(mine(), &[mine()], vec![copy_of_mine(), b])[0].next.is_some());
}
#[test]
fn ar15_earliest_next_wins() {
    let later = item("18x905", "後の予約", "2026-10-27 11:00", "2026-10-27 11:30");
    let earlier = other("2026-10-27 10:45", "2026-10-27 11:30");
    let rooms = annotate_one(mine(), &[mine()], vec![later, copy_of_mine(), earlier]);
    assert_eq!(rooms[0].next, next_of("2026-10-27 10:45", "2026-10-27 11:30"));
}
#[test]
fn ar16_multiple_rooms() {
    let mut items = vec![mine(), item("18x2", "面談", "2026-10-27 14:00", "2026-10-27 15:00")];
    let own = items.clone();
    let bookings = vec![
        ("会議室A".to_string(), vec![copy_of_mine(), other("2026-10-27 11:00", "2026-10-27 12:00")]),
        ("会議室B".to_string(), vec![copy_of_mine(), item("18x906", "面談", "2026-10-27 14:00", "2026-10-27 15:00")]),
        ("会議室C".to_string(), vec![]),
    ];
    annotate_rooms(&mut items, &own, &bookings);
    assert_eq!(
        items[0].rooms,
        vec![
            RoomUse { name: "会議室A".into(), next: next_of("2026-10-27 11:00", "2026-10-27 12:00") },
            RoomUse { name: "会議室B".into(), next: None },
        ]
    );
    assert_eq!(items[1].rooms, vec![RoomUse { name: "会議室B".into(), next: None }]);
}
#[test]
fn ar17_idempotent_and_clears_old_rooms() {
    let mut items = vec![mine()];
    let bookings = vec![("会議室A".to_string(), vec![copy_of_mine(), other("2026-10-27 11:00", "2026-10-27 12:00")])];
    annotate_rooms(&mut items, &[mine()], &bookings);
    let first = items.clone();
    annotate_rooms(&mut items, &[mine()], &bookings);
    assert_eq!(items, first);
    // 予約がなくなったら会議室の情報も消える
    annotate_rooms(&mut items, &[mine()], &[]);
    assert!(items[0].rooms.is_empty());
}
#[test]
fn ar18_own_but_hidden_meeting_is_not_next() {
    // 絞り込みで一覧にない自分の予定（own にはある）も「次の予約」に数えない
    let hidden = with_status(item("18x2", "レビュー", "2026-10-27 11:00", "2026-10-27 12:00"), "Held");
    let copy = item("18x904", "レビュー", "2026-10-27 11:00", "2026-10-27 12:00");
    let mut items = vec![mine()];
    annotate_rooms(&mut items, &[mine(), hidden], &[("会議室A".into(), vec![copy_of_mine(), copy])]);
    assert_eq!(items[0].rooms[0].next, None);
}
#[test]
fn ar19_zero_length_item_has_no_next() {
    let a = item("18x1", "電話", "2026-10-27 10:00", "2026-10-27 10:00");
    let b = item("18x901", "電話", "2026-10-27 10:00", "2026-10-27 10:00");
    let rooms = annotate_one(a.clone(), &[a], vec![b, other("2026-10-27 10:00", "2026-10-27 11:00")]);
    assert_eq!(rooms, vec![RoomUse { name: "会議室A".into(), next: None }]);
}

// ===== 2.15 build_day =====

fn jst() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).unwrap()
}

/// UTC の値でレコードを作る（query の返り値と同じ形）
fn rec(id: &str, subject: &str, ds: &str, ts: &str, te: &str, status: &str) -> Value {
    json!({
        "id": id, "subject": subject, "activitytype": "Meeting",
        "date_start": ds, "time_start": ts, "due_date": ds, "time_end": te,
        "eventstatus": status, "location": "", "modifiedtime": ""
    })
}

#[test]
fn bd01_without_rooms_same_as_build_today() {
    let records = vec![rec("18x1", "定例", "2026-10-27", "01:00:00", "02:00:00", "Planned")];
    let f = Filter { show_held: true, show_not_held: true, ..Default::default() };
    let items = build_day(&records, &[], CrmTimeZone::Utc, &jst(), d("2026-10-27"), &f);
    assert_eq!(items, build_today(&records, CrmTimeZone::Utc, &jst(), d("2026-10-27"), &f));
    assert!(items[0].rooms.is_empty());
}
#[test]
fn bd02_rooms_converted_with_same_time_zone() {
    // UTC 01:00-02:00 = JST 10:00-11:00。会議室の予約も同じく変換して比べる
    let records = vec![rec("18x1", "定例", "2026-10-27", "01:00:00", "02:00:00", "Planned")];
    let rooms = vec![(
        "会議室A".to_string(),
        vec![
            rec("18x901", "定例", "2026-10-27", "01:00:00", "02:00:00", "Planned"),
            rec("18x902", "他部署MTG", "2026-10-27", "02:00:00", "03:00:00", "Planned"),
        ],
    )];
    let f = Filter { show_held: true, show_not_held: true, ..Default::default() };
    let items = build_day(&records, &rooms, CrmTimeZone::Utc, &jst(), d("2026-10-27"), &f);
    assert_eq!(
        items[0].rooms,
        vec![RoomUse { name: "会議室A".into(), next: next_of("2026-10-27 11:00", "2026-10-27 12:00") }]
    );
}
#[test]
fn bd03_filtered_own_event_still_excluded_from_next() {
    // 完了を表示しない設定でも、自分の完了済みの予定は「次の予約」に数えない
    let records = vec![
        rec("18x1", "定例", "2026-10-27", "01:00:00", "02:00:00", "Planned"),
        rec("18x2", "レビュー", "2026-10-27", "02:00:00", "03:00:00", "Held"),
    ];
    let rooms = vec![(
        "会議室A".to_string(),
        vec![
            rec("18x901", "定例", "2026-10-27", "01:00:00", "02:00:00", "Planned"),
            rec("18x904", "レビュー", "2026-10-27", "02:00:00", "03:00:00", "Planned"),
        ],
    )];
    let f = Filter { show_held: false, show_not_held: true, ..Default::default() };
    let items = build_day(&records, &rooms, CrmTimeZone::Utc, &jst(), d("2026-10-27"), &f);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].rooms, vec![RoomUse { name: "会議室A".into(), next: None }]);
}
#[test]
fn bd04_broken_room_records_ignored() {
    let records = vec![rec("18x1", "定例", "2026-10-27", "01:00:00", "02:00:00", "Planned")];
    let rooms = vec![("会議室A".to_string(), vec![json!({"id": "bad"}), json!(null)])];
    let f = Filter { show_held: true, show_not_held: true, ..Default::default() };
    let items = build_day(&records, &rooms, CrmTimeZone::Utc, &jst(), d("2026-10-27"), &f);
    assert!(items[0].rooms.is_empty());
}

// ===== 2.16 due_room_alerts =====

fn alerted() -> ScheduleItem {
    let mut i = mine();
    i.rooms = vec![RoomUse { name: "会議室A".into(), next: next_of("2026-10-27 11:00", "2026-10-27 12:00") }];
    i
}

fn alert_ids(items: &[ScheduleItem], now: &str, fired: &mut HashSet<String>) -> Vec<(String, i64)> {
    due_room_alerts(items, dt(now), fired).into_iter().map(|(i, m)| (i.id, m)).collect()
}

#[test]
fn ra01_five_minutes_before_end() {
    assert_eq!(alert_ids(&[alerted()], "2026-10-27 10:55", &mut HashSet::new()), vec![("18x1".into(), 5)]);
}
#[test]
fn ra02_six_minutes_before_end_is_too_early() {
    assert!(alert_ids(&[alerted()], "2026-10-27 10:54", &mut HashSet::new()).is_empty());
}
#[test]
fn ra03_one_minute_before_end() {
    assert_eq!(alert_ids(&[alerted()], "2026-10-27 10:59", &mut HashSet::new()), vec![("18x1".into(), 1)]);
}
#[test]
fn ra04_at_end_is_too_late() {
    assert!(alert_ids(&[alerted()], "2026-10-27 11:00", &mut HashSet::new()).is_empty());
}
#[test]
fn ra05_after_end_is_too_late() {
    // スリープ明けで終了を過ぎていたら出さない
    assert!(alert_ids(&[alerted()], "2026-10-27 11:30", &mut HashSet::new()).is_empty());
}
#[test]
fn ra06_room_without_next() {
    let mut i = mine();
    i.rooms = vec![RoomUse { name: "会議室A".into(), next: None }];
    assert!(alert_ids(&[i], "2026-10-27 10:56", &mut HashSet::new()).is_empty());
}
#[test]
fn ra07_no_rooms() {
    assert!(alert_ids(&[mine()], "2026-10-27 10:56", &mut HashSet::new()).is_empty());
}
#[test]
fn ra08_all_day_not_alerted() {
    let i = all_day(alerted());
    assert!(alert_ids(&[i], "2026-10-27 10:56", &mut HashSet::new()).is_empty());
}
#[test]
fn ra09_held_and_not_held_not_alerted() {
    for s in ["Held", "Not Held"] {
        let i = with_status(alerted(), s);
        assert!(alert_ids(&[i], "2026-10-27 10:56", &mut HashSet::new()).is_empty(), "{s}");
    }
}
#[test]
fn ra10_only_once() {
    let mut fired = HashSet::new();
    assert_eq!(alert_ids(&[alerted()], "2026-10-27 10:55", &mut fired).len(), 1);
    assert!(alert_ids(&[alerted()], "2026-10-27 10:56", &mut fired).is_empty());
    assert!(alert_ids(&[alerted()], "2026-10-27 10:59", &mut fired).is_empty());
}
#[test]
fn ra11_new_end_time_alerts_again() {
    let mut fired = HashSet::new();
    assert_eq!(alert_ids(&[alerted()], "2026-10-27 10:55", &mut fired).len(), 1);
    let mut moved = alerted();
    moved.end = dt("2026-10-27 11:30");
    assert_eq!(alert_ids(&[moved], "2026-10-27 11:26", &mut fired), vec![("18x1".into(), 4)]);
}
#[test]
fn ra12_remaining_minutes_rounded_up() {
    let now = dt("2026-10-27 10:55") + Duration::seconds(30);
    let r = due_room_alerts(&[alerted()], now, &mut HashSet::new());
    assert_eq!(r[0].1, 5);
}
#[test]
fn ra13_reminder_keys_do_not_suppress() {
    // 開始前のリマインドの記録（id|開始|分）とは別に数える
    let mut fired: HashSet<String> = ["18x1|2026-10-27 10:00:00|10".to_string(), "18x1|2026-10-27 11:00:00|5".to_string()].into();
    assert_eq!(alert_ids(&[alerted()], "2026-10-27 10:55", &mut fired).len(), 1);
}
#[test]
fn ra14_multiple_items() {
    let mut b = alerted();
    b.id = "18x2".into();
    b.start = dt("2026-10-27 10:30");
    let c = mine();
    let r = alert_ids(&[alerted(), b, c], "2026-10-27 10:57", &mut HashSet::new());
    assert_eq!(r, vec![("18x1".into(), 3), ("18x2".into(), 3)]);
}

// ===== 2.17 connection_room_text =====

#[test]
fn cr01_no_rooms() {
    assert_eq!(connection_room_text(0, &[]), "");
}
#[test]
fn cr02_all_found() {
    assert_eq!(connection_room_text(2, &[]), "。会議室 2件を確認しました");
}
#[test]
fn cr03_some_missing() {
    assert_eq!(connection_room_text(1, &["X".into(), "Y".into()]), "。見つからない会議室: X, Y");
}
#[test]
fn cr04_all_missing() {
    assert_eq!(connection_room_text(0, &["X".into()]), "。見つからない会議室: X");
}

// ===== 2.18 ScheduleItem の rooms のシリアライズ =====

#[test]
fn sj01_rooms_serialized() {
    let v = serde_json::to_value(alerted()).unwrap();
    assert_eq!(
        v["rooms"],
        json!([{"name": "会議室A", "next": {"subject": "他部署MTG", "start": "2026-10-27T11:00", "end": "2026-10-27T12:00"}}])
    );
    let back: ScheduleItem = serde_json::from_value(v).unwrap();
    assert_eq!(back, alerted());
}
#[test]
fn sj02_empty_rooms_omitted_and_old_json_reads() {
    let v = serde_json::to_value(mine()).unwrap();
    assert!(v.get("rooms").is_none());
    // 古い版が保存したキャッシュ（rooms がない）も読める
    let back: ScheduleItem = serde_json::from_value(v).unwrap();
    assert!(back.rooms.is_empty());
}
#[test]
fn sj03_room_without_next_omits_next() {
    let mut i = mine();
    i.rooms = vec![RoomUse { name: "会議室A".into(), next: None }];
    assert_eq!(serde_json::to_value(i).unwrap()["rooms"], json!([{"name": "会議室A"}]));
}
