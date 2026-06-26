import { createResource, createSignal, For, Show } from "solid-js";
import { A } from "@solidjs/router";
import { api } from "@/lib/api";
import { useSelection } from "@/lib/selection";
import { useApp } from "@/lib/store";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { formatDate } from "@/lib/format";
import { cn } from "@/lib/utils";

/**
 * 右ペインの記事一覧。URL（useSelection）と store.filter から listArticles の
 * 引数を組み立てて自動再フェッチする。all / feed / folder / 未分類 の全 scope に対応。
 */
export default function ArticleList() {
  const scope = useSelection();
  const app = useApp();

  const params = () => {
    const s = scope();
    const unread = app.state.filter === "unread" ? true : undefined;
    if (s.kind === "feed") return { feed_id: s.feedId, unread };
    if (s.kind === "folder")
      return s.folderId === "unclassified"
        ? { unclassified: true, unread }
        : { folder_id: s.folderId, unread };
    return { unread };
  };

  const [articles, { refetch }] = createResource(params, (p) =>
    api.listArticles(p),
  );
  const [stats, { refetch: refetchStats }] = createResource(() =>
    api.getStats(),
  );
  const [marking, setMarking] = createSignal(false);

  const markAll = async () => {
    setMarking(true);
    try {
      const s = scope();
      await api.markAllRead(
        s.kind === "feed" ? { feed_id: s.feedId } : undefined,
      );
      await Promise.all([refetch(), refetchStats()]);
    } catch (e) {
      alert(`既読化に失敗しました: ${String(e)}`);
    } finally {
      setMarking(false);
    }
  };

  return (
    <div class="space-y-4">
      <div class="flex items-center justify-between gap-2">
        <Badge variant={(stats()?.unread ?? 0) > 0 ? "unread" : "default"}>
          未読 {stats()?.unread ?? 0} 件
        </Badge>
        <Button
          size="sm"
          variant="outline"
          onClick={markAll}
          disabled={marking() || (articles()?.length ?? 0) === 0}
        >
          {marking()
            ? "既読化中…"
            : scope().kind === "feed"
              ? "このフィードを既読"
              : "すべて既読"}
        </Button>
      </div>

      <Show
        when={!articles.loading}
        fallback={<p class="text-sm text-muted-foreground">読み込み中…</p>}
      >
        <Show
          when={(articles()?.length ?? 0) > 0}
          fallback={<p class="text-sm text-muted-foreground">記事がありません。</p>}
        >
          <div class="divide-y divide-border">
            <For each={articles()}>
              {(a) => (
                <A
                  href={`/articles/${a.id}`}
                  class="-mx-2 block rounded-md px-2 py-3 transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                  <p
                    class={cn(
                      "line-clamp-2 text-sm",
                      a.is_read
                        ? "font-normal text-muted-foreground"
                        : "font-semibold text-foreground",
                    )}
                  >
                    {a.title}
                  </p>
                  <Show when={a.summary}>
                    <p class="mt-0.5 line-clamp-1 text-sm text-muted-foreground">
                      {a.summary}
                    </p>
                  </Show>
                  <Show when={a.published_at}>
                    <p class="mt-1 text-xs text-muted-foreground">
                      {formatDate(a.published_at!)}
                    </p>
                  </Show>
                </A>
              )}
            </For>
          </div>
        </Show>
      </Show>
    </div>
  );
}
