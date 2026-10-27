import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import {
  addDays,
  dayLabel,
  fmtDateHeader,
  fmtHm,
  fmtRange,
  footerText,
  isHeld,
  isNotHeld,
  isStale,
  nextItem,
  nextParts,
  parseLocal,
  phaseOf,
  timeLabel,
  type Item,
  type View,
} from "./logic";

const win = getCurrentWindow();
const $ = (id: string) => document.getElementById(id) as HTMLElement;

let view: View | null = null;
let focusedId: string | null = null;
let focusTimer: number | undefined;
let updating = false;

/** 日めくりで表示している日（null なら今日）と、その日の予定 */
interface DayView {
  date: string;
  items: Item[];
  fetchedAt: string;
}
let selectedDay: string | null = null;
let dayView: DayView | null = null;
let dayLoading = false;
let dayError: string | null = null;
let dayReturnTimer: number | undefined;
/** 今日以外を表示したまま操作しなければ、今日に戻る */
const DAY_RETURN_MS = 5 * 60_000;
/** 今日以外の日の予定を取り直す間隔（今日のポーリングに合わせて） */
const DAY_REFRESH_MS = 60_000;

const COMPACT_KEY = "compact";
const SIZE_KEY = "expandedSize";
const COMPACT_HEIGHT = 40;

function storageGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function storageSet(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // 保存できなくても表示には困らない
  }
}

let compact = storageGet(COMPACT_KEY) === "1";

function span(cls: string, text: string): HTMLSpanElement {
  const el = document.createElement("span");
  el.className = cls;
  el.textContent = text;
  return el;
}

function tag(text: string, cls: string): HTMLSpanElement {
  return span(`tag ${cls}`, text);
}

/** 「次は 18:00 件名 ・ あと25分」（時刻を大きく） */
function renderNext(target: HTMLElement, v: View, now: Date, prefix: string): void {
  const n = nextParts(v.items, now);
  if (!n) {
    target.replaceChildren(span("next-none", "これ以降の予定はありません"));
    return;
  }
  target.replaceChildren(
    span("next-label", prefix),
    span("next-time", n.time),
    span("next-subject", n.subject),
    span("next-remaining", n.remaining),
  );
}

function renderItem(item: Item, now: Date, soonMinutes: number, next: Item | undefined, day: Date): HTMLElement {
  const phase = phaseOf(item, now, soonMinutes);
  const el = document.createElement("div");
  el.className = `item ${phase}`;
  if (isHeld(item)) el.classList.add("held");
  if (isNotHeld(item)) el.classList.add("notheld");
  if (item.id === focusedId) el.classList.add("focused");
  el.dataset.id = item.id;

  // 左：開始時刻（大きく）と終了時刻（表示している日を基準にする）
  const label = timeLabel(item, day);
  const when = document.createElement("div");
  when.className = "when";
  if (label.date) when.append(span("when-date", label.date));
  when.append(span(item.allDay ? "when-main allday" : "when-main", label.main));
  if (label.sub) when.append(span("when-sub", label.sub));

  // 右：件名・状態・種別・場所
  const body = document.createElement("div");
  body.className = "body";
  const row = document.createElement("div");
  row.className = "row";
  row.append(span("subject", item.subject || "（件名なし）"));
  el.title = [fmtRange(item, day), item.subject, item.activityType, item.location].filter(Boolean).join("\n");

  if (phase === "ongoing") row.append(tag("進行中", "ongoing"));
  if (phase === "soon") {
    const mins = Math.ceil((parseLocal(item.start).getTime() - now.getTime()) / 60_000);
    row.append(tag(`あと${mins}分`, "soon"));
  } else if (next && next.id === item.id) {
    row.append(tag("次", "next"));
  }
  if (isHeld(item)) row.append(tag("済", "done"));
  if (isNotHeld(item)) row.append(tag("中止", "done"));
  if (item.badge) {
    const b = tag(item.badge === "new" ? "NEW" : "変更", `${item.badge} badge`);
    b.title = "クリックで消す";
    b.addEventListener("click", (e) => {
      e.stopPropagation();
      void invoke("dismiss_badge", { id: item.id });
    });
    row.append(b);
  }
  body.append(row);

  const meta = [item.activityType, item.location ? `📍 ${item.location}` : ""].filter(Boolean).join("  ");
  if (meta) body.append(span("meta", meta));
  el.append(when, body);

  el.addEventListener("click", () => {
    void invoke("open_record", { id: item.id }).catch((e) => console.error(e));
  });
  return el;
}

