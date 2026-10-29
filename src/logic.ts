// 画面の表示ロジック（DOM に依存しない）

export type Badge = "new" | "changed";

/** 同じ会議室の、終了時刻までに始まる別の予約（仕様 F-14） */
export interface RoomNext {
  subject: string;
  start: string;
  end: string;
}

/** 予定が使う会議室 */
export interface RoomUse {
  name: string;
  next?: RoomNext;
}

export interface Item {
  id: string;
  recordId: number;
  subject: string;
  activityType: string;
  /** ローカル時刻 "YYYY-MM-DDTHH:MM" */
  start: string;
  end: string;
  allDay: boolean;
  status: string;
  location?: string;
  badge?: Badge;
  rooms?: RoomUse[];
}

export type SyncState = "unconfigured" | "ok" | "error" | "auth_error";

export interface Status {
  state: SyncState;
  message: string | null;
  lastUpdated: string | null;
  fetching: boolean;
}

export interface View {
  items: Item[];
  status: Status;
  /** "YYYY-MM-DD" */
  today: string;
  display: { soonMinutes: number; opacity: number; theme: "system" | "light" | "dark" };
  /** 新しいバージョンがあれば、その版の番号 */
  updateVersion: string | null;
}

/** 予定の状態（仕様 F-01） */
export type Phase = "allday" | "done" | "ongoing" | "soon" | "upcoming";

/** 最終更新からこれ以上たったら、データが古いと警告する（分） */
export const STALE_MINUTES = 30;

/** "YYYY-MM-DDTHH:MM" をローカル時刻として読む */
export function parseLocal(s: string): Date {
  const [d, t = "00:00"] = s.split("T");
  const [y, mo, da] = d.split("-").map(Number);
  const [h, mi] = t.split(":").map(Number);
  return new Date(y, mo - 1, da, h, mi);
}

export function isHeld(item: Item): boolean {
  return item.status.trim().toLowerCase() === "held";
}

export function isNotHeld(item: Item): boolean {
  return item.status.trim().toLowerCase() === "not held";
}

export function phaseOf(item: Item, now: Date, soonMinutes: number): Phase {
  if (item.allDay) return "allday";
  const start = parseLocal(item.start).getTime();
  const end = parseLocal(item.end).getTime();
  const t = now.getTime();
  // 長さ0の予定（開始 = 終了）は開始を過ぎたら終了扱い
  if (end <= t) return "done";
  if (start <= t) return "ongoing";
  if (start - t <= soonMinutes * 60_000) return "soon";
  return "upcoming";
}

/** まだ始まっていない予定のうち一番早いもの（終日・完了・中止を除く） */
export function nextItem(items: Item[], now: Date): Item | undefined {
  let best: Item | undefined;
  for (const i of items) {
    if (i.allDay || isHeld(i) || isNotHeld(i)) continue;
    const s = parseLocal(i.start).getTime();
    if (s <= now.getTime()) continue;
    if (!best || s < parseLocal(best.start).getTime()) best = i;
  }
  return best;
}

/** 開始までの残り分（切り上げ） */
export function minutesUntil(item: Item, now: Date): number {
  return Math.ceil((parseLocal(item.start).getTime() - now.getTime()) / 60_000);
}

export function fmtRemaining(minutes: number): string {
  const m = Math.max(0, minutes);
  const h = Math.floor(m / 60);
  const mm = m % 60;
  if (h === 0) return `${mm}分`;
  return mm === 0 ? `${h}時間` : `${h}時間${mm}分`;
}

const pad = (n: number) => String(n).padStart(2, "0");

