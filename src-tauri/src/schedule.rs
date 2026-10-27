//! 予定データの変換・絞り込み・差分・リマインド判定（I/O を持たない純粋なロジック）

use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// 予定1件（日時はローカル時刻に直したもの）
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleItem {
    pub id: String,
    pub record_id: u64,
    pub subject: String,
    pub activity_type: String,
    #[serde(with = "local_minute")]
    pub start: NaiveDateTime,
    #[serde(with = "local_minute")]
    pub end: NaiveDateTime,
    pub all_day: bool,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default)]
    pub modified_time: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<Badge>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Badge {
    New,
    Changed,
}

/// 画面とキャッシュでは "YYYY-MM-DDTHH:MM" 形式でやりとりする
mod local_minute {
    use chrono::NaiveDateTime;
    use serde::{Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%dT%H:%M";

    pub fn serialize<S: Serializer>(dt: &NaiveDateTime, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&dt.format(FORMAT).to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<NaiveDateTime, D::Error> {
        let s = String::deserialize(d)?;
        NaiveDateTime::parse_from_str(&s, FORMAT).map_err(serde::de::Error::custom)
    }
}

/// 終了時刻が空の予定の長さ（仕様 5.3）
const DEFAULT_DURATION_MINUTES: i64 = 30;

/// `query` の取得範囲（UTC の日付と比べられるため、今日の前後1日に広げる）
pub fn query_range(today: NaiveDate) -> (NaiveDate, NaiveDate) {
    (today - Duration::days(1), today + Duration::days(1))
}

/// 予定を取得する VTQL を組み立てる
pub fn build_query(user_id: &str, from: NaiveDate, to: NaiveDate, offset: usize, select_all: bool) -> String {
    let fields = if select_all {
        "*"
    } else {
        "id, subject, activitytype, date_start, time_start, due_date, time_end, eventstatus, location, visibility, modifiedtime"
    };
    format!(
        "SELECT {fields} FROM Events WHERE assigned_user_id = '{}' AND date_start <= '{}' AND due_date >= '{}' LIMIT {offset}, {};",
        user_id.replace('\'', ""),
        to.format("%Y-%m-%d"),
        from.format("%Y-%m-%d"),
        QUERY_PAGE_SIZE,
    )
}

/// `query` が1回で返す最大件数
pub const QUERY_PAGE_SIZE: usize = 100;

fn str_field<'a>(rec: &'a Value, key: &str) -> &'a str {
    rec.get(key).and_then(Value::as_str).map(str::trim).unwrap_or("")
}

fn parse_time(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(s, "%H:%M"))
        .ok()
}

/// CRM は文字列を HTML エスケープした状態で保存しているので元に戻す
pub fn decode_html(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

fn to_local<Tz: TimeZone>(utc: NaiveDateTime, tz: &Tz) -> NaiveDateTime {
    Utc.from_utc_datetime(&utc).with_timezone(tz).naive_local()
}

/// `query` の1レコード（UTC の日時）を ScheduleItem（ローカル時刻）に変換する。
/// 必須項目（id, date_start）がないか壊れている場合は None。
pub fn parse_record<Tz: TimeZone>(rec: &Value, tz: &Tz) -> Option<ScheduleItem> {
    let id = str_field(rec, "id");
    let record_id: u64 = id.rsplit_once('x')?.1.parse().ok()?;

    let start_date = NaiveDate::parse_from_str(str_field(rec, "date_start"), "%Y-%m-%d").ok()?;
    let start_time = parse_time(str_field(rec, "time_start")).unwrap_or(NaiveTime::MIN);
    let start_utc = start_date.and_time(start_time);

    let end_date = NaiveDate::parse_from_str(str_field(rec, "due_date"), "%Y-%m-%d").unwrap_or(start_date);
    let end_utc = match parse_time(str_field(rec, "time_end")) {
        Some(t) => end_date.and_time(t),
        None => start_utc + Duration::minutes(DEFAULT_DURATION_MINUTES),
    };

    let start = to_local(start_utc, tz);
    let end = to_local(end_utc, tz).max(start);
    let all_day = start.time() == NaiveTime::MIN && end.time() == NaiveTime::MIN;

    let location = decode_html(str_field(rec, "location"));
    Some(ScheduleItem {
        id: id.to_string(),
        record_id,
        subject: decode_html(str_field(rec, "subject")),
        activity_type: decode_html(str_field(rec, "activitytype")),
        start,
        end,
        all_day,
        status: decode_html(str_field(rec, "eventstatus")),
        location: (!location.is_empty()).then_some(location),
        modified_time: str_field(rec, "modifiedtime").to_string(),
        badge: None,
    })
}

/// 予定が指定した日（ローカルの 00:00〜24:00）にかかるか
pub fn overlaps_day(item: &ScheduleItem, day: NaiveDate) -> bool {
    let day_start = day.and_time(NaiveTime::MIN);
    let day_end = day_start + Duration::days(1);
    if item.start == item.end {
        // 長さ0の予定（終日1日分を含む）は開始がその日の中にあるか
        return item.start >= day_start && item.start < day_end;
    }
    item.start < day_end && item.end > day_start
}

pub fn is_held(item: &ScheduleItem) -> bool {
    item.status.trim().eq_ignore_ascii_case("held")
}

pub fn is_not_held(item: &ScheduleItem) -> bool {
    item.status.trim().eq_ignore_ascii_case("not held")
}

/// 表示の絞り込み条件（設定画面の値）
#[derive(Clone, Debug, Default)]
pub struct Filter {
    /// 空ならすべての種別を表示する
    pub activity_types: Vec<String>,
    pub show_held: bool,
    pub show_not_held: bool,
}

impl Filter {
    pub fn passes(&self, item: &ScheduleItem) -> bool {
        if !self.show_held && is_held(item) {
            return false;
        }
        if !self.show_not_held && is_not_held(item) {
            return false;
        }
        self.activity_types.is_empty()
            || self.activity_types.iter().any(|t| t.trim().eq_ignore_ascii_case(item.activity_type.trim()))
    }
}

/// レコード一覧から「今日の表示対象」を作る（変換 → 今日にかかるもの → 絞り込み → 並べ替え）
pub fn build_today<Tz: TimeZone>(records: &[Value], tz: &Tz, today: NaiveDate, filter: &Filter) -> Vec<ScheduleItem> {
    let mut seen = HashSet::new();
    let mut items: Vec<ScheduleItem> = records
        .iter()
        .filter_map(|r| parse_record(r, tz))
        .filter(|i| overlaps_day(i, today) && filter.passes(i))
        // ページングの境目で同じレコードが重複して返る場合に備える
        .filter(|i| seen.insert(i.id.clone()))
        .collect();
    sort_items(&mut items);
    items
}

/// 終日予定を先頭に、その後は開始時刻順
pub fn sort_items(items: &mut [ScheduleItem]) {
    items.sort_by(|a, b| {
        b.all_day
            .cmp(&a.all_day)
            .then(a.start.cmp(&b.start))
            .then(a.end.cmp(&b.end))
            .then(a.subject.cmp(&b.subject))
    });
}

#[derive(Debug, Default, PartialEq)]
pub struct Diff {
    pub added: Vec<ScheduleItem>,
    /// (変更前, 変更後)
    pub changed: Vec<(ScheduleItem, ScheduleItem)>,
    pub removed: Vec<ScheduleItem>,
}

#[cfg(test)]
impl Diff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }
}

