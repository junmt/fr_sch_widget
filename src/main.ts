import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import {
  compactText,
  fmtDateHeader,
  fmtRange,
  footerText,
  isHeld,
  isNotHeld,
  isStale,
  nextItem,
  nextText,
  parseLocal,
  phaseOf,
  type Item,
  type View,
} from "./logic";

const win = getCurrentWindow();
const $ = (id: string) => document.getElementById(id) as HTMLElement;

let view: View | null = null;
let focusedId: string | null = null;
let focusTimer: number | undefined;

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

function tag(text: string, cls: string): HTMLSpanElement {
  const el = document.createElement("span");
  el.className = `tag ${cls}`;
  el.textContent = text;
  return el;
}

function renderItem(item: Item, now: Date, soonMinutes: number, next: Item | undefined): HTMLElement {
  const phase = phaseOf(item, now, soonMinutes);
  const el = document.createElement("div");
  el.className = `item ${phase}`;
  if (isHeld(item)) el.classList.add("held");
  if (isNotHeld(item)) el.classList.add("notheld");
  if (item.id === focusedId) el.classList.add("focused");
  el.dataset.id = item.id;

  const row = document.createElement("div");
  row.className = "row";
  const time = document.createElement("span");
  time.className = "time";
  time.textContent = fmtRange(item, parseLocal(view!.today));
  const subject = document.createElement("span");
  subject.className = "subject";
  subject.textContent = item.subject || "（件名なし）";
  el.title = [item.subject, item.activityType, item.location].filter(Boolean).join("\n");
  row.append(time, subject);

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
  el.append(row);

  const meta = [item.activityType, item.location ? `📍 ${item.location}` : ""].filter(Boolean).join("  ");
  if (meta) {
    const m = document.createElement("div");
    m.className = "meta";
    m.textContent = meta;
    el.append(m);
  }

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

  $("widget").classList.toggle("compact", compact);
  $("date").textContent = fmtDateHeader(parseLocal(v.today));
  $("count").textContent = `今日の予定 ${v.items.length}件`;
  $("next").textContent = nextText(v.items, now);
  $("compact").textContent = compactText(v.items, now);
  $("header").classList.toggle("stale", isStale(v.status, now));
  $("refresh").classList.toggle("spinning", v.status.fetching);

  const status = $("status");
  status.textContent = footerText(v.status);
  const isErr = v.status.state === "error" || v.status.state === "auth_error";
  const needsSettings = v.status.state === "auth_error" || v.status.state === "unconfigured";
  status.classList.toggle("error", isErr && !v.status.fetching);
  status.classList.toggle("link", needsSettings);
  status.title = needsSettings ? "クリックで設定を開く" : (v.status.message ?? "");

  const list = $("list");
  const scroll = list.scrollTop;
  list.replaceChildren();
  if (v.items.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty";
    empty.textContent = v.status.state === "unconfigured" ? "接続設定をすると予定が表示されます" : "今日の予定はありません";
    list.append(empty);
  } else {
    const next = nextItem(v.items, now);
    for (const item of v.items) list.append(renderItem(item, now, v.display.soonMinutes, next));
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
  $("refresh").addEventListener("click", () => void invoke("refresh_now"));
  $("settings").addEventListener("click", () => void invoke("open_settings"));
  $("status").addEventListener("click", () => {
    const s = view?.status.state;
    if (s === "auth_error" || s === "unconfigured") void invoke("open_settings");
  });
}

async function main(): Promise<void> {
  setupEvents();
  await listen<View>("state-changed", (e) => {
    view = e.payload;
    render();
  });
  await listen<string>("focus-item", (e) => focusItem(e.payload));
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
