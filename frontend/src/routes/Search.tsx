import { createResource, For, Show } from "solid-js";
import { A, useSearchParams } from "@solidjs/router";
import { api } from "@/lib/api";
import { normalizeQuery } from "@/lib/search";
import { Badge } from "@/components/ui/badge";
import { formatDate } from "@/lib/format";
import { cn } from "@/lib/utils";

/**
 * 全文検索の結果ページ。URL の ?q= を唯一の入力源にして自動再フェッチする
 * （検索ボックスは /search?q=… へ遷移するだけ）。空クエリは検索しない。
 */
export default function Search() {
  const [params] = useSearchParams();
  // ?q= は string | string[] | undefined。先頭の文字列だけ採り、trim する。
  const query = () => {
    const raw = Array.isArray(params.q) ? params.q[0] : params.q;
    return normalizeQuery(raw ?? "");
  };

  const [results] = createResource(
    () => query() || null, // null のときは fetcher を呼ばない
    (q) => api.searchArticles(q),
  );

  return (
    <div class="mx-auto max-w-3xl space-y-4 px-4 py-6">
      <Show
        when={query() !== ""}
        fallback={
          <p class="text-sm text-muted-foreground">検索語を入力してください。</p>
        }
      >
        <div class="flex items-center justify-between gap-2">
          <p class="text-sm text-muted-foreground">
            「<span class="font-medium text-foreground">{query()}</span>」の検索結果
          </p>
          <Show when={!results.loading}>
            <Badge>{results()?.length ?? 0} 件</Badge>
          </Show>
        </div>

        <Show
          when={!results.loading}
          fallback={<p class="text-sm text-muted-foreground">検索中…</p>}
        >
          <Show
            when={(results()?.length ?? 0) > 0}
            fallback={
              <p class="text-sm text-muted-foreground">
                一致する記事はありませんでした。
              </p>
            }
          >
            <div class="divide-y divide-border">
              <For each={results()}>
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
      </Show>
    </div>
  );
}
