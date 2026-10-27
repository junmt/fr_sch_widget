import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { parseMinutesList, parseTypes } from "./settings-logic";

interface Settings {
  crmUrl: string;
  username: string;
  crmTimeZone: "local" | "utc";
  intervalMinutes: number;
  reminderMinutes: number[];
  soonMinutes: number;
  notifyNewChanged: boolean;
  notifyDeleted: boolean;
  activityTypes: string[];
  showNotHeld: boolean;
  showHeld: boolean;
  alwaysOnTop: boolean;
  opacity: number;
  theme: "system" | "light" | "dark";
  autostart: boolean;
  autoUpdateCheck: boolean;
}

interface SettingsView {
  settings: Settings;
  hasAccessKey: boolean;
}

const input = (id: string) => document.getElementById(id) as HTMLInputElement;
const el = (id: string) => document.getElementById(id) as HTMLElement;

let hasAccessKey = false;

function setResult(id: string, text: string, ok: boolean | null): void {
  const r = el(id);
  r.textContent = text;
  r.classList.toggle("ok", ok === true);
  r.classList.toggle("ng", ok === false);
}

function applyTheme(theme: string): void {
  if (theme === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
}

function fill(v: SettingsView): void {
  const s = v.settings;
  hasAccessKey = v.hasAccessKey;
  input("crmUrl").value = s.crmUrl;
  input("username").value = s.username;
  (document.getElementById("crmTimeZone") as HTMLSelectElement).value = s.crmTimeZone;
  input("accessKey").value = "";
  input("accessKey").placeholder = hasAccessKey ? "保存済み（変更するときだけ入力）" : "";
  input("intervalMinutes").value = String(s.intervalMinutes);
  input("reminderMinutes").value = s.reminderMinutes.join(",");
  input("soonMinutes").value = String(s.soonMinutes);
  input("notifyNewChanged").checked = s.notifyNewChanged;
  input("notifyDeleted").checked = s.notifyDeleted;
  input("activityTypes").value = s.activityTypes.join(", ");
  input("showHeld").checked = s.showHeld;
  input("showNotHeld").checked = s.showNotHeld;
  input("alwaysOnTop").checked = s.alwaysOnTop;
  input("opacity").value = String(s.opacity);
  (document.getElementById("theme") as HTMLSelectElement).value = s.theme;
  input("autostart").checked = s.autostart;
  input("autoUpdateCheck").checked = s.autoUpdateCheck;
  updateDerived();
  applyTheme(s.theme);
}

function updateDerived(): void {
  el("opacityValue").textContent = `${input("opacity").value}%`;
  el("httpWarn").hidden = !input("crmUrl").value.trim().toLowerCase().startsWith("http://");
}

/** 入力を読み取る。不正な値があればエラーメッセージを返す */
function collect(): { settings: Settings } | { error: string } {
  const crmUrl = input("crmUrl").value.trim();
  const username = input("username").value.trim();
  if (!crmUrl || !username) return { error: "CRM の URL とユーザー名を入力してください" };
  if (!/^https?:\/\//i.test(crmUrl)) return { error: "CRM の URL は http:// か https:// で始めてください" };
  if (!hasAccessKey && !input("accessKey").value.trim()) return { error: "アクセスキーを入力してください" };

  const interval = Number(input("intervalMinutes").value);
  if (!Number.isInteger(interval) || interval < 1 || interval > 30) return { error: "更新間隔は 1〜30 の整数で入力してください" };
  const reminders = parseMinutesList(input("reminderMinutes").value);
  if (reminders === null) return { error: "リマインドは「10,1」のように 0〜1440 の数字をカンマで区切って入力してください" };
  const soon = Number(input("soonMinutes").value);
  if (!Number.isInteger(soon) || soon < 0 || soon > 1440) return { error: "「直前」とみなす時間は 0〜1440 の整数で入力してください" };

  return {
    settings: {
      crmUrl,
      username,
      crmTimeZone: crmTimeZone(),
      intervalMinutes: interval,
      reminderMinutes: reminders,
      soonMinutes: soon,
      notifyNewChanged: input("notifyNewChanged").checked,
      notifyDeleted: input("notifyDeleted").checked,
      activityTypes: parseTypes(input("activityTypes").value),
      showHeld: input("showHeld").checked,
      showNotHeld: input("showNotHeld").checked,
      alwaysOnTop: input("alwaysOnTop").checked,
      opacity: Number(input("opacity").value),
      theme: (document.getElementById("theme") as HTMLSelectElement).value as Settings["theme"],
      autostart: input("autostart").checked,
      autoUpdateCheck: input("autoUpdateCheck").checked,
    },
  };
}

function crmTimeZone(): Settings["crmTimeZone"] {
  return (document.getElementById("crmTimeZone") as HTMLSelectElement).value as Settings["crmTimeZone"];
}

function accessKeyArg(): string | null {
  const k = input("accessKey").value.trim();
  return k ? k : null;
}

async function testConnection(): Promise<void> {
  const btn = document.getElementById("test") as HTMLButtonElement;
  btn.disabled = true;
  setResult("testResult", "接続しています…", null);
  try {
    const msg = await invoke<string>("test_connection", {
      crmUrl: input("crmUrl").value,
      username: input("username").value,
      accessKey: accessKeyArg(),
      crmTimeZone: crmTimeZone(),
    });
    setResult("testResult", msg, true);
  } catch (e) {
    setResult("testResult", String(e), false);
  } finally {
    btn.disabled = false;
  }
}

async function save(e: Event): Promise<void> {
  e.preventDefault();
  const c = collect();
  if ("error" in c) {
    setResult("saveResult", c.error, false);
    return;
  }
  const btn = document.getElementById("save") as HTMLButtonElement;
  btn.disabled = true;
  try {
    const v = await invoke<SettingsView>("save_settings", { settings: c.settings, accessKey: accessKeyArg() });
    fill(v);
    setResult("saveResult", "保存しました", true);
  } catch (err) {
    setResult("saveResult", String(err), false);
  } finally {
    btn.disabled = false;
  }
}

async function main(): Promise<void> {
  fill(await invoke<SettingsView>("get_settings"));
  document.getElementById("form")!.addEventListener("submit", (e) => void save(e));
  document.getElementById("test")!.addEventListener("click", () => void testConnection());
  document.getElementById("close")!.addEventListener("click", () => void getCurrentWindow().close());
  input("opacity").addEventListener("input", updateDerived);
  input("crmUrl").addEventListener("input", updateDerived);
}

void main();
