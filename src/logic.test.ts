// テスト仕様書 7章（logic.ts）。テスト名の先頭はテストIDに対応する。
import { describe, expect, it } from "vitest";
import {
  compactText,
  fmtDateHeader,
  fmtRange,
  fmtRemaining,
  footerText,
  isHeld,
  isNotHeld,
  isStale,
  nextItem,
  nextParts,
  nextText,
  parseLocal,
  phaseOf,
  timeLabel,
  type Item,
  type Status,
} from "./logic";

const at = (s: string) => parseLocal(s.replace(" ", "T"));

function item(id: string, start: string, end: string, extra: Partial<Item> = {}): Item {
  return {
    id,
    recordId: 1,
    subject: `件名${id}`,
    activityType: "Meeting",
    start: start.replace(" ", "T"),
    end: end.replace(" ", "T"),
    allDay: false,
    status: "Planned",
    ...extra,
  };
}

const ok: Status = { state: "ok", message: null, lastUpdated: null, fetching: false };
const std = item("1", "2026-10-27 14:00", "2026-10-27 15:00");

describe("7.1 phaseOf", () => {
  it("PH-01 終日は時刻に関係なく allday", () => {
    const a = item("2", "2026-10-27 00:00", "2026-10-28 00:00", { allDay: true });
    for (const now of ["2026-10-26 23:00", "2026-10-27 12:00", "2026-10-28 01:00"]) {
      expect(phaseOf(a, at(now), 15)).toBe("allday");
    }
  });
  it("PH-02 開始の15分より前は upcoming", () => expect(phaseOf(std, at("2026-10-27 13:00"), 15)).toBe("upcoming"));
  it("PH-03 開始の15分前から開始までは soon", () => expect(phaseOf(std, at("2026-10-27 13:50"), 15)).toBe("soon"));
  it("PH-04 開始から終了までは ongoing", () => expect(phaseOf(std, at("2026-10-27 14:30"), 15)).toBe("ongoing"));
  it("PH-05 終了後は done", () => expect(phaseOf(std, at("2026-10-27 16:00"), 15)).toBe("done"));
  it("PH-06 長さ0の予定で開始前は upcoming / soon", () => {
    const z = item("3", "2026-10-27 14:00", "2026-10-27 14:00");
    expect(phaseOf(z, at("2026-10-27 13:00"), 15)).toBe("upcoming");
    expect(phaseOf(z, at("2026-10-27 13:59"), 15)).toBe("soon");
  });
  it("PH-07 長さ0の予定で開始を過ぎたら done", () => {
    const z = item("3", "2026-10-27 14:00", "2026-10-27 14:00");
    expect(phaseOf(z, at("2026-10-27 14:00"), 15)).toBe("done");
    expect(phaseOf(z, at("2026-10-27 15:00"), 15)).toBe("done");
  });
  it.each([
    ["2026-10-27 13:44", "upcoming"],
    ["2026-10-27 13:45", "soon"],
    ["2026-10-27 13:59", "soon"],
    ["2026-10-27 14:00", "ongoing"],
    ["2026-10-27 14:59", "ongoing"],
    ["2026-10-27 15:00", "done"],
  ])("PH-08 境界 %s → %s", (now, expected) => {
    expect(phaseOf(std, at(now), 15)).toBe(expected);
  });
  it("PH-09 直前とみなす時間が0なら soon にならない", () => {
    expect(phaseOf(std, at("2026-10-27 13:59"), 0)).toBe("upcoming");
  });
});

describe("7.2 nextItem / nextText / compactText", () => {
  const items = [
    item("0", "2026-10-27 00:00", "2026-10-28 00:00", { allDay: true }),
    item("1", "2026-10-27 09:00", "2026-10-27 10:00"),
    item("9", "2026-10-27 10:30", "2026-10-27 11:00", { status: "Not Held" }),
    item("8", "2026-10-27 10:40", "2026-10-27 11:00", { status: "Held" }),
    item("3", "2026-10-27 13:00", "2026-10-27 14:00"),
    item("2", "2026-10-27 11:00", "2026-10-27 12:00", { subject: "" }),
  ];
  it("NE-01 終日・完了・中止・開始済みを除いた一番早い予定", () => {
    expect(nextItem(items, at("2026-10-27 09:30"))?.id).toBe("2");
  });
  it("NE-02 開始ちょうどの予定は次の予定にしない", () => {
    expect(nextItem(items, at("2026-10-27 11:00"))?.id).toBe("3");
  });
  it("NE-03 候補がない / 空", () => {
    expect(nextItem(items, at("2026-10-27 13:00"))).toBeUndefined();
    expect(nextItem([], at("2026-10-27 13:00"))).toBeUndefined();
    expect(nextText(items, at("2026-10-27 13:00"))).toBe("これ以降の予定はありません");
    expect(compactText([], at("2026-10-27 13:00"))).toBe("これ以降の予定はありません");
  });
  it("NE-04 残り時間（切り上げ）", () => {
    expect(nextText(items, at("2026-10-27 09:30"))).toBe("次の予定まで あと 1時間30分");
    expect(nextText(items, new Date(2026, 9, 27, 12, 59, 30))).toBe("次の予定まで あと 1分");
  });
  it("NE-05 件名が空なら（件名なし）", () => {
    expect(compactText(items, at("2026-10-27 10:35"))).toBe("次 11:00 （件名なし）（あと25分）");
    expect(compactText(items, at("2026-10-27 11:30"))).toBe("次 13:00 件名3（あと1時間30分）");
  });
});

