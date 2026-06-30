import { createResource, createSignal, For, Show } from "solid-js";
import { api, type Highlight } from "@/lib/api";
import { Button } from "@/components/ui/button";

/**
 * スター（星）のトグル（#32）。現在の星付き id 一覧から所属を判定し、
 * 楽観的に表示を切り替える。ローカル知識ベースで外部同期しない。
 */
export function StarToggle(props: { articleId: string }) {
  const [stars, { mutate }] = createResource(api.listStars);
  const starred = () => (stars() ?? []).includes(props.articleId);
  const [busy, setBusy] = createSignal(false);

  const toggle = async () => {
    const id = props.articleId;
    const next = !starred();
    setBusy(true);
    // 楽観更新（失敗時は元に戻す）
    mutate((prev) =>
      next ? [...(prev ?? []), id] : (prev ?? []).filter((x) => x !== id),
    );
    try {
      if (next) await api.addStar(id);
      else await api.removeStar(id);
    } catch (e) {
      mutate((prev) =>
        next ? (prev ?? []).filter((x) => x !== id) : [...(prev ?? []), id],
      );
      alert(`スターの更新に失敗しました: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Button
      size="sm"
      variant={starred() ? "default" : "outline"}
      onClick={() => void toggle()}
      disabled={busy()}
      title={starred() ? "スターを外す" : "スターを付ける"}
    >
      {starred() ? "★ スター済み" : "☆ スター"}
    </Button>
  );
}

/**
 * ハイライト / 注釈（#32）。本文の選択範囲を quote として保存し、任意メモを付ける。
 * 位置アンカーは quote 文字列を正典とし、offset は best-effort のヒント。
 */
export function Highlights(props: { articleId: string }) {
  const [list, { refetch }] = createResource(
    () => props.articleId,
    (id) => api.getHighlights(id),
  );
  const [note, setNote] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  // 現在の本文選択テキスト（trim 済み）。空なら null。
  const currentSelection = (): string | null => {
    const sel = window.getSelection?.()?.toString().trim();
    return sel && sel.length > 0 ? sel : null;
  };

  const add = async () => {
    const quote = currentSelection();
    if (!quote) {
      setError("本文を選択してからハイライトを追加してください。");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.createHighlight(props.articleId, {
        quote,
        note: note().trim() || undefined,
      });
      setNote("");
      window.getSelection?.()?.removeAllRanges();
      await refetch();
    } catch (e) {
      setError(`ハイライトの保存に失敗: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const remove = async (h: Highlight) => {
    await api.deleteHighlight(h.id);
    await refetch();
  };

  const editNote = async (h: Highlight) => {
    const next = prompt("メモ", h.note ?? "");
    if (next === null) return; // キャンセル
    await api.updateHighlight(h.id, { note: next });
    await refetch();
  };

  return (
    <section class="space-y-3 border-t border-border pt-4">
      <h2 class="text-sm font-semibold">ハイライト</h2>

      <div class="flex items-start gap-2">
        <textarea
          class="min-h-9 flex-1 rounded-md border border-border bg-background px-2 py-1.5 text-sm"
          placeholder="本文を選択 →（任意）メモを書いて「追加」"
          value={note()}
          onInput={(e) => setNote(e.currentTarget.value)}
          rows={1}
        />
        <Button size="sm" variant="outline" onClick={() => void add()} disabled={busy()}>
          選択範囲を追加
        </Button>
      </div>

      <Show when={error()}>
        <p class="text-xs text-destructive">{error()}</p>
      </Show>

      <Show
        when={(list()?.length ?? 0) > 0}
        fallback={<p class="text-xs text-muted-foreground">まだハイライトはありません。</p>}
      >
        <ul class="space-y-2">
          <For each={list()}>
            {(h) => (
              <li class="rounded-md border border-border bg-muted/30 p-2 text-sm">
                <blockquote class="border-l-2 border-border pl-2 italic text-foreground/90">
                  {h.quote}
                </blockquote>
                <Show when={h.note}>
                  <p class="mt-1 text-xs text-muted-foreground whitespace-pre-wrap">
                    {h.note}
                  </p>
                </Show>
                <div class="mt-1 flex gap-3">
                  <button
                    type="button"
                    class="text-xs text-muted-foreground underline underline-offset-2 hover:text-foreground"
                    onClick={() => void editNote(h)}
                  >
                    メモを編集
                  </button>
                  <button
                    type="button"
                    class="text-xs text-destructive underline underline-offset-2"
                    onClick={() => void remove(h)}
                  >
                    削除
                  </button>
                </div>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </section>
  );
}
