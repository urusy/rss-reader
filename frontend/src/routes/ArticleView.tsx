import { createResource, createSignal, Show } from "solid-js";
import { useParams } from "@solidjs/router";
import { api, type Article } from "@/lib/api";
import { Button } from "@/components/ui/button";

export default function ArticleView() {
  const params = useParams();
  const [article, { mutate }] = createResource(() => params.id, api.getArticle);
  const [busy, setBusy] = createSignal<"summarize" | "translate" | null>(null);

  const run = async (kind: "summarize" | "translate") => {
    setBusy(kind);
    try {
      const updated: Article =
        kind === "summarize"
          ? await api.summarize(params.id, "ja")
          : await api.translate(params.id, "ja");
      mutate(updated);
    } catch (e) {
      alert(`処理に失敗しました: ${String(e)}`);
    } finally {
      setBusy(null);
    }
  };

  return (
    <Show
      when={article()}
      fallback={<p class="text-muted-foreground text-sm">読み込み中…</p>}
    >
      {(a) => (
        <article class="space-y-4">
          <header class="space-y-2">
            <h1 class="text-2xl font-bold tracking-tight">{a().title}</h1>
            <a
              href={a().url}
              target="_blank"
              rel="noreferrer"
              class="text-sm text-muted-foreground underline underline-offset-4"
            >
              元記事を開く ↗
            </a>
          </header>

          <div class="flex gap-2">
            <Button size="sm" onClick={() => run("summarize")} disabled={busy() !== null}>
              {busy() === "summarize" ? "要約中…" : "要約 (Claude)"}
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => run("translate")}
              disabled={busy() !== null}
            >
              {busy() === "translate" ? "翻訳中…" : "翻訳 (Claude)"}
            </Button>
          </div>

          <Show when={a().summary}>
            <section class="rounded-lg border border-border bg-muted/40 p-4">
              <h2 class="text-sm font-semibold mb-1">要約</h2>
              <p class="text-sm whitespace-pre-wrap">{a().summary}</p>
            </section>
          </Show>

          <Show when={a().translation}>
            <section class="rounded-lg border border-border bg-muted/40 p-4">
              <h2 class="text-sm font-semibold mb-1">翻訳</h2>
              <div class="prose prose-sm dark:prose-invert max-w-none whitespace-pre-wrap">
                {a().translation}
              </div>
            </section>
          </Show>

          <div
            class="prose prose-sm dark:prose-invert max-w-none"
            innerHTML={a().content}
          />
        </article>
      )}
    </Show>
  );
}
