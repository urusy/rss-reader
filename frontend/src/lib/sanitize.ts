import DOMPurify from "dompurify";

/**
 * フィード本文の信頼できない HTML を表示用に浄化する。
 * - <style> タグは除去（ページ全体に効く埋め込み CSS は危険）。
 * - inline `style` 属性は「安全なタイポグラフィ系プロパティだけ」に絞って残す。
 *   全除去すると、コードをインライン style（等幅フォント・色・空白維持）だけで表す
 *   フィード（Blogger 等）でコードの体裁が完全に失われるため。margin/position/width/
 *   display など**レイアウトを破壊しうるプロパティは落とす**（負マージン対策の趣旨は維持）。
 * - monospace 指定の要素には `feed-mono` クラスを付け、CSS 側で行間の間延びを詰められるようにする。
 * - <script>/on* 属性/javascript: URL は DOMPurify 既定で除去（XSS 対策）。
 */

// inline style で残す安全なプロパティ（見た目のみ・レイアウトに影響しない）。
const SAFE_STYLE_PROPS = new Set([
  "color",
  "background-color",
  "font-family",
  "font-style",
  "font-weight",
  "font-variant",
  "text-decoration",
  "text-decoration-line",
  "white-space",
]);

// font-family がコード用（等幅）かの判定。'Roboto Mono' 等の固有名も拾う。
const MONO_RE = /font-family\s*:[^;]*(monospace|mono|courier|consolas|menlo|monaco|source\s*code)/i;

function filterInlineStyle(value: string): string {
  return value
    .split(";")
    .map((decl) => decl.trim())
    .filter(Boolean)
    .filter((decl) => {
      const i = decl.indexOf(":");
      if (i < 0) return false;
      return SAFE_STYLE_PROPS.has(decl.slice(0, i).trim().toLowerCase());
    })
    .join("; ");
}

let hooksRegistered = false;
function ensureHooks(): void {
  if (hooksRegistered) return;
  hooksRegistered = true;

  // 要素単位: 元の style が等幅フォント指定なら feed-mono を付ける（style 絞り込みの前に読む）。
  DOMPurify.addHook("uponSanitizeElement", (node) => {
    const el = node as Element;
    const style = el.getAttribute?.("style");
    if (style && MONO_RE.test(style)) el.classList.add("feed-mono");
  });

  // 属性単位: style を安全プロパティのみに絞る。空になれば属性ごと落とす。
  DOMPurify.addHook("uponSanitizeAttribute", (_node, data) => {
    if (data.attrName !== "style") return;
    const filtered = filterInlineStyle(data.attrValue);
    data.attrValue = filtered;
    if (!filtered) data.keepAttr = false;
  });
}

export function sanitizeArticleHtml(html: string): string {
  ensureHooks();
  return DOMPurify.sanitize(html, {
    FORBID_TAGS: ["style"],
  });
}
