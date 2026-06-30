import { createSignal } from "solid-js";

/** 矢印キー1回あたりの増減幅（px）。 */
const KEYBOARD_STEP = 16;

/** value を [min, max] に収める。純粋関数。 */
export function clampWidth(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/**
 * localStorage の保存幅を読み、数値化して [min, max] にクランプして返す。
 * 未保存・非数値は fallback（これもクランプ）にフォールバック。副作用なしの純粋関数
 * （呼び出し時に環境を読むだけ）。localStorage 不可の環境でも安全。
 */
export function readStoredWidth(
  key: string,
  fallback: number,
  min: number,
  max: number,
): number {
  let raw: string | null = null;
  try {
    raw = typeof localStorage !== "undefined" ? localStorage.getItem(key) : null;
  } catch {
    raw = null;
  }
  const n = raw === null || raw === "" ? NaN : Number.parseInt(raw, 10);
  return Number.isFinite(n)
    ? clampWidth(n, min, max)
    : clampWidth(fallback, min, max);
}

export interface ResizableWidth {
  /** 現在の幅（px）。リアクティブ。 */
  width: () => number;
  /** ハンドルの pointerdown ハンドラ（ドラッグ開始）。 */
  startDrag: (e: PointerEvent) => void;
  /** ハンドルの keydown ハンドラ（矢印/Home/End で増減）。 */
  onKeyDown: (e: KeyboardEvent) => void;
  min: number;
  max: number;
}

/**
 * ペイン幅をドラッグ/キーボードで調節し localStorage に永続化する。
 * 戻り値の `width()` を CSS 変数（grid-template-columns）に流し込む使い方を想定。
 */
export function createResizableWidth(opts: {
  storageKey: string;
  defaultWidth: number;
  min: number;
  max: number;
}): ResizableWidth {
  const { storageKey, defaultWidth, min, max } = opts;
  const [width, setWidth] = createSignal(
    readStoredWidth(storageKey, defaultWidth, min, max),
  );

  const persist = (w: number) => {
    try {
      localStorage.setItem(storageKey, String(Math.round(w)));
    } catch {
      /* プライベートモード等で localStorage 不可なら黙って無視 */
    }
  };

  const startDrag = (e: PointerEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = width();
    // ドラッグ中はテキスト選択と全体カーソルを抑止して操作感を整える。
    document.body.style.userSelect = "none";
    document.body.style.cursor = "col-resize";

    const onMove = (ev: PointerEvent) => {
      setWidth(clampWidth(startW + (ev.clientX - startX), min, max));
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      document.body.style.userSelect = "";
      document.body.style.cursor = "";
      persist(width());
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    let next: number | null = null;
    if (e.key === "ArrowLeft") next = width() - KEYBOARD_STEP;
    else if (e.key === "ArrowRight") next = width() + KEYBOARD_STEP;
    else if (e.key === "Home") next = min;
    else if (e.key === "End") next = max;
    if (next === null) return;
    e.preventDefault();
    const clamped = clampWidth(next, min, max);
    setWidth(clamped);
    persist(clamped);
  };

  return { width, startDrag, onKeyDown, min, max };
}
