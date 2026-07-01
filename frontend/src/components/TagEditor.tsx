import { createResource, createSignal, For, Show } from "solid-js";
import {
  api,
  errorStatus,
  type ArticleTag,
  type TagSuggestion,
} from "@/lib/api";
import { TagBadge } from "@/components/ui/tag-badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/** 記事1件のタグ編集 + AI 提案（#24）。 */
export default function TagEditor(props: { articleId: string }) {
  const [tags, { refetch }] = createResource(
    () => props.articleId,
    (id) => api.getArticleTags(id),
  );
  const [draft, setDraft] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [suggestions, setSuggestions] = createSignal<TagSuggestion[] | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  const currentUserIds = () =>
    (tags() ?? []).filter((t) => t.attached_source === "user").map((t) => t.id);

  // 新規/既存タグ名を作成→記事に付与（user エッジ集合に追加）。
  const addByName = async (name: string) => {
    const n = name.trim();
    if (!n) return;
    setBusy(true);
    setError(null);
    try {
      const tag = await api.createTag({ name: n });
      const next = Array.from(new Set([...currentUserIds(), tag.id]));
      await api.setArticleTags(props.articleId, next);
      setDraft("");
      await refetch();
    } catch (e) {
      setError(`タグ追加に失敗: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const remove = async (t: ArticleTag) => {
    await api.detachArticleTag(props.articleId, t.id);
    await refetch();
  };

  const suggest = async (refresh = false) => {
    setBusy(true);
    setError(null);
    try {
      setSuggestions(await api.suggestTags(props.articleId, refresh));
    } catch (e) {
      const code = errorStatus(e);
      setError(
        code === 503
          ? "ANTHROPIC_API_KEY が未設定です。"
          : code === 502
            ? "提案の生成に失敗しました。"
            : `エラー: ${String(e)}`,
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <section class="space-y-2">
      <div class="flex flex-wrap items-center gap-1">
        <For each={tags()}>
          {(t) => (
            <TagBadge
              color={t.color}
              ai={t.attached_source === "ai"}
              onRemove={() => void remove(t)}
            >
              {t.name}
            </TagBadge>
          )}
        </For>
        <Show when={(tags()?.length ?? 0) === 0}>
          <span class="text-xs text-muted-foreground">タグなし</span>
        </Show>
      </div>

      <div class="flex items-center gap-2">
        <Input
          class="h-8 max-w-48"
          placeholder="タグを追加…"
          value={draft()}
          onInput={(e) => setDraft(e.currentTarget.value)}
          onKeyDown={(e) => e.key === "Enter" && void addByName(draft())}
          disabled={busy()}
        />
        <Button size="sm" variant="outline" onClick={() => void suggest(false)} disabled={busy()}>
          AI でタグ提案
        </Button>
      </div>

      <Show when={error()}>
        <p class="text-xs text-destructive">{error()}</p>
      </Show>

      <Show when={suggestions()}>
        <div class="flex flex-wrap items-center gap-1">
          <span class="text-xs text-muted-foreground">提案:</span>
          <For each={suggestions()}>
            {(s) => (
              <button
                type="button"
                class="inline-flex items-center gap-1 rounded-full border border-dashed border-border bg-background px-2 py-0.5 text-xs text-muted-foreground hover:bg-accent"
                onClick={() => void addByName(s.name)}
              >
                + {s.name}
              </button>
            )}
          </For>
          <Button size="sm" variant="ghost" onClick={() => void suggest(true)} disabled={busy()}>
            再提案
          </Button>
        </div>
      </Show>
    </section>
  );
}
