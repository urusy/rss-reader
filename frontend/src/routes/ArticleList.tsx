import {
  createEffect,
  createMemo,
  createResource,
  createSignal,
  For,
  Show,
} from "solid-js";
import { useSearchParams } from "@solidjs/router";
import { api, errorStatus } from "@/lib/api";
import { useSelection } from "@/lib/selection";
import { useApp } from "@/lib/store";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { formatDate } from "@/lib/format";
import { cn } from "@/lib/utils";

/**
 * 3ペインの中央ペイン。URL（useSelection）と store.filter から listArticles の
 * 引数を組み立てて自動再フェッチする。行クリックは ?article=<id> を立てて右ペインに
 * 本文を出すだけ（パス＝scope は保持）。all / feed / folder / 未分類 の全 scope に対応。
 */
export default function ArticleList() {
  const scope = useSelection();
  const app = useApp();
  const [searchParams, setSearchParams] = useSearchParams();

  const selectedId = () =>
    Array.isArray(searchParams.article)
      ? searchParams.article[0]
      : searchParams.article;

  // scope と未読フィルタを依存にしたソース。view scope のときだけ resolver を呼ぶ。
  const source = () => ({
    s: scope(),
    unread: app.state.filter === "unread" ? true : undefined,
  });

  const [articles, { refetch }] = createResource(source, async ({ s, unread }) => {
    if (s.kind === "view")
      return api.resolveSavedView(s.viewId, unread || undefined);
    if (s.kind === "feed")
      return api.listArticles({ feed_id: s.feedId, unread });
    if (s.kind === "folder")
      return s.folderId === "unclassified"
        ? api.listArticles({ unclassified: true, unread })
        : api.listArticles({ folder_id: s.folderId, unread });
    return api.listArticles({ unread });
  });
  const [stats, { refetch: refetchStats }] = createResource(() =>
    api.getStats(),
  );
  const [marking, setMarking] = createSignal(false);

  // --- 関連度スコア (#25): スコア結合 + 重要順ソート（articles 不変・クライアント側） ---
  const [scoring, setScoring] = createSignal(false);
  const scoreById = createMemo(() => {
    const m = new Map<string, number>();
    for (const s of app.relevanceScores() ?? []) m.set(s.article_id, s.score);
    return m;
  });
  const sorted = createMemo(() => {
    const list = articles() ?? [];
    if (app.state.sort !== "relevance") return list;
    const m = scoreById();
    return list.slice().sort((a, b) => (m.get(b.id) ?? -1) - (m.get(a.id) ?? -1));
  });
  const runScoring = async () => {
    setScoring(true);
    try {
      await api.scoreRelevance();
      app.refetchRelevanceScores();
      app.setSort("relevance");
    } catch (e) {
      const code = errorStatus(e);
      alert(
        code === 503
          ? "ANTHROPIC_API_KEY が未設定です。"
          : code === 502
            ? "スコアリングに失敗しました。"
            : `エラー: ${String(e)}`,
      );
    } finally {
      setScoring(false);
    }
  };

  // 現在の表示順を store に公開（キーボード j/k/o/Enter の移動対象。#18）。
  createEffect(() => {
    app.setNavItems((sorted() ?? []).map((a) => ({ id: a.id, url: a.url })));
  });

  // 行選択: ?article を立てて右ペインへ。既読化は本文側の滞在/スクロール起点で行うため、
  // ここでは楽観的に既読にしない。実既読になったら store.readIds 経由でグレーアウトする。
  const select = (id: string) => {
    setSearchParams({ article: id });
  };

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
    <div class="space-y-4 p-4">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <Badge variant={(stats()?.unread ?? 0) > 0 ? "unread" : "default"}>
          未読 {stats()?.unread ?? 0} 件
        </Badge>
        <div class="flex flex-wrap items-center gap-1">
          <Button
            size="sm"
            variant={app.state.sort === "relevance" ? "default" : "outline"}
            onClick={() =>
              app.setSort(
                app.state.sort === "relevance" ? "newest" : "relevance",
              )
            }
            title="新着順 / 重要順を切り替え"
          >
            {app.state.sort === "relevance" ? "重要順" : "新着順"}
          </Button>
          <Button size="sm" variant="outline" onClick={runScoring} disabled={scoring()}>
            {scoring() ? "スコア中…" : "スコアリング"}
          </Button>
          <Show when={scope().kind !== "view"}>
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
          </Show>
        </div>
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
            <For each={sorted()}>
              {(a) => (
                <button
                  type="button"
                  onClick={() => select(a.id)}
                  class={cn(
                    "block w-full rounded-md px-2 py-3 text-left transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                    selectedId() === a.id && "bg-accent",
                  )}
                >
                  <div class="flex items-start justify-between gap-2">
                    <p
                      class={cn(
                        "line-clamp-2 text-sm",
                        a.is_read || app.state.readIds[a.id]
                          ? "font-normal text-muted-foreground"
                          : "font-semibold text-foreground",
                      )}
                    >
                      {a.title}
                    </p>
                    <Show when={scoreById().has(a.id)}>
                      <Badge>{Math.round((scoreById().get(a.id) ?? 0) * 100)}%</Badge>
                    </Show>
                  </div>
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
                </button>
              )}
            </For>
          </div>
        </Show>
      </Show>
    </div>
  );
}
