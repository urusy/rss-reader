import { For, Show } from "solid-js";
import { useApp } from "@/lib/store";

const ROWS: [string, string][] = [
  ["j / k", "次の記事 / 前の記事"],
  ["Enter", "記事を開く"],
  ["m", "既読にする"],
  ["o", "原文を新しいタブで開く"],
  ["/", "検索"],
  ["g", "一覧へ戻る"],
  ["?", "このヘルプ"],
];

export default function KeyboardHelp() {
  const app = useApp();
  return (
    <Show when={app.state.helpOpen}>
      <div
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
        onClick={() => app.closeHelp()}
      >
        <div
          class="w-full max-w-sm rounded-lg border border-border bg-background p-5 text-foreground shadow-lg"
          onClick={(e) => e.stopPropagation()}
        >
          <h2 class="mb-3 text-sm font-semibold">キーボードショートカット</h2>
          <ul class="space-y-2">
            <For each={ROWS}>
              {([key, desc]) => (
                <li class="flex items-center justify-between gap-4 text-sm">
                  <kbd class="rounded border border-border bg-muted px-1.5 py-0.5 text-xs">
                    {key}
                  </kbd>
                  <span class="text-muted-foreground">{desc}</span>
                </li>
              )}
            </For>
          </ul>
        </div>
      </div>
    </Show>
  );
}
