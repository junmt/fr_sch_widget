// 画面の表示ロジック（DOM に依存しない）

export type Badge = "new" | "changed";

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
