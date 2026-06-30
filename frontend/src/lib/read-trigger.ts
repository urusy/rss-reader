// 「少し読んだら既読」の判定ロジック。記事を開いた瞬間ではなく、一定時間の滞在 or
// スクロール操作を「読んだ」シグナルとして扱う。scrolledEnough は純粋関数（vitest 対象）、
// findScrollParent / readScrollMetrics は DOM 依存。

/** 開いてから既読化するまでの滞在時間（ミリ秒）。スクロールが先に成立すればそちらが優先。 */
export const DWELL_MS = 3000;
/** これだけスクロールしたら「読み進めた」とみなす量（px）。長文向け。 */
export const SCROLL_MIN_PX = 150;
/** 表示下端が全体のこの割合に達したら既読とみなす。短文（スクロール余地が小さい）向け。 */
export const SCROLL_MIN_FRACTION = 0.6;

export interface ScrollMetrics {
  scrollTop: number;
  clientHeight: number;
  scrollHeight: number;
}

export interface ScrolledEnoughOpts {
  minPx?: number;
  minFraction?: number;
}

/**
 * スクロール量が「読んだ」とみなせる量に達したか（純粋関数）。
 * - minPx を超えてスクロールした（長文を読み進めた）、または
 * - 表示下端が全体の minFraction 以上に達した（短文を末尾近くまで見た）
 */
export function scrolledEnough(
  m: ScrollMetrics,
  opts: ScrolledEnoughOpts = {},
): boolean {
  const minPx = opts.minPx ?? SCROLL_MIN_PX;
  const minFraction = opts.minFraction ?? SCROLL_MIN_FRACTION;
  if (m.scrollTop > minPx) return true;
  if (m.scrollHeight <= 0) return false;
  return (m.scrollTop + m.clientHeight) / m.scrollHeight >= minFraction;
}

/**
 * el の最近接スクロール可能祖先を返す。該当が無ければ window（モバイルや単体ページ）。
 */
export function findScrollParent(el: Element): HTMLElement | Window {
  let node: HTMLElement | null = el.parentElement;
  while (node) {
    const oy = getComputedStyle(node).overflowY;
    if (
      (oy === "auto" || oy === "scroll") &&
      node.scrollHeight > node.clientHeight
    ) {
      return node;
    }
    node = node.parentElement;
  }
  return window;
}

/** Window / HTMLElement のいずれからもスクロール量メトリクスを読む。 */
export function readScrollMetrics(target: HTMLElement | Window): ScrollMetrics {
  if (target instanceof Window) {
    const doc = document.scrollingElement ?? document.documentElement;
    return {
      scrollTop: doc.scrollTop,
      clientHeight: doc.clientHeight,
      scrollHeight: doc.scrollHeight,
    };
  }
  return {
    scrollTop: target.scrollTop,
    clientHeight: target.clientHeight,
    scrollHeight: target.scrollHeight,
  };
}
