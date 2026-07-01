import { createResource, createSignal, For, Show } from "solid-js";
import { api, errorStatus, type ClusterWithMembers } from "@/lib/api";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";

/** 意味的クラスタリング（#26）。話題ごとに記事をカードでまとめ、統合要約を生成。 */
export default function Clusters() {
  const [clusters, { refetch }] = createResource(() => api.listClusters());
  const [busy, setBusy] = createSignal(false);
  // クラスタ別の統合要約（生成後の表示用）と busy/error。
  const [summaries, setSummaries] = createSignal<Record<string, string>>({});
  const [sumBusy, setSumBusy] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  const recompute = async () => {
    setBusy(true);
    try {
      await api.reclusterNow();
      await refetch();
    } finally {
      setBusy(false);
    }
  };

  const summarize = async (c: ClusterWithMembers) => {
    setSumBusy(c.id);
    setError(null);
    try {
      const updated = await api.summarizeCluster(c.id);
      if (updated.summary)
        setSummaries((s) => ({ ...s, [c.id]: updated.summary! }));
    } catch (e) {
      const code = errorStatus(e);
      setError(
        code === 503
          ? "ANTHROPIC_API_KEY が未設定です。"
          : code === 502
            ? "要約生成に失敗しました。"
            : `エラー: ${String(e)}`,
      );
    } finally {
      setSumBusy(null);
    }
  };

  const sourceCount = (c: ClusterWithMembers) =>
    new Set(c.members.map((m) => m.feed_id)).size;

  return (
    <div class="mx-auto max-w-3xl space-y-4 px-4 py-6">
      <div class="flex items-center justify-between gap-2">
        <h1 class="text-2xl font-bold tracking-tight">話題のまとまり</h1>
        <Button onClick={recompute} disabled={busy()}>
          {busy() ? "再計算中…" : "再計算"}
        </Button>
      </div>

      <Show when={error()}>
        <p class="text-sm text-destructive">{error()}</p>
      </Show>

      <Show
        when={(clusters()?.length ?? 0) > 0}
        fallback={
          <p class="text-sm text-muted-foreground">
            まとまった話題はまだありません。「再計算」で作成できます。
          </p>
        }
      >
        <For each={clusters()}>
          {(c) => (
            <Card>
              <CardHeader>
                <CardTitle>
                  <div class="flex flex-wrap items-center gap-2">
                    <span class="text-base">{c.title}</span>
                    <Badge>{c.size} 件</Badge>
                    <Badge>{sourceCount(c)} 媒体</Badge>
                  </div>
                </CardTitle>
              </CardHeader>
              <CardContent class="space-y-3">
                <ul class="space-y-1">
                  <For each={c.members}>
                    {(m) => (
                      <li class="flex items-start gap-2 text-sm">
                        <span class="shrink-0 text-xs text-muted-foreground">
                          {m.feed_title ?? "—"}
                        </span>
                        <a
                          href={m.url}
                          target="_blank"
                          rel="noreferrer"
                          class="min-w-0 flex-1 truncate underline underline-offset-2 hover:text-foreground"
                        >
                          {m.title}
                        </a>
                        <Show when={m.is_representative}>
                          <Badge variant="unread">代表</Badge>
                        </Show>
                        <Show when={m.is_duplicate}>
                          <Badge variant="stale">重複</Badge>
                        </Show>
                      </li>
                    )}
                  </For>
                </ul>

                <Show
                  when={summaries()[c.id] ?? c.summary}
                  fallback={
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => void summarize(c)}
                      disabled={sumBusy() === c.id}
                    >
                      {sumBusy() === c.id ? "要約中…" : "統合要約"}
                    </Button>
                  }
                >
                  {(text) => (
                    <div class="rounded-md border border-border bg-muted/40 p-3">
                      <p class="prose prose-sm dark:prose-invert max-w-none whitespace-pre-wrap">
                        {text()}
                      </p>
                    </div>
                  )}
                </Show>
              </CardContent>
            </Card>
          )}
        </For>
      </Show>
    </div>
  );
}
