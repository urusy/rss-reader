import { createMemo, createSignal, For, Show } from "solid-js";
import { A } from "@solidjs/router";
import { useApp } from "@/lib/store";
import type { Feed } from "@/lib/api";

const navItem =
  "block h-8 px-2 rounded-md text-sm leading-8 hover:bg-accent truncate";
const navActive = "bg-accent text-accent-foreground";

/**
 * フォルダ→フィードのツリー（機能02のUI）。未分類は末尾固定の仮想グループ。
 * 表示・ナビゲーションのみ。フォルダ/フィードの管理操作は /manage（機能01）。
 */
export default function FeedTree(props: { onNavigate?: () => void }) {
  const app = useApp();
  const [collapsed, setCollapsed] = createSignal<Record<string, boolean>>({});

  const grouped = createMemo(() => {
    const map = new Map<string, Feed[]>();
    const unclassified: Feed[] = [];
    for (const f of app.feeds() ?? []) {
      if (f.folder_id) {
        const arr = map.get(f.folder_id) ?? [];
        arr.push(f);
        map.set(f.folder_id, arr);
      } else {
        unclassified.push(f);
      }
    }
    return { map, unclassified };
  });

  const toggle = (id: string) => setCollapsed((c) => ({ ...c, [id]: !c[id] }));
  const go = () => props.onNavigate?.();

  return (
    <nav class="space-y-0.5">
      <For each={app.folders()}>
        {(folder) => (
          <div>
            <div class="flex items-center">
              <button
                type="button"
                class="flex h-8 w-5 items-center justify-center text-muted-foreground"
                onClick={() => toggle(folder.id)}
                aria-label={collapsed()[folder.id] ? "展開" : "折りたたみ"}
              >
                {collapsed()[folder.id] ? "▸" : "▾"}
              </button>
              <A
                href={`/folders/${folder.id}`}
                class={`${navItem} flex-1 font-medium`}
                activeClass={navActive}
                onClick={go}
              >
                {folder.name}
              </A>
            </div>
            <Show when={!collapsed()[folder.id]}>
              <div class="ml-5 space-y-0.5">
                <For each={grouped().map.get(folder.id) ?? []}>
                  {(f) => (
                    <A
                      href={`/feeds/${f.id}`}
                      class={navItem}
                      activeClass={navActive}
                      onClick={go}
                    >
                      {f.title ?? f.url}
                    </A>
                  )}
                </For>
              </div>
            </Show>
          </div>
        )}
      </For>

      <Show when={grouped().unclassified.length > 0}>
        <div>
          <A
            href="/folders/unclassified"
            class={`${navItem} font-medium text-muted-foreground`}
            activeClass={navActive}
            onClick={go}
          >
            未分類
          </A>
          <div class="ml-5 space-y-0.5">
            <For each={grouped().unclassified}>
              {(f) => (
                <A
                  href={`/feeds/${f.id}`}
                  class={navItem}
                  activeClass={navActive}
                  onClick={go}
                >
                  {f.title ?? f.url}
                </A>
              )}
            </For>
          </div>
        </div>
      </Show>
    </nav>
  );
}
