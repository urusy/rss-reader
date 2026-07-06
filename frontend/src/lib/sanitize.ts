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
  const kept = value
    .split(";")
    .map((decl) => decl.trim())
    .filter(Boolean)
    .filter((decl) => {
      const i = decl.indexOf(":");
      if (i < 0) return false;
      return SAFE_STYLE_PROPS.has(decl.slice(0, i).trim().toLowerCase());
    });
  return ensureReadableTextColor(kept).join("; ");
}

// 補完する対比色。テーマの前景色ではなく固定値（背景も inline の固定色なので、
// テーマに追従させるとかえって明るい背景 × 白文字の事故が再発する）。
const DARK_TEXT = "#1f2937";
const LIGHT_TEXT = "#f5f5f5";

/**
 * 背景色だけ指定して文字色を指定しない要素は、テーマの文字色（ダークでは白）を
 * 継承して「明るい背景 × 白文字」で読めなくなる（Google Testing Blog の
 * 色分けコード表で実害）。背景の明度から対比色を補完する。
 * 背景色を解釈できない場合（named color 等）は何もしない。
 */
function ensureReadableTextColor(decls: string[]): string[] {
  const get = (prop: string) =>
    decls.find((d) => d.slice(0, d.indexOf(":")).trim().toLowerCase() === prop);
  const bg = get("background-color");
  if (!bg || get("color")) return decls;
  const lum = relativeLuminance(bg.slice(bg.indexOf(":") + 1).trim());
  if (lum === null) return decls;
  return [...decls, `color: ${lum > 0.5 ? DARK_TEXT : LIGHT_TEXT}`];
}

/** #rgb / #rrggbb / rgb(r,g,b) / rgba(r,g,b,a) を 0..1 の輝度へ。解釈不能は null。 */
function relativeLuminance(cssColor: string): number | null {
  let r: number, g: number, b: number;
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(cssColor);
  const rgb = /^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*(?:,\s*([\d.]+)\s*)?\)$/i.exec(
    cssColor,
  );
  if (hex) {
    const h = hex[1];
    const full = h.length === 3 ? h.split("").map((c) => c + c).join("") : h;
    r = parseInt(full.slice(0, 2), 16);
    g = parseInt(full.slice(2, 4), 16);
    b = parseInt(full.slice(4, 6), 16);
  } else if (rgb) {
    // ほぼ透明な背景は下地が透けるので判定しない。
    if (rgb[4] !== undefined && parseFloat(rgb[4]) < 0.5) return null;
    [r, g, b] = [Number(rgb[1]), Number(rgb[2]), Number(rgb[3])];
  } else {
    return null;
  }
  return (0.299 * r + 0.587 * g + 0.114 * b) / 255;
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

  // リンク: DOMPurify は既定で target を落とすため、フィード内リンクが
  // リーダー SPA と同じタブで開いてしまう。href 持ちの <a> は新規タブ +
  // rel="noopener noreferrer"（reverse tabnabbing 対策、監査 LOW）に統一する
  // （DOMPurify 公式ドキュメントのレシピ）。
  DOMPurify.addHook("afterSanitizeAttributes", (node) => {
    const el = node as Element;
    if (el.tagName === "A" && el.hasAttribute("href")) {
      el.setAttribute("target", "_blank");
      el.setAttribute("rel", "noopener noreferrer");
    }
  });
}

export function sanitizeArticleHtml(html: string): string {
  ensureHooks();
  return DOMPurify.sanitize(html, {
    FORBID_TAGS: ["style"],
  });
}
