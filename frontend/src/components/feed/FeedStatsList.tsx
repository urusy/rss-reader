import { createMemo, createResource, For, Show } from "solid-js";
import { api, type FeedOverview } from "@/lib/api";
import { lastPostLabel, postsPerWeekLabel } from "@/lib/format";

/**
 * フィード別の「最終投稿 / 投稿頻度」一覧。
 * 自前で listFeeds() と listFeedOverview() を取得し feed.id で突合するため、
 * 記事一覧画面や機能01の /manage に一切依存しない（自己完結）。
 * 機能01 着地後は、この行レンダリングを FeedManage の各行へ移設して再利用する。
 */
export default function FeedStatsList() {
  const [feeds] = createResource(() => api.listFeeds());
  const [overview] = createResource(() => api.listFeedOverview());

  const byId = createMemo(
    () =>
      new Map<string, FeedOverview>(
        (overview() ?? []).map((o) => [o.feed_id, o] as const),
      ),
  );

  return (
    <Show
      when={!feeds.loading}
      fallback={<p class="text-sm text-muted-foreground">読み込み中…</p>}
    >
      <Show
        when={(feeds()?.length ?? 0) > 0}
        fallback={
          <p class="text-sm text-muted-foreground">フィードがありません。</p>
        }
      >
        <ul class="divide-y divide-border">
          <For each={feeds()}>
            {(feed) => {
              const o = () => byId().get(feed.id);
              return (
                <li class="flex items-center justify-between gap-3 py-3">
                  <span class="text-sm font-medium min-w-0 truncate">
                    {feed.title ?? feed.url}
                  </span>
                  <span class="text-xs text-muted-foreground whitespace-nowrap">
                    {lastPostLabel(o()?.last_published_at ?? null)} ・{" "}
                    {postsPerWeekLabel(o()?.posts_per_week ?? 0)}
                  </span>
                </li>
              );
            }}
          </For>
        </ul>
      </Show>
    </Show>
  );
}