export function fmtHm(d: Date): string {
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function fmtMd(d: Date): string {
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

function sameDay(a: Date, b: Date): boolean {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

/** 一覧に出す時刻。今日以外の日時には日付を付ける */
export function fmtRange(item: Item, today: Date): string {
  const s = parseLocal(item.start);
  const e = parseLocal(item.end);
  if (item.allDay) {
    // 終了は「最終日の翌日 00:00」
    const last = new Date(e.getFullYear(), e.getMonth(), e.getDate() - 1);
    return last > s ? `終日 ${fmtMd(s)}〜${fmtMd(last)}` : "終日";
  }
  const part = (d: Date) => (sameDay(d, today) ? fmtHm(d) : `${fmtMd(d)} ${fmtHm(d)}`);
  return `${part(s)}-${part(e)}`;
}

const WEEKDAYS = ["日", "月", "火", "水", "木", "金", "土"];

export function fmtDateHeader(d: Date): string {
  return `${fmtMd(d)}(${WEEKDAYS[d.getDay()]})`;
}

export function isStale(status: Status, now: Date): boolean {
  if (!status.lastUpdated) return false;
  return now.getTime() - new Date(status.lastUpdated).getTime() >= STALE_MINUTES * 60_000;
}

/** フッターの文言 */
export function footerText(status: Status): string {
  const last = status.lastUpdated ? fmtHm(new Date(status.lastUpdated)) : null;
  if (status.fetching) return "更新中…";
  switch (status.state) {
    case "unconfigured":
      return "接続設定をしてください";
    case "auth_error":
      return `⚠ ${status.message ?? "ログインできません"}`;
    case "error":
      return `⚠ ${status.message ?? "接続できません"}${last ? `（最終更新 ${last}）` : ""}`;
    default:
      return last ? `更新 ${last}` : "";
  }
}

/** ヘッダー2行目（次の予定までの残り時間） */
export function nextText(items: Item[], now: Date): string {
  const n = nextItem(items, now);
  if (!n) return "これ以降の予定はありません";
  return `次の予定まで あと ${fmtRemaining(minutesUntil(n, now))}`;
}

/** 折りたたみ表示の1行 */
export function compactText(items: Item[], now: Date): string {
  const n = nextItem(items, now);
  if (!n) return "これ以降の予定はありません";
  return `次 ${fmtHm(parseLocal(n.start))} ${n.subject || "（件名なし）"}（あと${fmtRemaining(minutesUntil(n, now))}）`;
}

/** 一覧の左に出す時刻。`main` が開始時刻（大きく表示する）、`date` は開始が今日でないときの日付、`sub` は終了など */
export interface TimeLabel {
  date: string | null;
  main: string;
  sub: string;
}

export function timeLabel(item: Item, today: Date): TimeLabel {
  const s = parseLocal(item.start);
  const e = parseLocal(item.end);
  if (item.allDay) {
    const last = new Date(e.getFullYear(), e.getMonth(), e.getDate() - 1);
    return { date: null, main: "終日", sub: last > s ? `${fmtMd(s)}〜${fmtMd(last)}` : "" };
  }
  // 左の列は狭いので、翌日に終わるときは「翌」を付けるだけにする
  const tomorrow = new Date(today.getFullYear(), today.getMonth(), today.getDate() + 1);
  const end = sameDay(e, today) ? fmtHm(e) : sameDay(e, tomorrow) ? `翌${fmtHm(e)}` : `${fmtMd(e)} ${fmtHm(e)}`;
  return {
    date: sameDay(s, today) ? null : fmtMd(s),
    main: fmtHm(s),
    sub: item.end === item.start ? "" : `〜${end}`,
  };
}

/** ヘッダーと折りたたみ表示の「次の予定」（時刻を大きく出すため分けて返す） */
export interface NextParts {
  time: string;
  subject: string;
  remaining: string;
}

export function nextParts(items: Item[], now: Date): NextParts | null {
  const n = nextItem(items, now);
  if (!n) return null;
  return {
    time: fmtHm(parseLocal(n.start)),
    subject: n.subject || "（件名なし）",
    remaining: `あと${fmtRemaining(minutesUntil(n, now))}`,
  };
}

/** "YYYY-MM-DD" に n 日足す（マイナスで前の日） */
export function addDays(date: string, n: number): string {
  const d = parseLocal(date);
  const r = new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);
  return `${r.getFullYear()}-${pad(r.getMonth() + 1)}-${pad(r.getDate())}`;
}

/** 日めくりで表示している日の呼び方（今日・明日・昨日。それ以外は空） */
export function dayLabel(date: string, today: string): string {
  if (date === today) return "今日";
  if (date === addDays(today, 1)) return "明日";
  if (date === addDays(today, -1)) return "昨日";
  return "";
}

/** 終了の何分前から「会議室の次の予約」を警告するか（Rust の ROOM_ALERT_MINUTES と同じ） */
export const ROOM_ALERT_MINUTES = 5;

/** 使う会議室の名前（"会議室A, 会議室B"。なければ空） */
export function roomNames(item: Item): string {
  return (item.rooms ?? []).map((r) => r.name).join(", ");
}

/** 終了直後の予約（"会議室A 11:00〜 件名"）。予約の開始が表示している日でなければ日付を付ける */
export function roomNextTexts(item: Item, day: Date): string[] {
  const result: string[] = [];
  for (const r of item.rooms ?? []) {
    if (!r.next) continue;
    const s = parseLocal(r.next.start);
    const time = sameDay(s, day) ? fmtHm(s) : `${fmtMd(s)} ${fmtHm(s)}`;
    result.push(`${r.name} ${time}〜 ${r.next.subject || "（件名なし）"}`);
  }
  return result;
}

/** 終了直後に会議室の予約があり、終了の5分前〜終了の間か（終日・完了・中止は除く） */
export function roomAlertActive(item: Item, now: Date): boolean {
  if (item.allDay || isHeld(item) || isNotHeld(item)) return false;
  if (!(item.rooms ?? []).some((r) => r.next)) return false;
  const left = parseLocal(item.end).getTime() - now.getTime();
  return left > 0 && left <= ROOM_ALERT_MINUTES * 60_000;
}
