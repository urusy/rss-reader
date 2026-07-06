// 読み上げの時間表示: Web Speech API は発話時間を返さないため、
// 全体時間は「文字数×想定読速」の初期推定 → 実再生の実測読速で較正する。
// 経過表示は 進捗率 × 推定全体 で導出（バーと常に整合）。
import { describe, it, expect } from "vitest";
import {
  initialTotalSecs,
  estimateTotalSecs,
  formatClock,
} from "./tts-time";

describe("initialTotalSecs", () => {
  it("文字数 ÷ (想定読速 × rate)", () => {
    expect(initialTotalSecs(420, 1)).toBe(60); // 7文字/秒
    expect(initialTotalSecs(420, 2)).toBe(30);
    expect(initialTotalSecs(420, 0.75)).toBe(80);
  });

  it("0文字は0秒", () => {
    expect(initialTotalSecs(0, 1)).toBe(0);
  });
});

describe("estimateTotalSecs", () => {
  const base = { totalChars: 420, rate: 1 };

  it("較正データが無ければ初期推定を返す", () => {
    expect(
      estimateTotalSecs({ ...base, playedSecs: 0, progressDelta: 0 }),
    ).toBe(60);
  });

  it("再生時間・進捗増分が閾値未満なら較正しない（序盤のブレ防止）", () => {
    // 3秒未満
    expect(
      estimateTotalSecs({ ...base, playedSecs: 2, progressDelta: 0.5 }),
    ).toBe(60);
    // 進捗2%未満
    expect(
      estimateTotalSecs({ ...base, playedSecs: 10, progressDelta: 0.01 }),
    ).toBe(60);
  });

  it("閾値を超えたら実測読速で較正する: 全体 = 実再生秒 ÷ 進捗増分", () => {
    // 10秒で10%進んだ → 全体100秒
    expect(
      estimateTotalSecs({ ...base, playedSecs: 10, progressDelta: 0.1 }),
    ).toBe(100);
    // 30秒で50%進んだ → 全体60秒
    expect(
      estimateTotalSecs({ ...base, playedSecs: 30, progressDelta: 0.5 }),
    ).toBe(60);
  });

  it("進捗100%到達後も破綻しない（全体=実再生秒）", () => {
    expect(
      estimateTotalSecs({ ...base, playedSecs: 90, progressDelta: 1 }),
    ).toBe(90);
  });
});

describe("formatClock", () => {
  it("分:秒（秒はゼロ埋め）", () => {
    expect(formatClock(0)).toBe("0:00");
    expect(formatClock(5)).toBe("0:05");
    expect(formatClock(65)).toBe("1:05");
    expect(formatClock(600)).toBe("10:00");
  });

  it("1時間以上は 時:分:秒", () => {
    expect(formatClock(3661)).toBe("1:01:01");
  });

  it("小数は四捨五入、負値は0扱い", () => {
    expect(formatClock(59.6)).toBe("1:00");
    expect(formatClock(-3)).toBe("0:00");
  });
});
