import { describe, it, expect } from "vitest";
import { normalizeForTts } from "./tts-normalize";
import type { DictEntry } from "./tts-dict";

// 決定性のため BUILTIN ではなくテスト専用の小辞書を注入する。
const abbr = (match: string, reading: string): DictEntry => ({
  match,
  reading,
  caseSensitive: true,
});
const word = (match: string, reading: string): DictEntry => ({
  match,
  reading,
  caseSensitive: false,
});

const DICT: DictEntry[] = [
  abbr("AI", "エーアイ"),
  abbr("UI", "ユーアイ"),
  abbr("API", "エーピーアイ"),
  abbr("URL", "ユーアールエル"),
  word("Google", "グーグル"),
  word("OpenAI", "オープンエーアイ"),
];

describe("normalizeForTts", () => {
  it("converts an abbreviation next to Japanese", () => {
    expect(normalizeForTts("AIが便利", DICT)).toBe("エーアイが便利");
  });

  it("converts multiple abbreviations", () => {
    expect(normalizeForTts("APIとURL", DICT)).toBe(
      "エーピーアイとユーアールエル",
    );
  });

  it("does not fire inside a longer English word (boundary)", () => {
    expect(normalizeForTts("AIR", DICT)).toBe("AIR");
    expect(normalizeForTts("aid", DICT)).toBe("aid");
    expect(normalizeForTts("MAIL", DICT)).toBe("MAIL");
  });

  it("does not fire when embedded (letter before match)", () => {
    // "OpenAI" は独立エントリで置換されるが、内部の "AI" 単体では発火しない。
    expect(normalizeForTts("OpenAI", DICT)).toBe("オープンエーアイ");
  });

  it("matches general words case-insensitively", () => {
    expect(normalizeForTts("google と Google", DICT)).toBe(
      "グーグル と グーグル",
    );
  });

  it("keeps abbreviations case-sensitive (lowercase stays)", () => {
    expect(normalizeForTts("ai", DICT)).toBe("ai");
  });

  it("prefers the longest match", () => {
    const dict: DictEntry[] = [
      abbr("AI", "エーアイ"),
      word("AI Studio", "エーアイスタジオ"),
    ];
    expect(normalizeForTts("AI Studio が良い", dict)).toBe(
      "エーアイスタジオ が良い",
    );
  });

  it("lets a user entry override the builtin (same match)", () => {
    const dict: DictEntry[] = [abbr("AI", "エーアイ"), abbr("AI", "アイ")];
    // 後勝ちではなく呼び出し側でマージ済みを渡す想定だが、
    // 同一 match が複数あっても壊れず、最初の置換で確定し二重変換しないことを確認。
    const out = normalizeForTts("AI", dict);
    expect(["エーアイ", "アイ"]).toContain(out);
  });

  it("handles Japanese mixed text with punctuation", () => {
    expect(normalizeForTts("AI。次はUI！", DICT)).toBe("エーアイ。次はユーアイ！");
  });

  it("leaves unknown words untouched (no auto alphabet reading)", () => {
    expect(normalizeForTts("XYZ", DICT)).toBe("XYZ");
  });

  it("returns input unchanged for empty text or empty dict", () => {
    expect(normalizeForTts("", DICT)).toBe("");
    expect(normalizeForTts("AI", [])).toBe("AI");
  });
});
