import { createSignal, For, Show } from "solid-js";
import { A } from "@solidjs/router";
import { api, type QuerySpec } from "@/lib/api";
import { useApp } from "@/lib/store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";

const navItem = "block h-8 px-2 rounded-md text-sm leading-8 hover:bg-accent";
const navActive = "bg-accent text-accent-foreground";

/** スマートビュー（仮想フィード）の一覧 + 作成ダイアログ（#27）。 */
export default function SavedViewList(props: { onNavigate?: () => void }) {
  const app = useApp();
  const [open, setOpen] = createSignal(false);
  const [name, setName] = createSignal("");
  const [text, setText] = createSignal("");
  const [unread, setUnread] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const reset = () => {
    setName("");
    setText("");
    setUnread(false);
    setError(null);
  };

  const save = async () => {
    if (!name().trim()) return;
    const query: QuerySpec = {};
    if (text().trim()) query.text = text().trim();
    if (unread()) query.unread_only = true;
    setBusy(true);
    setError(null);
    try {
      await api.createSavedView({ name: name().trim(), query });
      app.refetchSavedViews();
      reset();
      setOpen(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (id: string) => {
    await api.deleteSavedView(id);
    app.refetchSavedViews();
  };

  return (
    <div class="space-y-1">
      <div class="flex items-center justify-between px-2">
        <span class="text-xs font-semibold text-muted-foreground">
          スマートビュー
        </span>
        <button
          type="button"
          class="text-xs text-muted-foreground hover:text-foreground"
          onClick={() => setOpen(true)}
        >
          ＋
        </button>
      </div>
      <For each={app.savedViews()}>
        {(v) => (
          <div class="group flex items-center gap-1">
            <A
              href={`/views/${v.id}`}
              class={`${navItem} min-w-0 flex-1 truncate`}
              activeClass={navActive}
              onClick={() => props.onNavigate?.()}
            >
              {v.name}
            </A>
            <button
              type="button"
              class="shrink-0 px-1 text-xs text-muted-foreground opacity-0 hover:text-destructive group-hover:opacity-100"
              title="削除"
              onClick={() => void remove(v.id)}
            >
              ×
            </button>
          </div>
        )}
      </For>

      <Dialog
        open={open()}
        onOpenChange={(d) => {
          setOpen(d.open);
          if (!d.open) reset();
        }}
      >
        <DialogContent>
          <DialogTitle>スマートビューを作成</DialogTitle>
          <div class="mt-4 space-y-3">
            <Input
              placeholder="ビュー名（例: Rust の未読）"
              value={name()}
              onInput={(e) => setName(e.currentTarget.value)}
            />
            <Input
              placeholder="検索語（任意・タイトル/本文）"
              value={text()}
              onInput={(e) => setText(e.currentTarget.value)}
            />
            <label class="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={unread()}
                onChange={(e) => setUnread(e.currentTarget.checked)}
              />
              未読のみ
            </label>
            <Show when={error()}>
              <p class="text-xs text-destructive">{error()}</p>
            </Show>
            <div class="flex justify-end gap-2">
              <Button variant="outline" onClick={() => setOpen(false)}>
                キャンセル
              </Button>
              <Button onClick={save} disabled={busy() || !name().trim()}>
                {busy() ? "保存中…" : "保存"}
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
