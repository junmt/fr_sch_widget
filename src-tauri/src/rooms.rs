//! 会議室（仕様 F-14）。会議室は CRM のユーザーとして作られ、予定の参加者として招待される前提。
//! 招待すると、会議室のユーザーが担当の予定（同じ件名・時刻の複製）ができるので、
//! それを自分の予定と突き合わせて「どの会議室を使うか」「終了直後に次の予約があるか」を判定する。
//! I/O を持たない純粋なロジック。

use crate::schedule::{
    build_today, is_not_held, local_minute, parse_record, remindable, CrmTimeZone, Filter, ScheduleItem,
    QUERY_PAGE_SIZE,
};
use chrono::{Duration, NaiveDate, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

/// 予定が使う会議室と、その会議室の終了直後の予約
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomUse {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<RoomNext>,
}

/// 同じ会議室の、自分の予定の終了時刻までに始まる別の予約
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomNext {
    pub subject: String,
    #[serde(with = "local_minute")]
    pub start: NaiveDateTime,
    #[serde(with = "local_minute")]
    pub end: NaiveDateTime,
}

/// 終了の何分前に「次の予約がある」と知らせるか
pub const ROOM_ALERT_MINUTES: i64 = 5;

/// 設定の1行（`会議室A` か `会議室A=19x12`）
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomSpec {
    pub name: String,
    /// Webservice の ID を直接書いたとき（Users を読む権限がない環境向け）
    pub id: Option<String>,
}

/// CRM のユーザーを見つけた会議室
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Room {
    pub name: String,
    pub id: String,
}

fn is_ws_id(s: &str) -> bool {
    matches!(s.split_once('x'), Some((a, b))
        if !a.is_empty() && !b.is_empty() && a.bytes().all(|c| c.is_ascii_digit()) && b.bytes().all(|c| c.is_ascii_digit()))
}

/// `会議室B=19x12` → 名前と ID。`=` の右が ID の形でなければ、全体を名前とみなす
pub fn parse_room_spec(entry: &str) -> RoomSpec {
    let entry = entry.trim();
    if let Some((name, id)) = entry.rsplit_once('=') {
        let (name, id) = (name.trim(), id.trim());
        if !name.is_empty() && is_ws_id(id) {
            return RoomSpec { name: name.to_string(), id: Some(id.to_string()) };
        }
    }
    RoomSpec { name: entry.to_string(), id: None }
}

/// 名前の比べ方：空白（全角を含む）を除き、英字の大文字・小文字を区別しない
pub fn name_key(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase()
}

/// 会議室を探すためにユーザーの一覧を取る VTQL
pub fn build_users_query(offset: usize) -> String {
    format!("SELECT id, user_name, first_name, last_name FROM Users LIMIT {offset}, {QUERY_PAGE_SIZE};")
}

fn user_matches(user: &Value, key: &str) -> bool {
    let f = |k: &str| user.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let (login, first, last) = (f("user_name"), f("first_name"), f("last_name"));
    [login, format!("{last}{first}"), format!("{first}{last}")]
        .iter()
        .any(|c| !c.is_empty() && name_key(c) == key)
}

/// 設定の会議室を CRM のユーザーに結びつける。
/// ID を書いたものはそのまま、名前だけのものはユーザー名（ログイン名）か氏名（姓名・名姓）で探す。
/// 戻り値は (見つかった会議室, 見つからなかった名前)
pub fn resolve_rooms(specs: &[RoomSpec], users: &[Value]) -> (Vec<Room>, Vec<String>) {
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for spec in specs {
        let id = spec.id.clone().or_else(|| {
            let key = name_key(&spec.name);
            users
                .iter()
                .find(|u| user_matches(u, &key))
                .and_then(|u| u.get("id").and_then(Value::as_str))
                .map(str::to_string)
        });
        match id {
            Some(id) => found.push(Room { name: spec.name.clone(), id }),
            None => missing.push(spec.name.clone()),
        }
    }
    (found, missing)
}

/// 同じ予定（の複製）か：開始・終了・件名が同じ
fn same_event(a: &ScheduleItem, b: &ScheduleItem) -> bool {
    a.start == b.start && a.end == b.end && a.subject == b.subject
}

/// 一覧の予定に、使う会議室と終了直後の予約を付ける。
/// - `own` は自分の予定すべて（絞り込む前のもの）。自分の別の予定と同じ予約は「次の予約」に数えない
/// - `bookings` は会議室ごとの予約（会議室のユーザーが担当の予定）
/// - 次の予約：終日・中止を除き、自分の予定の開始より後〜終了時刻ちょうどまでに始まるもののうち一番早いもの
pub fn annotate_rooms(items: &mut [ScheduleItem], own: &[ScheduleItem], bookings: &[(String, Vec<ScheduleItem>)]) {
    let is_own = |b: &ScheduleItem| own.iter().any(|o| same_event(o, b));
    for item in items.iter_mut() {
        item.rooms.clear();
        // 終日の予定は終日の予約としか同じにならず、終日の予約は除くので、会議室は付かない
        for (name, list) in bookings {
            if !list.iter().any(|b| !b.all_day && same_event(item, b)) {
                continue;
            }
            let next = list
                .iter()
                .filter(|b| !b.all_day && !is_not_held(b) && !is_own(b))
                .filter(|b| b.start > item.start && b.start <= item.end)
                .min_by_key(|b| b.start)
                .map(|b| RoomNext { subject: b.subject.clone(), start: b.start, end: b.end });
            item.rooms.push(RoomUse { name: name.clone(), next });
        }
    }
}

/// レコードを変換する（日付での絞り込みはしない）
fn parse_all<Tz: TimeZone>(records: &[Value], source: CrmTimeZone, tz: &Tz) -> Vec<ScheduleItem> {
    records.iter().filter_map(|r| parse_record(r, source, tz)).collect()
}

/// その日の一覧を作り（`build_today`）、会議室の情報を付ける
pub fn build_day<Tz: TimeZone>(
    records: &[Value],
    rooms: &[(String, Vec<Value>)],
    source: CrmTimeZone,
    tz: &Tz,
    day: NaiveDate,
    filter: &Filter,
) -> Vec<ScheduleItem> {
    let mut items = build_today(records, source, tz, day, filter);
    if !rooms.is_empty() {
        let own = parse_all(records, source, tz);
        let bookings: Vec<(String, Vec<ScheduleItem>)> =
            rooms.iter().map(|(name, recs)| (name.clone(), parse_all(recs, source, tz))).collect();
        annotate_rooms(&mut items, &own, &bookings);
    }
    items
}

/// 終了直後に会議室の予約がある予定のうち、今知らせるべきもの。
/// - 終了の5分前〜終了の間に1回だけ（`fired` に記録する。時刻が変わったら新しい終了時刻でもう一度）
/// - 終日・完了・中止の予定は知らせない。終了を過ぎていたら（スリープ明けなど）知らせない
///
/// 戻り値は (予定, 終了までの残り分)
pub fn due_room_alerts(items: &[ScheduleItem], now: NaiveDateTime, fired: &mut HashSet<String>) -> Vec<(ScheduleItem, i64)> {
    items
        .iter()
        .filter(|i| remindable(i) && i.rooms.iter().any(|r| r.next.is_some()))
        .filter(|i| now < i.end && now >= i.end - Duration::minutes(ROOM_ALERT_MINUTES))
        .filter(|i| fired.insert(format!("room|{}|{}", i.id, i.end)))
        .map(|i| (i.clone(), ((i.end - now).num_seconds() + 59) / 60))
        .collect()
}

/// 接続テストの結果に付ける、会議室の確認結果
pub fn connection_room_text(found: usize, missing: &[String]) -> String {
    match (found, missing.is_empty()) {
        (0, true) => String::new(),
        (n, true) => format!("。会議室 {n}件を確認しました"),
        (_, false) => format!("。見つからない会議室: {}", missing.join(", ")),
    }
}

#[cfg(test)]
#[path = "rooms_tests.rs"]
mod tests;
