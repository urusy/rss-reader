import { createSignal, For, Show } from "solid-js";
import { api, errorStatus, type AskMessage } from "@/lib/api";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/**
 * Ask Claude (#22): 記事ビュー内のチャット UI。会話はローカル state（ステートレス
 * 契約 = 毎回 messages[] を送る）。記事を切り替えると新しい ArticleAsk がマウントされ
 * 会話はリセットされる。
 */
export default function ArticleAsk(props: { articleId: string }) {
  const [messages, setMessages] = createSignal<AskMessage[]>([]);
  const [draft, setDraft] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [save, setSave] = createSignal(false);

  const send = async () => {
    const q = draft().trim();
    if (!q || busy()) return;
    const next: AskMessage[] = [...messages(), { role: "user", content: q }];
    setMessages(next);
    setDraft("");
    setBusy(true);
    setError(null);
    try {
      const res = await api.askArticle(props.articleId, next, save());
      setMessages([...next, { role: "assistant", content: res.answer }]);
    } catch (e) {
      // 失敗時は末尾の user メッセージを差し戻して再送可能に。
      setMessages(messages().slice(0, -1));
      setDraft(q);
      const code = errorStatus(e);
      setError(
        code === 503
          ? "ANTHROPIC_API_KEY が未設定です（要約/翻訳と同じ）。"
          : code === 502
            ? "Claude への接続に失敗しました。"
            : `エラー: ${String(e)}`,
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card class="space-y-3 p-4">
      <h2 class="text-sm font-medium">Claude に質問</h2>

      <Show when={messages().length > 0}>
        <div class="space-y-2">
          <For each={messages()}>
            {(msg) => (
              <div
                class={
                  msg.role === "user"
                    ? "ml-auto max-w-[85%] rounded-lg bg-primary px-3 py-2 text-sm text-primary-foreground"
                    : "mr-auto max-w-[85%] rounded-lg bg-muted px-3 py-2"
                }
              >
                <div
                  class={
                    msg.role === "assistant"
                      ? "prose prose-sm dark:prose-invert max-w-none whitespace-pre-wrap"
                      : "whitespace-pre-wrap"
                  }
                >
                  {msg.content}
                </div>
              </div>
            )}
          </For>
        </div>
      </Show>

      <Show when={error()}>
        <p class="text-sm text-destructive">{error()}</p>
      </Show>

      <div class="flex items-center gap-2">
        <Input
          placeholder="この記事について質問…"
          value={draft()}
          onInput={(e) => setDraft(e.currentTarget.value)}
          onKeyDown={(e) => e.key === "Enter" && send()}
          disabled={busy()}
        />
        <Button onClick={send} disabled={busy() || !draft().trim()}>
          {busy() ? "問い合わせ中…" : "質問"}
        </Button>
      </div>

      <label class="flex items-center gap-2 text-xs text-muted-foreground">
        <input
          type="checkbox"
          checked={save()}
          onChange={(e) => setSave(e.currentTarget.checked)}
        />
        この Q&A を記事に保存する
      </label>
    </Card>
  );
}
