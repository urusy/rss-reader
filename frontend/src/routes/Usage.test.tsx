// 利用状況ページ: サマリー API の結果が各カードに描画されること、
// 空データで案内文が出ることを固定する。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, cleanup } from "@solidjs/testing-library";
import type { UsageSummary } from "@/lib/api";

const getUsageSummary = vi.fn();
vi.mock("@/lib/api", () => ({
  api: { getUsageSummary: (...a: unknown[]) => getUsageSummary(...a) },
}));

import Usage from "./Usage";

const empty: UsageSummary = { buckets: [], llm: [], tts_sources: [] };

const populated: UsageSummary = {
  buckets: [
    { bucket: "2026-07-07T00:00:00Z", feature: "summarize", count: 4 },
    { bucket: "2026-07-07T00:00:00Z", feature: "search", count: 2 },
  ],
  llm: [
    {
      purpose: "summarize",
      model: "claude-sonnet-4-6",
      calls: 2,
      input_tokens: 12345,
      output_tokens: 678,
    },
  ],
  tts_sources: [{ source: "summary", count: 3 }],
};

beforeEach(() => {
  cleanup();
  getUsageSummary.mockReset();
});

describe("Usage ページ", () => {
  it("空データでは案内文を表示する", async () => {
    getUsageSummary.mockResolvedValue(empty);
    render(() => <Usage />);
    expect(await screen.findByText("まだ記録がありません")).toBeTruthy();
  });

  it("機能別利用回数とLLM集計を描画する", async () => {
    getUsageSummary.mockResolvedValue(populated);
    render(() => <Usage />);
    // 機能別（日本語ラベル）。「要約」は機能別と LLM 用途列の両方に出る。
    expect((await screen.findAllByText("要約")).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("検索")).toBeTruthy();
    // LLM 表（モデル・トークン短縮表記）
    expect(screen.getByText("claude-sonnet-4-6")).toBeTruthy();
    expect(screen.getByText("12.3k")).toBeTruthy();
    // 概算コストの表示
    expect(screen.getByText(/概算コスト/)).toBeTruthy();
    // TTS 内訳
    expect(screen.getByText("読み上げの内訳")).toBeTruthy();
  });

  it("期間セレクタの初期値は30日で、bucket=day を要求する", async () => {
    getUsageSummary.mockResolvedValue(empty);
    render(() => <Usage />);
    await screen.findByText("まだ記録がありません");
    expect(getUsageSummary).toHaveBeenCalledWith(30, "day");
  });
});
