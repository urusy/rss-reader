/**
 * 利用状況サマリーの表示用整形（純関数のみ）。
 * データ取得は api.getUsageSummary、描画は routes/Usage.tsx が担う。
 */
import type { UsageBucket, LlmUsageRow } from "./api";

export type BucketUnit = "day" | "week" | "month";

/** 期間セレクタの日数 → サーバーへ渡すバケット単位。 */
export function bucketForDays(days: number): BucketUnit {
  if (days <= 31) return "day";
  if (days <= 92) return "week";
  return "month";
}

const MS_PER_DAY = 24 * 60 * 60 * 1000;

export interface FilledBucket {
  /** バケット先頭（ローカル=JST の日/週/月境界） */
  start: Date;
  total: number;
}

/**
 * サーバーの疎な時系列（イベントがあったバケットのみ）を、期間全体の
 * 連続バケット列にゼロ埋めして返す。末尾が最新。バーグラフの X 軸を
 * 欠損なく描くための整形で、feature 別の内訳は totalsByFeature が担う。
 *
 * バケットの照合はローカルタイム（JST）の暦境界で行う。サーバーの
 * date_trunc は UTC 境界なので日付またぎで最大9時間ずれるが、
 * 「アクティビティの俯瞰」という用途では許容する（既読化の正確な
 * 監査ログではない）。
 */
export function fillBuckets(
  rows: UsageBucket[],
  days: number,
  unit: BucketUnit,
  now: Date = new Date(),
): FilledBucket[] {
  const starts = bucketStarts(days, unit, now);
  const totals = new Map<number, number>();
  for (const r of rows) {
    const t = new Date(r.bucket).getTime();
    if (Number.isNaN(t)) continue;
    // 属するバケット = start <= t な最後の start（二分せず線形で十分な件数）。
    for (let i = starts.length - 1; i >= 0; i--) {
      if (t >= starts[i].getTime()) {
        totals.set(i, (totals.get(i) ?? 0) + r.count);
        break;
      }
    }
  }
  return starts.map((start, i) => ({ start, total: totals.get(i) ?? 0 }));
}

/** 期間内の連続バケット先頭列（昇順）。 */
function bucketStarts(days: number, unit: BucketUnit, now: Date): Date[] {
  const out: Date[] = [];
  if (unit === "day") {
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    for (let i = days - 1; i >= 0; i--) {
      out.push(new Date(today.getTime() - i * MS_PER_DAY));
    }
  } else if (unit === "week") {
    // 週の頭は月曜（ISO 週、date_trunc('week') と一致）。
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const dow = (today.getDay() + 6) % 7; // Mon=0
    const thisWeek = new Date(today.getTime() - dow * MS_PER_DAY);
    const weeks = Math.ceil(days / 7);
    for (let i = weeks; i >= 0; i--) {
      const start = new Date(thisWeek.getTime() - i * 7 * MS_PER_DAY);
      // 期間より前の週は捨てる（先頭週は部分的に重なれば含める）。
      if (start.getTime() + 7 * MS_PER_DAY > now.getTime() - days * MS_PER_DAY) {
        out.push(start);
      }
    }
  } else {
    const months = Math.ceil(days / 30);
    for (let i = months; i >= 0; i--) {
      out.push(new Date(now.getFullYear(), now.getMonth() - i, 1));
    }
  }
  return out;
}

/** 機能別合計（全期間合算・降順）。横バーリスト用。 */
export function totalsByFeature(rows: UsageBucket[]): { feature: string; count: number }[] {
  const totals = new Map<string, number>();
  for (const r of rows) {
    totals.set(r.feature, (totals.get(r.feature) ?? 0) + r.count);
  }
  return [...totals.entries()]
    .map(([feature, count]) => ({ feature, count }))
    .sort((a, b) => b.count - a.count || a.feature.localeCompare(b.feature));
}

/** 機能キー → 日本語ラベル。未知キーはそのまま（新キーで UI が壊れない）。 */
const FEATURE_LABELS: Record<string, string> = {
  mark_read: "既読化",
  mark_read_all: "一括既読",
  summarize: "要約",
  summary_delete: "要約の削除",
  translate: "翻訳",
  translation_delete: "翻訳の削除",
  ask: "Ask Claude",
  extract: "本文抽出",
  search: "検索",
  star: "スター",
  highlight: "ハイライト",
  tag_assign: "タグ付け",
  tag_suggest: "タグ提案",
  read_later: "後で読む",
  feed_add: "フィード追加",
  feed_refresh: "フィード再取得",
  feed_delete: "フィード削除",
  feed_discover: "フィード検出",
  opml_import: "OPML インポート",
  opml_export: "OPML エクスポート",
  digest_view: "ダイジェスト閲覧",
  digest_refresh: "ダイジェスト生成",
  clusters_view: "クラスタ閲覧",
  recluster: "再クラスタリング",
  cluster_summary_req: "クラスタ要約",
  relevance_score: "関連度スコア",
  backup_export: "バックアップ出力",
  backup_import: "バックアップ取込",
  tts_play: "読み上げ",
};

export function featureLabel(key: string): string {
  return FEATURE_LABELS[key] ?? key;
}

/** LLM purpose → 日本語ラベル（llm_usage_events 側）。 */
const PURPOSE_LABELS: Record<string, string> = {
  summarize: "要約",
  translate: "翻訳",
  chat: "Ask Claude",
  suggest_tags: "タグ提案",
  digest: "ダイジェスト",
  score_relevance: "関連度スコア",
  cluster_summary: "クラスタ要約",
};

export function purposeLabel(key: string): string {
  return PURPOSE_LABELS[key] ?? key;
}

/** TTS 読み上げ対象 → 日本語ラベル。 */
const TTS_SOURCE_LABELS: Record<string, string> = {
  content: "本文",
  summary: "要約",
  translation: "翻訳",
  unknown: "不明",
};

export function ttsSourceLabel(key: string): string {
  return TTS_SOURCE_LABELS[key] ?? key;
}

/** トークン数の短縮表記: 999 → "999" / 12345 → "12.3k" / 1.5M。 */
export function formatTokens(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${trimZero((n / 1000).toFixed(1))}k`;
  return `${trimZero((n / 1_000_000).toFixed(1))}M`;
}

function trimZero(s: string): string {
  return s.endsWith(".0") ? s.slice(0, -2) : s;
}

/**
 * 要約/翻訳のキャッシュ節約率（%）。
 * HTTP 要求件数（summarize/translate、キャッシュヒット込み）と
 * LLM 実呼び出し件数の差分から算出。要求ゼロなら null（非表示）。
 * 背景実行などで実呼び出しが要求を上回る場合は 0 に丸める。
 */
export function cacheHitRate(buckets: UsageBucket[], llm: LlmUsageRow[]): number | null {
  const cached = ["summarize", "translate"];
  const requested = buckets
    .filter((b) => cached.includes(b.feature))
    .reduce((sum, b) => sum + b.count, 0);
  if (requested === 0) return null;
  const actual = llm
    .filter((r) => cached.includes(r.purpose))
    .reduce((sum, r) => sum + r.calls, 0);
  const rate = Math.round(((requested - actual) / requested) * 100);
  return Math.max(0, rate);
}
