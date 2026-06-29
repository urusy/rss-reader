import { createSignal } from "solid-js";

export type Theme = "light" | "dark" | "graphite" | "sepia";

export const STORAGE_KEY = "theme";

/** 選択肢の順序（toggle の巡回順・セレクトの並び）。 */
export const THEMES: Theme[] = ["light", "dark", "graphite", "sepia"];

export const THEME_LABELS: Record<Theme, string> = {
  light: "ライト",
  dark: "ダーク",
  graphite: "グラファイト",
  sepia: "セピア",
};

/** 暗色系テーマ（color-scheme=dark を当て、dark: ユーティリティを効かせる対象）。 */
const DARK_THEMES: Theme[] = ["dark", "graphite"];

function isTheme(v: unknown): v is Theme {
  return typeof v === "string" && (THEMES as string[]).includes(v);
}

/** prefers-color-scheme: dark か。matchMedia 未実装環境（jsdom 既定）でも安全に false。 */
function prefersDark(): boolean {
  return (
    typeof window !== "undefined" &&
    !!window.matchMedia?.("(prefers-color-scheme: dark)").matches
  );
}

/** localStorage 最優先 → prefers-color-scheme → "light"。副作用なしの純粋関数（呼び出し時に環境を読む）。 */
export function initialTheme(): Theme {
  const stored =
    typeof localStorage !== "undefined" ? localStorage.getItem(STORAGE_KEY) : null;
  if (isTheme(stored)) return stored;
  return prefersDark() ? "dark" : "light";
}

/** <html> にテーマクラスと color-scheme を反映。副作用のみ。light はクラスなし（:root）。 */
export function applyTheme(t: Theme): void {
  const el = document.documentElement;
  el.classList.remove("dark", "graphite", "sepia");
  if (t !== "light") el.classList.add(t);
  // ネイティブ UI（スクロールバー/フォーム/キャンバス）も暗色/明色に追従。
  el.style.colorScheme = DARK_THEMES.includes(t) ? "dark" : "light";
}

// 安価な定数で seed（import 時に matchMedia / localStorage を読まない＝テスト容易・jsdom 安全）。
const [theme, setThemeSignal] = createSignal<Theme>("light");
export { theme };

/** 明示設定: signal 更新 + 永続化 + DOM 反映。ユーザー操作はここを通る。 */
export function setTheme(t: Theme): void {
  setThemeSignal(t);
  localStorage.setItem(STORAGE_KEY, t);
  applyTheme(t);
}

/** THEMES の順に巡回（light→dark→graphite→sepia→light）。 */
export function toggleTheme(): void {
  const i = THEMES.indexOf(theme());
  setTheme(THEMES[(i + 1) % THEMES.length]);
}

/**
 * 起動時に一度だけ呼ぶ（index.tsx の render 前）。
 * 解決済みテーマで signal を seed し DOM へ反映する。
 * localStorage への書き込みはしない（prefers 由来の初期値を勝手に固定しないため）。
 */
export function initTheme(): void {
  const t = initialTheme();
  setThemeSignal(t);
  applyTheme(t);
}
