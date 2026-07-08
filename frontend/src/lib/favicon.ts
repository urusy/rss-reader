//! 記事一覧のソース表示（issue #1）向けの純関数。
//
// favicon は購読済みサイトの /favicon.ico を <img> で直読みする方式（第三者
// サービス不使用・CSP は img-src で外部画像を既に許可）。取得できない場合は
// 頭文字＋自動色の丸アバターにフォールバックするため、その材料もここで作る。

/**
 * 記事 URL のオリジンから favicon の URL を導出する。
 * 例: https://blog.example.com/2026/x → https://blog.example.com/favicon.ico
 * パース不能な URL は null（呼び出し側はアバターにフォールバックする）。
 */
export function faviconUrlFor(articleUrl: string): string | null {
  try {
    return new URL(articleUrl).origin + "/favicon.ico";
  } catch {
    return null;
  }
}

/** ソース名の先頭1文字（大文字化）。空なら "?"。サロゲートペア安全。 */
export function sourceInitial(name: string): string {
  const trimmed = name.trim();
  if (!trimmed) return "?";
  return [...trimmed][0].toUpperCase();
}

/**
 * seed（フィード名等）から決定的に丸アバターの背景色を作る。
 * hue のみ振り、彩度/明度は固定 → 白文字で light/dark 両テーマとも可読。
 */
export function avatarColor(seed: string): string {
  let h = 0;
  for (let i = 0; i < seed.length; i++) {
    h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  }
  return `hsl(${h % 360} 55% 45%)`;
}
