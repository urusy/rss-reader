import { describe, it, expect, beforeEach } from "vitest";
import {
  hashText,
  loadTtsPos,
  saveTtsPos,
  clearTtsPos,
} from "./tts-progress";

beforeEach(() => localStorage.clear());

const save = (
  id: string,
  src: string,
  text: string,
  chunk: number,
  ratio = 0,
  t = 1,
) =>
  saveTtsPos(id, src, {
    chunk,
    len: text.length,
    hash: hashText(text),
    ratio,
    t,
  });

describe("hashText", () => {
  it("is deterministic and unsigned", () => {
    expect(hashText("hello")).toBe(hashText("hello"));
    expect(hashText("hello")).toBeGreaterThanOrEqual(0);
    expect(hashText("hello")).not.toBe(hashText("hellp"));
  });
});

describe("tts-progress store", () => {
  it("round-trips chunk and ratio for the same text", () => {
    save("a1", "body", "one two three", 2, 0.4);
    expect(loadTtsPos("a1", "body", "one two three")).toEqual({
      chunk: 2,
      ratio: 0.4,
    });
  });

  it("invalidates on length mismatch (excerpt⇄fulltext)", () => {
    save("a1", "body", "short text", 1);
    expect(loadTtsPos("a1", "body", "a different longer text")).toBeNull();
  });

  it("invalidates on hash mismatch at equal length", () => {
    save("a1", "body", "abcde", 1);
    expect(loadTtsPos("a1", "body", "abcdz")).toBeNull(); // same length, diff hash
  });

  it("keeps sources independent per article", () => {
    save("a1", "body", "body text", 3);
    save("a1", "summary", "summary text", 1);
    expect(loadTtsPos("a1", "body", "body text")?.chunk).toBe(3);
    expect(loadTtsPos("a1", "summary", "summary text")?.chunk).toBe(1);
  });

  it("clears one entry without touching others", () => {
    save("a1", "body", "body text", 3);
    save("a1", "summary", "summary text", 1);
    clearTtsPos("a1", "summary");
    expect(loadTtsPos("a1", "summary", "summary text")).toBeNull();
    expect(loadTtsPos("a1", "body", "body text")?.chunk).toBe(3);
  });

  it("prunes the oldest (smallest t) entries beyond the cap", () => {
    // 201 件を古い→新しい順（t=1..201）で保存 → 最古(t=1)が剪定される。
    for (let i = 1; i <= 201; i++) {
      save(`article-${i}`, "body", `text ${i}`, 0, 0, i);
    }
    expect(loadTtsPos("article-1", "body", "text 1")).toBeNull(); // 剪定済み
    expect(loadTtsPos("article-201", "body", "text 201")?.chunk).toBe(0);
  });

  it("does not throw and returns null on corrupt storage", () => {
    localStorage.setItem("tts-pos", "not json");
    expect(loadTtsPos("a1", "body", "x")).toBeNull();
    expect(() => save("a1", "body", "x", 0)).not.toThrow();
    expect(loadTtsPos("a1", "body", "x")?.chunk).toBe(0);
  });
});
