/** 検索クエリの純粋ヘルパ（vitest 対象）。UI から URL 組み立てを切り出す。 */

/** 入力を trim する。前後の空白は検索意図に含めない。 */
export function normalizeQuery(raw: string): string {
  return raw.trim();
}

/**
 * 検索結果ページへの href を組み立てる。trim 後に空なら null
 * （= 遷移しない。空クエリで全件 %% スキャンを投げない）。
 */
export function searchHref(raw: string): string | null {
  const q = normalizeQuery(raw);
  if (q === "") return null;
  return `/search?q=${encodeURIComponent(q)}`;
}
