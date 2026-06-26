/**
 * ISO 8601 → 「YYYY/MM/DD」。タイムゾーンを Asia/Tokyo に固定し、実行環境の TZ に
 * 依存しない決定的な出力にする（単一ユーザ=JST 前提）。不正な ISO は空文字。
 */
const dateFormatter = new Intl.DateTimeFormat("ja-JP", {
  timeZone: "Asia/Tokyo",
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
});

export function formatDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return dateFormatter.format(d); // 例: "2026/06/26"
}

/** ISO日時 → 「投稿なし / 今日 / 昨日 / N日前」 */
export function lastPostLabel(iso: string | null, now: Date = new Date()): string {
  if (!iso) return "投稿なし";
  const then = new Date(iso).getTime();
  const days = Math.floor((now.getTime() - then) / 86_400_000);
  if (days <= 0) return "今日";
  if (days === 1) return "昨日";
  return `${days}日前`;
}

/** 週あたり本数 → 「投稿なし / 週Y件」（小数1桁、末尾.0は省く） */
export function postsPerWeekLabel(n: number): string {
  if (!n || n <= 0) return "投稿なし";
  const rounded = Math.round(n * 10) / 10; // backend で丸め済みだが念のため冪等に
  const text = Number.isInteger(rounded) ? String(rounded) : rounded.toFixed(1);
  return `週${text}件`;
}