function applyDisplay(v: View): void {
  const root = document.documentElement;
  if (v.display.theme === "system") delete root.dataset.theme;
  else root.dataset.theme = v.display.theme;
  root.style.setProperty("--bg-alpha", String(v.display.opacity / 100));
}

function render(): void {
  if (!view) return;
  const now = new Date();
  const v = view;
  applyDisplay(v);

  const other = viewingOtherDay();
  const shownDate = other ? selectedDay! : v.today;
  const items = other ? (dayView?.items ?? []) : v.items;
  const label = dayLabel(shownDate, v.today);

  $("widget").classList.toggle("compact", compact);
  $("date").textContent = fmtDateHeader(parseLocal(shownDate));
  // 今日以外の日は、2行目に「明日の予定を表示しています」と出すので件数だけにする（ヘッダーの幅が足りないため）
  $("count").textContent = other ? `${items.length}件` : `今日の予定 ${items.length}件`;
  $("today").hidden = !other;
  $("header").classList.toggle("other-day", other);
  if (other) {
    const text = dayLoading ? "読み込み中…" : dayError ? `⚠ ${dayError}` : `${label || fmtDateHeader(parseLocal(shownDate))}の予定を表示しています`;
    $("next").replaceChildren(span("day-label", text));
  } else {
    renderNext($("next"), v, now, "次は");
  }
  // 折りたたみ表示はいつも今日の次の予定
  renderNext($("compact"), v, now, "次");
  $("header").classList.toggle("stale", !other && isStale(v.status, now));
  $("refresh").classList.toggle("spinning", other ? dayLoading : v.status.fetching);

  const status = $("status");
  status.textContent = other ? (dayView ? `取得 ${fmtHm(new Date(dayView.fetchedAt))}` : "") : footerText(v.status);
  const isErr = v.status.state === "error" || v.status.state === "auth_error";
  const needsSettings = v.status.state === "auth_error" || v.status.state === "unconfigured";
  status.classList.toggle("error", isErr && !v.status.fetching);
  status.classList.toggle("link", needsSettings);
  status.title = needsSettings ? "クリックで設定を開く" : (v.status.message ?? "");

  const update = $("update") as HTMLButtonElement;
  update.hidden = !v.updateVersion;
  if (v.updateVersion && !updating) {
    update.textContent = `⬆ v${v.updateVersion} に更新`;
    update.title = "クリックで更新して再起動";
  }

  const list = $("list");
  const scroll = list.scrollTop;
  list.replaceChildren();
  if (items.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty";
    empty.textContent = other
      ? dayLoading
        ? "読み込み中…"
        : dayError
          ? "予定を取得できませんでした"
          : `${label || "この日"}の予定はありません`
      : v.status.state === "unconfigured"
        ? "接続設定をすると予定が表示されます"
        : v.status.lastUpdated
          ? "今日の予定はありません"
          : "まだ予定を取得していません";
    list.append(empty);
  } else {
    // 「次」の印は今日だけ
    const next = other ? undefined : nextItem(items, now);
    const day = parseLocal(shownDate);
    for (const item of items) list.append(renderItem(item, now, v.display.soonMinutes, next, day));
  }
  list.scrollTop = scroll;
}

/** 毎分 00 秒に表示を更新する（残り時間・状態の再計算） */
function scheduleMinuteTick(): void {
  const now = new Date();
  const ms = 60_000 - (now.getSeconds() * 1000 + now.getMilliseconds()) + 50;
  window.setTimeout(() => {
    render();
    scheduleMinuteTick();
  }, ms);
}

async function setCompact(on: boolean): Promise<void> {
  const factor = await win.scaleFactor();
  const size = (await win.innerSize()).toLogical(factor);
  if (on) {
    storageSet(SIZE_KEY, JSON.stringify({ width: size.width, height: size.height }));
    await win.setSize(new LogicalSize(size.width, COMPACT_HEIGHT));
  } else {
    let restored = { width: size.width, height: 420 };
    try {
      const saved = JSON.parse(storageGet(SIZE_KEY) ?? "null");
      if (saved && saved.height > COMPACT_HEIGHT) restored = saved;
    } catch {
      // 既定の高さに戻す
    }
    await win.setSize(new LogicalSize(restored.width, restored.height));
  }
  compact = on;
  storageSet(COMPACT_KEY, on ? "1" : "0");
  render();
}

