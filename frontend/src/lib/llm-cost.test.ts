import { describe, it, expect } from "vitest";
import { estimateCostUsd, formatUsd } from "./llm-cost";
import type { LlmUsageRow } from "./api";

const row = (model: string, input: number, output: number): LlmUsageRow => ({
  purpose: "summarize",
  model,
  calls: 1,
  input_tokens: input,
  output_tokens: output,
});

describe("estimateCostUsd（概算。価格表は llm-cost.ts に一元管理）", () => {
  it("既知モデルはトークン数×単価で概算する", () => {
    // claude-sonnet-4-6: $3/MTok in, $15/MTok out（価格表と連動）
    const cost = estimateCostUsd([row("claude-sonnet-4-6", 1_000_000, 100_000)]);
    expect(cost).not.toBeNull();
    expect(cost!.usd).toBeCloseTo(3 + 1.5, 5);
    expect(cost!.hasUnknownModel).toBe(false);
  });

  it("複数行を合算する", () => {
    const cost = estimateCostUsd([
      row("claude-sonnet-4-6", 500_000, 0),
      row("claude-sonnet-4-6", 500_000, 0),
    ]);
    expect(cost!.usd).toBeCloseTo(3, 5);
  });

  it("未知モデルは金額に含めず hasUnknownModel を立てる", () => {
    const cost = estimateCostUsd([
      row("claude-sonnet-4-6", 1_000_000, 0),
      row("some-future-model", 1_000_000, 0),
    ]);
    expect(cost!.usd).toBeCloseTo(3, 5);
    expect(cost!.hasUnknownModel).toBe(true);
  });

  it("空配列は null（カード自体を出さない判断材料）", () => {
    expect(estimateCostUsd([])).toBeNull();
  });

  it("ゼロトークンは $0", () => {
    expect(estimateCostUsd([row("claude-sonnet-4-6", 0, 0)])!.usd).toBe(0);
  });
});

describe("formatUsd", () => {
  it("1ドル未満はセント精度、以上は2桁", () => {
    expect(formatUsd(0.0042)).toBe("$0.0042");
    expect(formatUsd(1.5)).toBe("$1.50");
    expect(formatUsd(12.345)).toBe("$12.35");
  });
});
