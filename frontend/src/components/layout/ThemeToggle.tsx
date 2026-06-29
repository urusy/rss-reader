import { For } from "solid-js";
import { theme, setTheme, THEMES, THEME_LABELS, type Theme } from "@/lib/theme";

/** テーマ選択（ライト/ダーク/グラファイト/セピア）。状態は lib/theme の signal。 */
export function ThemeToggle() {
  return (
    <label class="flex items-center gap-2 text-sm">
      <span class="text-muted-foreground">テーマ</span>
      <select
        class="h-8 flex-1 rounded-md border border-input bg-background px-2 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        value={theme()}
        onChange={(e) => setTheme(e.currentTarget.value as Theme)}
      >
        <For each={THEMES}>
          {(t) => <option value={t}>{THEME_LABELS[t]}</option>}
        </For>
      </select>
    </label>
  );
}
