import { createResource, createSignal, Show } from "solid-js";
import { useSearchParams } from "@solidjs/router";
import { api, errorStatus } from "@/lib/api";
import { renderMarkdown } from "@/lib/markdown";
import { Prose } from "@/components/ui/prose";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";

/** AI デイリーダイジェスト（#23）。最新 or ?date= 指定日を Markdown 表示。 */
export default function Digest() {
  const [searchParams] = useSearchParams();
  const [digest, { mutate }] = createResource(
    () => (searchParams.date as string | undefined) ?? "__latest__",
    (key) =>
      key === "__latest__" ? api.getLatestDigest() : api.getDigest(key),
  );
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const html = () => renderMarkdown(digest()?.markdown);

  const regenerate = async () => {
    setBusy(true);
    setError(null);
    try {
      mutate(await api.refreshDigest());
    } catch (e) {
      const code = errorStatus(e);
      setError(
        code === 503
          ? "ANTHROPIC_API_KEY が未設定です。"
          : code === 502
            ? "生成に失敗しました（Claude 障害）。"
            : `エラー: ${String(e)}`,
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="mx-auto max-w-3xl space-y-4 px-4 py-6">
      <div class="flex items-center justify-between gap-2">
        <h1 class="text-2xl font-bold tracking-tight">デイリーダイジェスト</h1>
        <Button onClick={regenerate} disabled={busy()}>
          {busy() ? "生成中…" : "再生成"}
        </Button>
      </div>

      <Show when={error()}>
        <p class="text-sm text-destructive">{error()}</p>
      </Show>

      <Show
        when={digest()}
        fallback={
          <Show
            when={digest.error && errorStatus(digest.error) === 404}
            fallback={
              <p class="text-sm text-muted-foreground">読み込み中…</p>
            }
          >
            <p class="text-sm text-muted-foreground">
              まだダイジェストがありません。「再生成」で作成できます。
            </p>
          </Show>
        }
      >
        {(d) => (
          <Card>
            <CardHeader>
              <CardTitle>
                <div class="flex items-center gap-2">
                  {d().date}
                  <Badge>{d().article_count} 件の記事から生成</Badge>
                </div>
              </CardTitle>
            </CardHeader>
            <CardContent>
              <Prose html={html()} />
            </CardContent>
          </Card>
        )}
      </Show>
    </div>
  );
}
