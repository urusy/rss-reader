import { createSignal, For, Show } from "solid-js";
import { useApp } from "@/lib/store";
import { api, type DiscoveredFeed } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";

/**
 * フィード追加（機能08）＋ 自動検出（機能20）。
 * 「検出」: サイト/記事 URL から候補を取得し選んで購読。
 * 「直接追加」: 入力がフィード URL そのものなら従来どおり購読。
 */
export function AddFeedDialog() {
  const app = useApp();
  const [url, setUrl] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [open, setOpen] = createSignal(false);
  const [candidates, setCandidates] = createSignal<DiscoveredFeed[] | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  const reset = () => {
    setUrl("");
    setCandidates(null);
    setError(null);
  };

  const discover = async () => {
    const v = url().trim();
    if (!v) return;
    setBusy(true);
    setError(null);
    setCandidates(null);
    try {
      const res = await api.discoverFeeds(v);
      setCandidates(res.candidates);
      if (res.candidates.length === 0) {
        setError("このページからフィードを検出できませんでした。");
      }
    } catch (e) {
      setError(`検出に失敗しました: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const addDirect = async () => {
    const v = url().trim();
    if (!v) return;
    setBusy(true);
    setError(null);
    try {
      await api.addFeed(v);
      app.refetchFeeds();
      reset();
      setOpen(false);
    } catch (e) {
      setError(`追加に失敗しました: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const subscribe = async (c: DiscoveredFeed) => {
    setBusy(true);
    try {
      await api.addFeed(c.url);
      app.refetchFeeds();
      setCandidates((cs) =>
        (cs ?? []).map((x) =>
          x.url === c.url ? { ...x, already_subscribed: true } : x,
        ),
      );
    } catch (e) {
      setError(`購読に失敗しました: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Button size="sm" class="w-full" onClick={() => setOpen(true)}>
        フィードを追加
      </Button>
      <Dialog
        open={open()}
        onOpenChange={(d) => {
          setOpen(d.open);
          if (!d.open) reset();
        }}
      >
        <DialogContent>
          <DialogTitle>フィードを追加</DialogTitle>
          <div class="mt-4 space-y-3">
            <Input
              placeholder="サイトURL または フィードURL"
              value={url()}
              onInput={(e) => setUrl(e.currentTarget.value)}
              onKeyDown={(e) => e.key === "Enter" && discover()}
            />
            <div class="flex flex-wrap justify-end gap-2">
              <Button variant="outline" onClick={() => setOpen(false)}>
                キャンセル
              </Button>
              <Button variant="outline" onClick={addDirect} disabled={busy()}>
                直接追加
              </Button>
              <Button onClick={discover} disabled={busy()}>
                {busy() ? "検出中…" : "検出"}
              </Button>
            </div>

            <Show when={error()}>
              <p class="text-sm text-destructive">{error()}</p>
            </Show>

            <Show when={(candidates()?.length ?? 0) > 0}>
              <ul class="divide-y divide-border border-t border-border">
                <For each={candidates()!}>
                  {(c) => (
                    <li class="flex items-center justify-between gap-3 py-2">
                      <div class="min-w-0">
                        <p class="truncate text-sm font-medium">
                          {c.title ?? c.url}
                        </p>
                        <p class="truncate text-xs text-muted-foreground">
                          {c.kind.toUpperCase()} ・ {c.url}
                        </p>
                      </div>
                      <Show
                        when={!c.already_subscribed}
                        fallback={<Badge>購読済み</Badge>}
                      >
                        <Button
                          size="sm"
                          onClick={() => void subscribe(c)}
                          disabled={busy()}
                        >
                          購読
                        </Button>
                      </Show>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
