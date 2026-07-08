import { describe, it, expect } from "vitest";
import { faviconUrlFor, sourceInitial, avatarColor } from "./favicon";

describe("faviconUrlFor", () => {
  it("記事URLのオリジンから /favicon.ico を作る", () => {
    expect(faviconUrlFor("https://blog.example.com/2026/07/post")).toBe(
      "https://blog.example.com/favicon.ico",
    );
  });

  it("ポート・クエリ・フラグメントは落としてオリジンだけ使う", () => {
    expect(faviconUrlFor("http://example.com:8080/a?b=1#c")).toBe(
      "http://example.com:8080/favicon.ico",
    );
  });

  it("パース不能な URL は null", () => {
    expect(faviconUrlFor("not-a-url")).toBeNull();
    expect(faviconUrlFor("")).toBeNull();
  });
});

describe("sourceInitial", () => {
  it("先頭1文字を大文字で返す", () => {
    expect(sourceInitial("Example Blog")).toBe("E");
    expect(sourceInitial("hello")).toBe("H");
  });

  it("前後の空白は無視する", () => {
    expect(sourceInitial("  yahoo")).toBe("Y");
  });

  it("空文字・空白のみは ?", () => {
    expect(sourceInitial("")).toBe("?");
    expect(sourceInitial("   ")).toBe("?");
  });

  it("日本語やサロゲートペアの先頭1文字", () => {
    expect(sourceInitial("あるブログ")).toBe("あ");
    expect(sourceInitial("𝔊 blog")).toBe("𝔊");
  });
});

describe("avatarColor", () => {
  it("同じ seed には常に同じ色（決定的）", () => {
    expect(avatarColor("example")).toBe(avatarColor("example"));
  });

  it("hsl(<hue> 55% 45%) 形式で hue は 0..359", () => {
    const m = avatarColor("whatever").match(/^hsl\((\d+) 55% 45%\)$/);
    expect(m).not.toBeNull();
    const hue = Number(m![1]);
    expect(hue).toBeGreaterThanOrEqual(0);
    expect(hue).toBeLessThan(360);
  });

  it("異なる seed はおおむね異なる色になる", () => {
    expect(avatarColor("aaa")).not.toBe(avatarColor("zzz"));
  });
});
