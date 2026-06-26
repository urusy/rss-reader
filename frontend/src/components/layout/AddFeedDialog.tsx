import { createSignal } from "solid-js";
import { useApp } from "@/lib/store";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";

/**
 * フィード追加（機能08）。記事一覧上部の常駐 input を撤去し、Sidebar 下部の
 * ボタンから Dialog を開く配置に変更。open は signal で制御（Ark v5 の asChild
 * レンダープロップを避け、トリガは外部ボタン + onOpenChange で同期）。
 */
export function AddFeedDialog() {
  const app = useApp();
  const [url, setUrl] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [open, setOpen] = createSignal(false);

  const add = async () => {
    const v = url().trim();
    if (!v) return;
    setBusy(true);
    try {
      await api.addFeed(v);
      setUrl("");
      app.refetchFeeds();
      setOpen(false);
    } catch (e) {
      alert(`追加に失敗しました: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Button size="sm" class="w-full" onClick={() => setOpen(true)}>
        フィードを追加
      </Button>
      <Dialog open={open()} onOpenChange={(d) => setOpen(d.open)}>
        <DialogContent>
          <DialogTitle>フィードを追加</DialogTitle>
          <div class="mt-4 space-y-3">
            <Input
              placeholder="https://example.com/feed.xml"
              value={url()}
              onInput={(e) => setUrl(e.currentTarget.value)}
              onKeyDown={(e) => e.key === "Enter" && add()}
            />
            <div class="flex justify-end gap-2">
              <Button variant="outline" onClick={() => setOpen(false)}>
                キャンセル
              </Button>
              <Button onClick={add} disabled={busy()}>
                {busy() ? "追加中…" : "追加"}
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
