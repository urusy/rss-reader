import { createSignal } from "solid-js";

export type Theme = "light" | "dark";

export const STORAGE_KEY = "theme";

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
  if (stored === "light" || stored === "dark") return stored;
  return prefersDark() ? "dark" : "light";
}

/** <html> に dark クラスと color-scheme を反映。副作用のみ。 */
export function applyTheme(t: Theme): void {
  const el = document.documentElement;
  el.classList.toggle("dark", t === "dark");
  el.style.colorScheme = t; // ネイティブ UI（スクロールバー/フォーム/キャンバス）も追従
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

export function toggleTheme(): void {
  setTheme(theme() === "dark" ? "light" : "dark");
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
