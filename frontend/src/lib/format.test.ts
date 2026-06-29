import { describe, it, expect } from "vitest";
import { formatDate, postsPerWeekLabel } from "./format";

describe("formatDate (Asia/Tokyo 固定)", () => {
  it("UTC 0時は JST 同日", () => {
    expect(formatDate("2026-06-26T00:00:00Z")).toBe("2026/06/26");
  });
  it("UTC 15時は JST 翌日（TZ固定の境界）", () => {
    expect(formatDate("2026-06-25T15:00:00Z")).toBe("2026/06/26");
  });
  it("不正な ISO は空文字", () => {
    expect(formatDate("not-a-date")).toBe("");
  });
});

describe("postsPerWeekLabel", () => {
  it("0 は投稿なし", () => {
    expect(postsPerWeekLabel(0)).toBe("投稿なし");
  });
  it("整数は末尾 .0 を省く", () => {
    expect(postsPerWeekLabel(7)).toBe("週7件");
  });
  it("小数は1桁", () => {
    expect(postsPerWeekLabel(3.5)).toBe("週3.5件");
  });
});
