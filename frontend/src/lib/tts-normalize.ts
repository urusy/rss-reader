/**
 * 読み上げ (TTS) テキスト正規化 — 辞書に載った英字略語/英単語をカタカナ読みへ置換する。
 * 純粋関数（辞書は引数注入）。読み上げ直前に本文/要約/翻訳へ一律適用する（ListenBar）。
 *
 * 誤爆対策が肝: 英字が英数字に挟まれている箇所（AIR / OpenAI の内部 AI 等）は置換しない。
 */

import type { DictEntry } from "./tts-dict";

/** 正規表現メタ文字をエスケープ（"C++" / ".NET" 等の記号入り match 向け）。 */
function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * text 内の各エントリ match を reading へ置換して返す。
 *
 * - 単語境界: `\b` は数字/`_` を語構成扱いするため使わず、前後が英数字でない時だけ置換する
 *   ルックアラウンド `(?<![A-Za-z0-9])…(?![A-Za-z0-9])` を使う。日本語・記号・文頭文末に隣接
 *   した英字だけを拾い、英単語内部の部分一致を避ける。
 * - 大小文字: caseSensitive を正規表現フラグ（g / gi）に反映。
 * - 優先順位: match 長の降順に処理し、部分被り（"AI Studio" を "AI" より先に）を防ぐ。
 * - 二重変換なし: reading は非ラテンのカタカナなので、後続エントリのラテン系パターンに
 *   再マッチしない。ゆえに逐次 replace で安全。
 *
 * 性能: 再生ボタン押下時に一度走るだけ。辞書数百語 × 本文数千字でも体感できるコストはない。
 * 将来必要なら「単一の交替正規表現 + コールバック辞書引き」で O(n) 1 パスに畳める。
 */
export function normalizeForTts(text: string, entries: DictEntry[]): string {
  if (!text || entries.length === 0) return text;
  const sorted = [...entries].sort((a, b) => b.match.length - a.match.length);
  let out = text;
  for (const e of sorted) {
    if (!e.match) continue;
    const re = new RegExp(
      `(?<![A-Za-z0-9])${escapeRegExp(e.match)}(?![A-Za-z0-9])`,
      e.caseSensitive ? "g" : "gi",
    );
    out = out.replace(re, e.reading);
  }
  return out;
}