function focusItem(id: string): void {
  focusedId = id;
  if (compact) void setCompact(false);
  render();
  document.querySelector(`.item[data-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "center" });
  window.clearTimeout(focusTimer);
  focusTimer = window.setTimeout(() => {
    focusedId = null;
    render();
  }, 5000);
}

function viewingOtherDay(): boolean {
  return !!view && selectedDay !== null && selectedDay !== view.today;
}

async function loadDay(date: string): Promise<void> {
  dayLoading = true;
  dayError = null;
  render();
  try {
    const d = await invoke<DayView>("get_day", { date });
    if (selectedDay === date) dayView = d;
  } catch (e) {
    if (selectedDay === date) dayError = String(e);
  } finally {
    dayLoading = false;
    render();
  }
}

/** 表示する日を変える（null か今日なら今日に戻る） */
function setDay(date: string | null): void {
  if (!view) return;
  selectedDay = date === view.today ? null : date;
  dayView = null;
  dayError = null;
  window.clearTimeout(dayReturnTimer);
  if (selectedDay) {
    dayReturnTimer = window.setTimeout(() => setDay(null), DAY_RETURN_MS);
    void loadDay(selectedDay);
  }
  $("list").scrollTop = 0;
  render();
}

function moveDay(delta: number): void {
  if (!view) return;
  setDay(addDays(selectedDay ?? view.today, delta));
}

async function installUpdate(): Promise<void> {
  const version = view?.updateVersion;
  if (!version || updating) return;
  if (!window.confirm(`v${version} に更新します。\nウィジェットはいったん終了し、更新が終わると起動し直します。`)) return;
  const btn = $("update") as HTMLButtonElement;
  updating = true;
  btn.disabled = true;
  btn.textContent = "ダウンロード中…";
  try {
    await invoke("install_update");
  } catch (e) {
    updating = false;
    btn.disabled = false;
    btn.textContent = `⬆ v${version} に更新`;
    window.alert(String(e));
  }
}

function setupEvents(): void {
  // ヘッダー：ドラッグで移動、ダブルクリックで折りたたみ
  $("header").addEventListener("mousedown", (e) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    if (e.detail === 2) {
      void setCompact(!compact);
    } else {
      void win.startDragging();
    }
  });
  $("refresh").addEventListener("click", () => {
    if (viewingOtherDay()) void loadDay(selectedDay!);
    else void invoke("refresh_now");
  });
  $("prev-day").addEventListener("click", () => moveDay(-1));
  $("next-day").addEventListener("click", () => moveDay(1));
  $("today").addEventListener("click", () => setDay(null));
  $("add").addEventListener("click", () => {
    const date = viewingOtherDay() ? selectedDay : null;
    void invoke("open_new_event", { date }).catch((e) => window.alert(String(e)));
  });
  $("settings").addEventListener("click", () => void invoke("open_settings"));
  $("update").addEventListener("click", () => void installUpdate());
  // ブラウザの標準のメニュー（戻る・印刷など）の代わりに、このアプリのメニューを出す
  document.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    const itemId = (e.target as HTMLElement).closest<HTMLElement>(".item")?.dataset.id ?? null;
    void invoke("show_context_menu", { itemId, compact });
  });
  $("status").addEventListener("click", () => {
    const s = view?.status.state;
    if (s === "auth_error" || s === "unconfigured") void invoke("open_settings");
  });
}

async function main(): Promise<void> {
  setupEvents();
  await listen<View>("state-changed", (e) => {
    view = e.payload;
    // 今日以外を表示しているときも、ときどき取り直す（急に入った予定に気づけるように）
    if (viewingOtherDay() && dayView && !dayLoading && Date.now() - new Date(dayView.fetchedAt).getTime() > DAY_REFRESH_MS) {
      void loadDay(selectedDay!);
    }
    render();
  });
  await listen<string>("focus-item", (e) => focusItem(e.payload));
  await listen("toggle-compact", () => void setCompact(!compact));
  view = await invoke<View>("get_state");
  render();
  if (compact) {
    // 前回が折りたたみのまま終了していた場合は、高さだけ合わせる
    const factor = await win.scaleFactor();
    const size = (await win.innerSize()).toLogical(factor);
    if (size.height > COMPACT_HEIGHT + 4) await win.setSize(new LogicalSize(size.width, COMPACT_HEIGHT));
  }
  scheduleMinuteTick();
}

void main();
