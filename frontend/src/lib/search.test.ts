import { describe, it, expect } from "vitest";
import { normalizeQuery, searchHref } from "./search";

describe("normalizeQuery", () => {
  it("前後の空白を落とす", () => {
    expect(normalizeQuery("  rust  ")).toBe("rust");
  });
  it("内側の空白は保持する", () => {
    expect(normalizeQuery("  rust async  ")).toBe("rust async");
  });
});

describe("searchHref", () => {
  it("通常クエリを /search?q= に組み立てる", () => {
    expect(searchHref("rust")).toBe("/search?q=rust");
  });
  it("空・空白のみは null", () => {
    expect(searchHref("")).toBeNull();
    expect(searchHref("   ")).toBeNull();
  });
  it("日本語と記号を URL エンコードする", () => {
    expect(searchHref("機械学習")).toBe("/search?q=%E6%A9%9F%E6%A2%B0%E5%AD%A6%E7%BF%92");
    expect(searchHref("c& d")).toBe("/search?q=c%26%20d");
  });
});
