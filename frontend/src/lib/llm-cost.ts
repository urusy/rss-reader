/**
 * LLM 利用の USD 概算コスト。
 *
 * 価格表はこのファイルに一元管理する（価格改定時はここだけ更新して再ビルド）。
 * あくまで概算 — UI では必ず「概算」と明記すること。未知モデルは金額に
 * 含めず hasUnknownModel で知らせる（誤った金額を出すより出さない）。
 * 出典: https://claude.com/pricing#api （2026-07 時点）
 */
import type { LlmUsageRow } from "./api";

interface ModelPrice {
  /** USD per 1M input tokens */
  inPerMTok: number;
  /** USD per 1M output tokens */
  outPerMTok: number;
}

const PRICE_TABLE: Record<string, ModelPrice> = {
  "claude-sonnet-4-6": { inPerMTok: 3, outPerMTok: 15 },
  "claude-sonnet-4-5": { inPerMTok: 3, outPerMTok: 15 },
  "claude-opus-4-6": { inPerMTok: 15, outPerMTok: 75 },
  "claude-opus-4-5": { inPerMTok: 15, outPerMTok: 75 },
  "claude-haiku-4-5": { inPerMTok: 1, outPerMTok: 5 },
  "claude-haiku-3-5": { inPerMTok: 0.8, outPerMTok: 4 },
};

/** モデル ID の日付サフィックス（-20250101 等）を落として価格表を引く。 */
function priceFor(model: string): ModelPrice | undefined {
  if (PRICE_TABLE[model]) return PRICE_TABLE[model];
  const base = model.replace(/-\d{8}$/, "");
  return PRICE_TABLE[base];
}

export interface CostEstimate {
  usd: number;
  /** 価格表にないモデルが混ざっていた（その分は金額に含まれていない） */
  hasUnknownModel: boolean;
}

/** 集計行から USD 概算を合算。空配列は null（表示しない）。 */
export function estimateCostUsd(rows: LlmUsageRow[]): CostEstimate | null {
  if (rows.length === 0) return null;
  let usd = 0;
  let hasUnknownModel = false;
  for (const r of rows) {
    const price = priceFor(r.model);
    if (!price) {
      hasUnknownModel = true;
      continue;
    }
    usd += (r.input_tokens / 1_000_000) * price.inPerMTok;
    usd += (r.output_tokens / 1_000_000) * price.outPerMTok;
  }
  return { usd, hasUnknownModel };
}

/** $0.0042 / $1.50 のような表示。1ドル未満はセント以下まで見せる。 */
export function formatUsd(usd: number): string {
  if (usd > 0 && usd < 1) return `$${usd.toPrecision(2)}`;
  return `$${usd.toFixed(2)}`;
}
