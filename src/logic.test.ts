// テスト仕様書 7章（logic.ts）。テスト名の先頭はテストIDに対応する。
import { describe, expect, it } from "vitest";
import {
  addDays,
  dayLabel,
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
  roomAlertActive,
  roomNames,
  roomNextTexts,
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

describe("7.9 日めくり addDays / dayLabel", () => {
  it("DA-01 次の日・前の日", () => {
    expect(addDays("2026-10-27", 1)).toBe("2026-10-28");
    expect(addDays("2026-10-27", -1)).toBe("2026-10-26");
    expect(addDays("2026-10-27", 0)).toBe("2026-10-27");
  });
  it("DA-02 月・年・うるう日の境目", () => {
    expect(addDays("2026-10-31", 1)).toBe("2026-11-01");
    expect(addDays("2026-12-31", 1)).toBe("2027-01-01");
    expect(addDays("2026-01-01", -1)).toBe("2025-12-31");
    expect(addDays("2028-02-28", 1)).toBe("2028-02-29");
    expect(addDays("2027-02-28", 1)).toBe("2027-03-01");
  });
  it("DA-03 何日も先", () => {
    expect(addDays("2026-10-27", 30)).toBe("2026-11-26");
  });
  it.each([
    ["DL-01", "2026-10-27", "今日"],
    ["DL-02", "2026-10-28", "明日"],
    ["DL-03", "2026-10-26", "昨日"],
    ["DL-04", "2026-10-29", ""],
    ["DL-04", "2026-10-25", ""],
  ])("%s %s → %j", (_id, date, expected) => {
    expect(dayLabel(date, "2026-10-27")).toBe(expected);
  });
  it("DL-05 月をまたぐ明日・昨日", () => {
    expect(dayLabel("2026-11-01", "2026-10-31")).toBe("明日");
    expect(dayLabel("2026-10-31", "2026-11-01")).toBe("昨日");
  });
});

describe("7.10 会議室 roomNames / roomNextTexts / roomAlertActive", () => {
  const next = { subject: "他部署MTG", start: "2026-10-27T15:00", end: "2026-10-27T16:00" };
  const withNext = item("1", "2026-10-27 14:00", "2026-10-27 15:00", {
    rooms: [{ name: "会議室A", next }, { name: "会議室B" }],
  });
  const day = parseLocal("2026-10-27");

  it("RN-01 会議室なし（rooms がない・空）", () => {
    expect(roomNames(std)).toBe("");
    expect(roomNames({ ...std, rooms: [] })).toBe("");
    expect(roomNextTexts(std, day)).toEqual([]);
  });
  it("RN-02 会議室の名前をカンマで並べる", () => {
    expect(roomNames(withNext)).toBe("会議室A, 会議室B");
  });
  it("RN-03 次の予約がある会議室だけ文言にする", () => {
    expect(roomNextTexts(withNext, day)).toEqual(["会議室A 15:00〜 他部署MTG"]);
  });
  it("RN-04 予約の開始が表示している日でなければ日付を付ける", () => {
    const i = item("1", "2026-10-27 23:00", "2026-10-28 00:30", {
      rooms: [{ name: "会議室A", next: { subject: "朝会", start: "2026-10-28T00:30", end: "2026-10-28T01:00" } }],
    });
    expect(roomNextTexts(i, day)).toEqual(["会議室A 10/28 00:30〜 朝会"]);
  });
  it("RN-05 予約の件名が空なら（件名なし）", () => {
    const i = { ...std, rooms: [{ name: "会議室A", next: { ...next, subject: "" } }] };
    expect(roomNextTexts(i, day)).toEqual(["会議室A 15:00〜 （件名なし）"]);
  });

  it.each([
    ["RW-01", "2026-10-27 14:54", false, "終了の6分前はまだ"],
    ["RW-02", "2026-10-27 14:55", true, "終了のちょうど5分前から"],
    ["RW-03", "2026-10-27 14:59", true, "終了の1分前"],
    ["RW-04", "2026-10-27 15:00", false, "終了ちょうどで終わり"],
    ["RW-05", "2026-10-27 15:30", false, "終了後"],
  ])("%s %s → %s（%s）", (_id, now, expected) => {
    expect(roomAlertActive(withNext, at(now))).toBe(expected);
  });
  it("RW-06 次の予約がない・会議室がないなら出さない", () => {
    expect(roomAlertActive({ ...withNext, rooms: [{ name: "会議室B" }] }, at("2026-10-27 14:58"))).toBe(false);
    expect(roomAlertActive(std, at("2026-10-27 14:58"))).toBe(false);
  });
  it("RW-07 終日・完了・中止は出さない", () => {
    expect(roomAlertActive({ ...withNext, allDay: true }, at("2026-10-27 14:58"))).toBe(false);
    expect(roomAlertActive({ ...withNext, status: "Held" }, at("2026-10-27 14:58"))).toBe(false);
    expect(roomAlertActive({ ...withNext, status: "Not Held" }, at("2026-10-27 14:58"))).toBe(false);
  });
});
