import { describe, it, expect } from "vitest";
import { scopeFromPath } from "./selection";

describe("scopeFromPath", () => {
  it("ルートは all", () => {
    expect(scopeFromPath("/", {})).toEqual({ kind: "all" });
  });
  it("/feeds/:feedId は feed", () => {
    expect(scopeFromPath("/feeds/abc", { feedId: "abc" })).toEqual({
      kind: "feed",
      feedId: "abc",
    });
  });
  it("/folders/:folderId は folder", () => {
    expect(scopeFromPath("/folders/xyz", { folderId: "xyz" })).toEqual({
      kind: "folder",
      folderId: "xyz",
    });
  });
  it("/folders/unclassified もセンチネルとして folder", () => {
    expect(
      scopeFromPath("/folders/unclassified", { folderId: "unclassified" }),
    ).toEqual({ kind: "folder", folderId: "unclassified" });
  });
  it("記事本文表示中は all 扱い（一覧 scope に影響しない）", () => {
    expect(scopeFromPath("/articles/1", { id: "1" })).toEqual({ kind: "all" });
  });
  it("不明パスは all にフォールバック", () => {
    expect(scopeFromPath("/manage", {})).toEqual({ kind: "all" });
  });
});
