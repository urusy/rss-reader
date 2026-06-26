import { createResource, createSignal, For, Show } from "solid-js";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import FeedStatsList from "@/components/feed/FeedStatsList";

export default function FeedList() {
  const [articles, { refetch }] = createResource(() => api.listArticles());
  const [stats, { refetch: refetchStats }] = createResource(() => api.getStats());
  const [url, setUrl] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [marking, setMarking] = createSignal(false);

  const markAll = async () => {
    setMarking(true);
    try {
      await api.markAllRead();
      await Promise.all([refetch(), refetchStats()]);
    } catch (e) {
      alert(`既読化に失敗しました: ${String(e)}`);
    } finally {
      setMarking(false);
    }
  };

  const addFeed = async () => {
    const value = url().trim();
    if (!value) return;
    setBusy(true);
    try {
      await api.addFeed(value);
      setUrl("");
      await refetch();
    } catch (e) {
      alert(`追加に失敗しました: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="space-y-6">
      <div class="flex items-center justify-between gap-2">
        <Badge variant={(stats()?.unread ?? 0) > 0 ? "unread" : "default"}>
          未読 {stats()?.unread ?? 0} 件
        </Badge>
        <Button
          size="sm"
          variant="outline"
          onClick={markAll}
          disabled={marking() || (stats()?.unread ?? 0) === 0}
        >
          {marking() ? "既読化中…" : "すべて既読"}
        </Button>
      </div>

      <div class="flex gap-2">
        <input
          class="flex-1 h-9 rounded-md border border-input bg-background px-3 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          placeholder="https://example.com/feed.xml"
          value={url()}
          onInput={(e) => setUrl(e.currentTarget.value)}
          onKeyDown={(e) => e.key === "Enter" && addFeed()}
        />
        <Button onClick={addFeed} disabled={busy()}>
          {busy() ? "追加中…" : "フィード追加"}
        </Button>
      </div>

      {/* 機能03: フィード別の最終投稿/投稿頻度。機能01の /manage 着地後に移設する暫定マウント。 */}
      <section class="rounded-lg border border-border p-4">
        <h2 class="text-sm font-semibold mb-2">フィード統計</h2>
        <FeedStatsList />
      </section>

      <Show
        when={!articles.loading}
        fallback={<p class="text-muted-foreground text-sm">読み込み中…</p>}
      >
        <Show
          when={(articles()?.length ?? 0) > 0}
          fallback={
            <p class="text-muted-foreground text-sm">
              記事がありません。フィードを追加してください。
            </p>
          }
        >
          <div class="space-y-3">
            <For each={articles()}>
              {(a) => (
                <a href={`/articles/${a.id}`} class="block">
                  <Card class="transition-colors hover:bg-accent">
                    <CardHeader>
                      <CardTitle
                        class={a.is_read ? "text-muted-foreground" : undefined}
                      >
                        {a.title}
                      </CardTitle>
                    </CardHeader>
                    <Show when={a.summary}>
                      <CardContent>
                        <p class="text-sm text-muted-foreground line-clamp-2">
                          {a.summary}
                        </p>
                      </CardContent>
                    </Show>
                  </Card>
                </a>
              )}
            </For>
          </div>
        </Show>
      </Show>
    </div>
  );
}
