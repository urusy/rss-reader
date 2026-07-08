import { For, Show, createResource, createSignal } from "solid-js";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { buttonVariants } from "@/components/ui/button";
import {
  Dialog,
  DialogCloseTrigger,
  DialogContent,
  DialogDescription,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { api, type SyncTokenInfo } from "@/lib/api";

/**
 * GReader 同期クライアント（機能29）: NetNewsWire / Reeder 等が ClientLogin で
 * 発行したトークンの一覧と失効。トークンは無期限（クライアントは再ログインを
 * 想定しない）なので、使わなくなった端末はここで失効させる。
 * バックエンドが SYNC_API_ENABLED=false でも一覧 API 自体は生きている
 * （空一覧 + 接続手順の案内として機能する）。
 */
export default function SyncClientsCard() {
  const [tokens, { refetch }] = createResource(() => api.listSyncTokens());
  const [error, setError] = createSignal<string | null>(null);

  const revoke = async (t: SyncTokenInfo) => {
    setError(null);
    try {
      await api.revokeSyncToken(t.id);
      void refetch();
    } catch {
      setError("トークンの失効に失敗しました");
    }
  };

  const fmt = (iso: string | null) =>
    iso ? new Date(iso).toLocaleString() : "未使用";

  return (
    <Card>
      <CardHeader>
        <CardTitle>同期クライアント（Google Reader API）</CardTitle>
      </CardHeader>
      <CardContent class="space-y-4">
        <div class="space-y-1 text-xs text-muted-foreground">
          <p>
            NetNewsWire / Reeder などの RSS クライアントから同期バックエンドとして
            使えます（バックエンドの <code>SYNC_API_ENABLED=true</code> が必要）。
          </p>
          <p>
            クライアント設定: アカウント種別 <span class="font-medium text-foreground">FreshRSS</span>
            {" ／ "}URL <code>http://&lt;このサーバー&gt;:8081</code>
            {" ／ "}ユーザー名は任意（下の一覧に表示される識別ラベル）
            {" ／ "}パスワードはログインパスワード。
          </p>
        </div>

        <Show when={error()}>{(m) => <p class="text-xs text-destructive">{m()}</p>}</Show>

        <div class="space-y-2">
          <p class="text-sm font-medium">接続中のクライアント</p>
          <Show
            when={(tokens() ?? []).length > 0}
            fallback={
              <p class="text-xs text-muted-foreground">
                {tokens.loading ? "読み込み中..." : "接続済みのクライアントはありません"}
              </p>
            }
          >
            <ul class="divide-y divide-border">
              <For each={tokens()}>
                {(t) => (
                  <li class="flex items-center gap-3 py-2">
                    <div class="min-w-0 flex-1">
                      <p class="truncate text-xs">{t.label ?? "（ラベルなし）"}</p>
                      <p class="text-xs text-muted-foreground">
                        最終利用: {fmt(t.last_used_at)} / 接続: {fmt(t.created_at)}
                      </p>
                    </div>
                    <Dialog>
                      <DialogTrigger
                        class={buttonVariants({ variant: "outline", size: "sm" })}
                      >
                        失効
                      </DialogTrigger>
                      <DialogContent>
                        <DialogTitle>同期トークンを失効しますか？</DialogTitle>
                        <DialogDescription>
                          「{t.label ?? "（ラベルなし）"}
                          」のクライアントは同期できなくなります。再接続には
                          クライアント側での再ログインが必要です。
                        </DialogDescription>
                        <div class="mt-4 flex justify-end gap-2">
                          <DialogCloseTrigger
                            class={buttonVariants({ variant: "outline" })}
                          >
                            キャンセル
                          </DialogCloseTrigger>
                          <DialogCloseTrigger
                            class={buttonVariants({ variant: "destructive" })}
                            onClick={() => void revoke(t)}
                          >
                            失効する
                          </DialogCloseTrigger>
                        </div>
                      </DialogContent>
                    </Dialog>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </div>
      </CardContent>
    </Card>
  );
}
