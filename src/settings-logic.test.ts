// テスト仕様書 7.6（settings-logic.ts）。テスト名の先頭はテストIDに対応する。
import { describe, expect, it } from "vitest";
import { parseMinutesList, parseTypes } from "./settings-logic";

describe("7.6 parseMinutesList", () => {
  it.each([
    ["PM-01", "10", [10]],
    ["PM-02", "10,1", [10, 1]],
    ["PM-02", "1, 10", [10, 1]],
    ["PM-03", "10、5，1", [10, 5, 1]],
    ["PM-03", "10 10 1", [10, 1]],
    ["PM-04", "", []],
    ["PM-04", "  ", []],
    ["PM-05", "0", [0]],
    ["PM-05", "1440", [1440]],
  ])("%s %j", (_id, text, expected) => {
    expect(parseMinutesList(text)).toEqual(expected);
  });

  it.each(["abc", "10,x", "-1", "1.5", "1441"])("PM-06 %j は不正", (text) => {
    expect(parseMinutesList(text)).toBeNull();
  });
});

describe("7.6 parseTypes", () => {
  it("PT-01 空白・空・重複・全角の区切り", () => {
    expect(parseTypes(" Call, Meeting ,,Call、打合せ")).toEqual(["Call", "Meeting", "打合せ"]);
  });
  it("PT-02 空", () => {
    expect(parseTypes("")).toEqual([]);
  });
});