describe("7.3 fmtRange", () => {
  const today = new Date(2026, 9, 27);
  it("RG-01 開始・終了とも今日", () => {
    expect(fmtRange(item("1", "2026-10-27 09:00", "2026-10-27 10:30"), today)).toBe("09:00-10:30");
  });
  it("RG-02 開始が前日", () => {
    expect(fmtRange(item("1", "2026-10-26 22:00", "2026-10-27 01:00"), today)).toBe("10/26 22:00-01:00");
  });
  it("RG-03 終了が翌日", () => {
    expect(fmtRange(item("1", "2026-10-27 23:00", "2026-10-28 01:00"), today)).toBe("23:00-10/28 01:00");
  });
  it("RG-04 開始・終了とも今日以外", () => {
    expect(fmtRange(item("1", "2026-10-26 09:00", "2026-10-28 18:00"), today)).toBe("10/26 09:00-10/28 18:00");
  });
  it("RG-05 終日（1日 / 長さ0）", () => {
    expect(fmtRange(item("1", "2026-10-27 00:00", "2026-10-28 00:00", { allDay: true }), today)).toBe("終日");
    expect(fmtRange(item("1", "2026-10-27 00:00", "2026-10-27 00:00", { allDay: true }), today)).toBe("終日");
  });
  it("RG-06 終日（複数日）", () => {
    expect(fmtRange(item("1", "2026-10-26 00:00", "2026-10-29 00:00", { allDay: true }), today)).toBe("終日 10/26〜10/28");
  });
});

describe("7.4 その他の表示", () => {
  it("DH-10 fmtDateHeader", () => {
    expect(fmtDateHeader(new Date(2026, 9, 27))).toBe("10/27(火)");
    expect(fmtDateHeader(new Date(2026, 0, 4))).toBe("1/4(日)");
  });
  it.each([
    [-3, "0分"],
    [0, "0分"],
    [59, "59分"],
    [60, "1時間"],
    [125, "2時間5分"],
  ])("FM-01 fmtRemaining(%i) → %s", (m, s) => expect(fmtRemaining(m)).toBe(s));
  it("PL-01 parseLocal", () => {
    expect(parseLocal("2026-10-27T09:05")).toEqual(new Date(2026, 9, 27, 9, 5));
    expect(parseLocal("2026-10-27")).toEqual(new Date(2026, 9, 27, 0, 0));
  });
  const now = at("2026-10-27 12:00");
  const minutesAgo = (m: number) => new Date(now.getTime() - m * 60_000).toISOString();
  it("IS-01 最終更新なしは古くない", () => expect(isStale(ok, now)).toBe(false));
  it("IS-02 29分前は古くない", () => expect(isStale({ ...ok, lastUpdated: minutesAgo(29) }, now)).toBe(false));
  it("IS-03 30分前ちょうどは古い", () => expect(isStale({ ...ok, lastUpdated: minutesAgo(30) }, now)).toBe(true));
  it("IS-04 31分前は古い", () => expect(isStale({ ...ok, lastUpdated: minutesAgo(31) }, now)).toBe(true));
  it("HN-01 isHeld / isNotHeld", () => {
    const s = (status: string) => item("1", "2026-10-27 09:00", "2026-10-27 10:00", { status });
    expect(isHeld(s(" HELD "))).toBe(true);
    expect(isHeld(s("not held"))).toBe(false);
    expect(isNotHeld(s("not held"))).toBe(true);
    expect(isNotHeld(s(" Not Held "))).toBe(true);
    expect(isHeld(s("Planned"))).toBe(false);
    expect(isNotHeld(s("Planned"))).toBe(false);
  });
});

