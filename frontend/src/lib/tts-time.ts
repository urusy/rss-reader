/**
 * 読み上げの時間表示（経過 / 全体）のための推定ロジック。
 *
 * Web Speech API は発話の所要時間を事前に返さないため、全体時間は推定するしかない:
 * - 初期推定: 文字数 ÷ (想定読速 CHARS_PER_SEC × rate) — 進捗補間と同じ想定値
 * - 較正: 実再生がある程度進んだら 全体 = 実再生秒 ÷ 進捗増分 に切り替える。
 *   声ごとの実効速度差・記事の漢字率・チャンク間ポーズを自動で織り込む。
 * 経過表示は「進捗率 × 推定全体」で導出し、プログレスバーと常に整合させる
 * （保存位置からの再開でも経過の実測が無くて破綻しない）。
 */
import { CHARS_PER_SEC } from "./tts";

/** 較正に必要な最小実再生秒（序盤のブレで全体が暴れるのを防ぐ）。 */
const CALIBRATE_MIN_SECS = 3;
/** 較正に必要な最小進捗増分。 */
const CALIBRATE_MIN_DELTA = 0.02;

/** 初期推定: 文字数 ÷ (想定読速 × rate)。 */
export function initialTotalSecs(totalChars: number, rate: number): number {
  if (totalChars <= 0 || rate <= 0) return 0;
  return totalChars / (CHARS_PER_SEC * rate);
}

/**
 * 全体秒数の推定。実再生 playedSecs 秒で進捗が progressDelta 進んだ実測が
 * 閾値を超えていれば較正値（playedSecs ÷ progressDelta）、それまでは初期推定。
 */
export function estimateTotalSecs(args: {
  totalChars: number;
  rate: number;
  /** このセッションで実際に再生した秒数（一時停止は含めない）。 */
  playedSecs: number;
  /** 同セッションで進んだ進捗率（0..1）。 */
  progressDelta: number;
}): number {
  const { totalChars, rate, playedSecs, progressDelta } = args;
  if (playedSecs >= CALIBRATE_MIN_SECS && progressDelta >= CALIBRATE_MIN_DELTA) {
    return playedSecs / progressDelta;
  }
  return initialTotalSecs(totalChars, rate);
}

/** 秒 → "M:SS"（1時間以上は "H:MM:SS"）。小数は四捨五入・負値は0。 */
export function formatClock(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(sec)}` : `${m}:${pad(sec)}`;
}
