import { createSignal, Show } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";

/**
 * 任意 Web ページの保存（Pocket 風「後で読む」）。AddFeedDialog の縮小版。
 * 保存は 201 即返し（本文抽出は背景実行）なので、成功したら /saved へ遷移して
 * 一覧に行を出す。タイトル・本文は抽出完了後に確定する。
 */
export function SavePageDialog() {
  const navigate = useNavigate();
  const [url, setUrl] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [open, setOpen] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const reset = () => {
    setUrl("");
    setError(null);
  };

  const save = async () => {
    const v = url().trim();
    if (!v) return;
    // 軽い事前チェックのみ（本検証はサーバの SavedUrl::parse）
    try {
      new URL(v);
    } catch {
      setError("URL の形式が正しくありません。");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.savePage(v);
      reset();
      setOpen(false);
      navigate("/saved");
    } catch (e) {
      setError(`保存に失敗しました: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Button
        size="sm"
        variant="outline"
        class="w-full"
        onClick={() => setOpen(true)}
      >
        ページを保存
      </Button>
      <Dialog
        open={open()}
        onOpenChange={(d) => {
          setOpen(d.open);
          if (!d.open) reset();
        }}
      >
        <DialogContent>
          <DialogTitle>ページを保存（後で読む）</DialogTitle>
          <div class="mt-4 space-y-3">
            <Input
              placeholder="https://example.com/article"
              value={url()}
              onInput={(e) => setUrl(e.currentTarget.value)}
              onKeyDown={(e) => e.key === "Enter" && save()}
            />
            <div class="flex justify-end gap-2">
              <Button variant="outline" onClick={() => setOpen(false)}>
                キャンセル
              </Button>
              <Button onClick={save} disabled={busy()}>
                {busy() ? "保存中…" : "保存"}
              </Button>
            </div>
            <Show when={error()}>
              <p class="text-sm text-destructive">{error()}</p>
            </Show>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
