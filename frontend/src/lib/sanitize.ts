import DOMPurify from "dompurify";

/**
 * フィード本文の信頼できない HTML を表示用に浄化する。
 * - <style> タグと inline style 属性を除去（prose の体裁を壊す埋め込み CSS・負マージン対策）。
 *   DOMPurify は既定で <style> を許可し CSS を浄化しないため、FORBID で明示的に落とす。
 * - <script>/on* 属性/javascript: URL は DOMPurify 既定で除去（XSS 対策）。
 */
export function sanitizeArticleHtml(html: string): string {
  return DOMPurify.sanitize(html, {
    FORBID_TAGS: ["style"],
    FORBID_ATTR: ["style"],
  });
}
