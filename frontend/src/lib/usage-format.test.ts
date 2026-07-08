import { describe, it, expect } from "vitest";
import {
  fillBuckets,
  featureLabel,
  formatTokens,
  cacheHitRate,
  totalsByFeature,
  bucketForDays,
} from "./usage-format";
import type { UsageBucket, LlmUsageRow } from "./api";

const bucket = (iso: string, feature: string, count: number): UsageBucket => ({
  bucket: iso,
  feature,
  count,
});

describe("fillBuckets（欠損バケットのゼロ埋め・日次）", () => {
  const now = new Date("2026-07-07T12:00:00+09:00");

  it("空データでも days 個のゼロバケットを返す", () => {
    const out = fillBuckets([], 3, "day", now);
    expect(out).toHaveLength(3);
    expect(out.every((b) => b.total === 0)).toBe(true);
  });

  it("あるバケットの合計は全 feature の合算になる", () => {
    const rows = [
      bucket("2026-07-07T00:00:00+09:00", "summarize", 2),
      bucket("2026-07-07T00:00:00+09:00", "search", 3),
      bucket("2026-07-06T00:00:00+09:00", "search", 1),
    ];
    const out = fillBuckets(rows, 3, "day", now);
    expect(out).toHaveLength(3);
    // 末尾が最新（今日）。
    expect(out[2].total).toBe(5);
    expect(out[1].total).toBe(1);
    expect(out[0].total).toBe(0);
  });

  it("週バケットは週数分返る", () => {
    const out = fillBuckets([], 28, "week", now);
    expect(out.length).toBeGreaterThanOrEqual(4);
    expect(out.length).toBeLessThanOrEqual(5);
  });
});

describe("featureLabel", () => {
  it("既知キーは日本語ラベル", () => {
    expect(featureLabel("summarize")).toBe("要約");
    expect(featureLabel("mark_read")).toBe("既読化");
    expect(featureLabel("tts_play")).toBe("読み上げ");
  });
  it("未知キーはそのまま返す（新キー追加時に UI が壊れない）", () => {
    expect(featureLabel("future_feature")).toBe("future_feature");
  });
});

describe("formatTokens", () => {
  it("1000未満はそのまま", () => {
    expect(formatTokens(0)).toBe("0");
    expect(formatTokens(999)).toBe("999");
  });
  it("千単位は k、百万単位は M", () => {
    expect(formatTokens(12345)).toBe("12.3k");
    expect(formatTokens(1_500_000)).toBe("1.5M");
  });
});

describe("cacheHitRate（要約/翻訳のキャッシュ節約率）", () => {
  const llm = (purpose: string, calls: number): LlmUsageRow => ({
    purpose,
    model: "m",
    calls,
    input_tokens: 0,
    output_tokens: 0,
  });

  it("HTTP 10回・実呼び出し4回 → 60%", () => {
    const buckets = [
      bucket("2026-07-07T00:00:00Z", "summarize", 6),
      bucket("2026-07-06T00:00:00Z", "translate", 4),
    ];
    expect(cacheHitRate(buckets, [llm("summarize", 2), llm("translate", 2)])).toBe(60);
  });

  it("要求ゼロなら null（表示しない）", () => {
    expect(cacheHitRate([], [])).toBeNull();
  });

  it("実呼び出しが要求を上回る異常系（背景実行等）は 0% に丸める", () => {
    const buckets = [bucket("2026-07-07T00:00:00Z", "summarize", 1)];
    expect(cacheHitRate(buckets, [llm("summarize", 5)])).toBe(0);
  });
});

describe("totalsByFeature（機能別合計・降順）", () => {
  it("複数バケットを合算し降順に並べる", () => {
    const rows = [
      bucket("2026-07-07T00:00:00Z", "search", 1),
      bucket("2026-07-06T00:00:00Z", "search", 2),
      bucket("2026-07-06T00:00:00Z", "summarize", 5),
    ];
    expect(totalsByFeature(rows)).toEqual([
      { feature: "summarize", count: 5 },
      { feature: "search", count: 3 },
    ]);
  });
});

describe("bucketForDays（期間→バケット単位の自動対応）", () => {
  it("短期間は day、90日は week、年は month", () => {
    expect(bucketForDays(7)).toBe("day");
    expect(bucketForDays(30)).toBe("day");
    expect(bucketForDays(90)).toBe("week");
    expect(bucketForDays(365)).toBe("month");
  });
});