describe("7.5 footerText", () => {
  const last = new Date(2026, 9, 27, 13, 42).toISOString();
  it("FT-01 取得中はどの状態でも「更新中…」", () => {
    for (const state of ["ok", "error", "auth_error", "unconfigured"] as const) {
      expect(footerText({ ...ok, state, fetching: true, message: "x", lastUpdated: last })).toBe("更新中…");
    }
  });
  it("FT-02 ok・最終更新あり", () => expect(footerText({ ...ok, lastUpdated: last })).toBe("更新 13:42"));
  it("FT-03 ok・最終更新なし", () => expect(footerText(ok)).toBe(""));
  it("FT-04 error・最終更新あり・メッセージあり", () => {
    expect(footerText({ ...ok, state: "error", message: "接続できません（タイムアウト）", lastUpdated: last })).toBe(
      "⚠ 接続できません（タイムアウト）（最終更新 13:42）",
    );
  });
  it("FT-05 error・最終更新なし・メッセージあり", () => {
    expect(footerText({ ...ok, state: "error", message: "接続できません" })).toBe("⚠ 接続できません");
  });
  it("FT-06 error・メッセージなし", () => {
    expect(footerText({ ...ok, state: "error", message: null })).toBe("⚠ 接続できません");
  });
  it("FT-07 auth_error（メッセージあり / なし）", () => {
    expect(footerText({ ...ok, state: "auth_error", message: "予定を見る権限がありません", lastUpdated: last })).toBe(
      "⚠ 予定を見る権限がありません",
    );
    expect(footerText({ ...ok, state: "auth_error", message: null })).toBe("⚠ ログインできません");
  });
  it("FT-08 unconfigured", () => {
    expect(footerText({ ...ok, state: "unconfigured", lastUpdated: last })).toBe("接続設定をしてください");
  });
});

describe("7.7 timeLabel（一覧の左の時刻）", () => {
  const today = new Date(2026, 9, 27);
  it("TL-01 今日の予定：開始を大きく、終了を「〜」で", () => {
    expect(timeLabel(item("1", "2026-10-27 09:00", "2026-10-27 10:30"), today)).toEqual({ date: null, main: "09:00", sub: "〜10:30" });
  });
  it("TL-02 前日から続く予定：日付を付ける", () => {
    expect(timeLabel(item("1", "2026-10-26 22:00", "2026-10-27 01:00"), today)).toEqual({ date: "10/26", main: "22:00", sub: "〜01:00" });
  });
  it("TL-03 翌日に終わる予定：終了に「翌」を付ける", () => {
    expect(timeLabel(item("1", "2026-10-27 23:00", "2026-10-28 01:00"), today)).toEqual({ date: null, main: "23:00", sub: "〜翌01:00" });
  });
  it("TL-07 2日以上あとに終わる予定：終了に日付を付ける", () => {
    expect(timeLabel(item("1", "2026-10-27 09:00", "2026-10-29 18:00"), today)).toEqual({ date: null, main: "09:00", sub: "〜10/29 18:00" });
  });
  it("TL-04 長さ0の予定：終了を出さない", () => {
    expect(timeLabel(item("1", "2026-10-27 14:00", "2026-10-27 14:00"), today)).toEqual({ date: null, main: "14:00", sub: "" });
  });
  it("TL-05 終日（1日）", () => {
    expect(timeLabel(item("1", "2026-10-27 00:00", "2026-10-28 00:00", { allDay: true }), today)).toEqual({ date: null, main: "終日", sub: "" });
  });
  it("TL-06 終日（複数日）", () => {
    expect(timeLabel(item("1", "2026-10-26 00:00", "2026-10-29 00:00", { allDay: true }), today)).toEqual({
      date: null,
      main: "終日",
      sub: "10/26〜10/28",
    });
  });
});

describe("7.8 nextParts（次の予定の表示）", () => {
  const items = [
    item("1", "2026-10-27 09:00", "2026-10-27 10:00"),
    item("2", "2026-10-27 18:00", "2026-10-27 18:30", { subject: "NHK夕会" }),
    item("3", "2026-10-27 13:00", "2026-10-27 14:00", { subject: "" }),
  ];
  it("NP-01 次の予定の時刻・件名・残り時間", () => {
    expect(nextParts(items, at("2026-10-27 17:35"))).toEqual({ time: "18:00", subject: "NHK夕会", remaining: "あと25分" });
  });
  it("NP-02 件名が空なら（件名なし）・1時間以上は時間で", () => {
    expect(nextParts(items, at("2026-10-27 11:30"))).toEqual({ time: "13:00", subject: "（件名なし）", remaining: "あと1時間30分" });
  });
  it("NP-03 次の予定がない", () => {
    expect(nextParts(items, at("2026-10-27 18:00"))).toBeNull();
    expect(nextParts([], at("2026-10-27 18:00"))).toBeNull();
  });
});