/// 前回と今回の一覧を id で比べる。時刻（開始・終了）が変わったものだけを「変更」とする。
pub fn diff(prev: &[ScheduleItem], next: &[ScheduleItem]) -> Diff {
    let prev_map: HashMap<&str, &ScheduleItem> = prev.iter().map(|i| (i.id.as_str(), i)).collect();
    let next_ids: HashSet<&str> = next.iter().map(|i| i.id.as_str()).collect();
    let mut d = Diff::default();
    for item in next {
        match prev_map.get(item.id.as_str()) {
            None => d.added.push(item.clone()),
            Some(old) if old.start != item.start || old.end != item.end => {
                d.changed.push(((*old).clone(), item.clone()))
            }
            Some(_) => {}
        }
    }
    d.removed = prev.iter().filter(|i| !next_ids.contains(i.id.as_str())).cloned().collect();
    d
}

/// NEW / 変更 バッジの管理。値は (バッジ, 付けた時刻)
pub type Badges = HashMap<String, (Badge, NaiveDateTime)>;

/// バッジが自動で消えるまでの時間（仕様 F-01）
pub const BADGE_TTL_MINUTES: i64 = 30;

/// 差分からバッジを付ける（削除されたものはバッジも消す）
pub fn record_badges(badges: &mut Badges, d: &Diff, now: NaiveDateTime) {
    for item in &d.added {
        badges.insert(item.id.clone(), (Badge::New, now));
    }
    for (_, item) in &d.changed {
        // NEW のまま時刻が変わった場合は NEW を残す
        let keep_new = matches!(badges.get(&item.id), Some((Badge::New, _)));
        if !keep_new {
            badges.insert(item.id.clone(), (Badge::Changed, now));
        }
    }
    for item in &d.removed {
        badges.remove(&item.id);
    }
}

/// 期限切れのバッジを消す。1件でも消したら true
pub fn expire_badges(badges: &mut Badges, now: NaiveDateTime) -> bool {
    let before = badges.len();
    badges.retain(|_, (_, at)| now - *at < Duration::minutes(BADGE_TTL_MINUTES));
    badges.len() != before
}

pub fn apply_badges(items: &mut [ScheduleItem], badges: &Badges) {
    for item in items {
        item.badge = badges.get(&item.id).map(|(b, _)| *b);
    }
}

/// リマインドの対象か（終日・完了・中止は対象外）
fn remindable(item: &ScheduleItem) -> bool {
    !item.all_day && !is_held(item) && !is_not_held(item)
}

/// 今リマインドすべき予定を返す。
/// - 開始前で「開始 - N分 ≦ 現在」になったタイミングを通知する
/// - 同じ予定・同じ開始時刻・同じ N の通知は1回だけ（`fired` に記録する）
/// - スリープ明けなどで複数のタイミングをまとめて過ぎていたら、通知は1回にまとめる
///
/// 戻り値は (予定, 開始までの残り分)
pub fn due_reminders(
    items: &[ScheduleItem],
    now: NaiveDateTime,
    offsets: &[u32],
    fired: &mut HashSet<String>,
) -> Vec<(ScheduleItem, i64)> {
    let mut due = Vec::new();
    for item in items.iter().filter(|i| remindable(i) && i.start > now) {
        let reached: Vec<String> = offsets
            .iter()
            .filter(|&&m| now >= item.start - Duration::minutes(m as i64))
            .map(|m| format!("{}|{}|{}", item.id, item.start, m))
            .collect();
        if reached.iter().any(|k| !fired.contains(k)) {
            let secs = (item.start - now).num_seconds();
            due.push((item.clone(), (secs + 59) / 60));
        }
        fired.extend(reached);
    }
    due
}

/// まだ始まっていない予定のうち一番早いもの（終日・完了・中止は除く）
pub fn next_item(items: &[ScheduleItem], now: NaiveDateTime) -> Option<&ScheduleItem> {
    items.iter().filter(|i| remindable(i) && i.start > now).min_by_key(|i| i.start)
}

#[cfg(test)]
#[path = "schedule_tests.rs"]
mod tests;
