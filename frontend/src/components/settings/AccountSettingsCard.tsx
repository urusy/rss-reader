import { For, Show, createResource, createSignal } from "solid-js";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { api, errorStatus, type SessionInfo } from "@/lib/api";
import { setAuthState } from "@/lib/auth";

/**
 * アカウント設定カード: パスワード変更・ログアウト・ログイン中デバイスの
 * 一覧と個別失効。パスワード変更が成功すると他デバイスは全て失効する。
 */
export default function AccountSettingsCard() {
  const [sessions, { refetch }] = createResource(() => api.listSessions());

  const [currentPw, setCurrentPw] = createSignal("");
  const [newPw, setNewPw] = createSignal("");
  const [confirmPw, setConfirmPw] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [info, setInfo] = createSignal<string | null>(null);

  const changePassword = async (e: Event) => {
    e.preventDefault();
    if (newPw() !== confirmPw()) {
      setError("新しいパスワードが一致しません");
      return;
    }
    setBusy(true);
    setError(null);
    setInfo(null);
    try {
      await api.changePassword(currentPw(), newPw());
      setCurrentPw("");
      setNewPw("");
      setConfirmPw("");
      setInfo("パスワードを変更しました。他のデバイスはログアウトされます。");
      void refetch();
    } catch (err) {
      const code = errorStatus(err);
      if (code === 401) setError("現在のパスワードが正しくありません");
      else if (code === 429)
        setError("試行回数が多すぎます。しばらく待ってから再試行してください。");
      else setError("パスワードは8文字以上128文字以下で設定してください");
    } finally {
      setBusy(false);
    }
  };

  const logout = async () => {
    setBusy(true);
    try {
      await api.logout();
    } finally {
      setBusy(false);
      setAuthState("login");
    }
  };

  const revoke = async (s: SessionInfo) => {
    setBusy(true);
    setError(null);
    try {
      await api.revokeSession(s.id);
      if (s.current) {
        // 自分自身を失効させた場合はログイン画面へ。
        setAuthState("login");
        return;
      }
      void refetch();
    } catch {
      setError("セッションの失効に失敗しました");
    } finally {
      setBusy(false);
    }
  };

  const fmt = (iso: string) => new Date(iso).toLocaleString();

  return (
    <Card>
      <CardHeader class="flex-row items-center justify-between">
        <CardTitle>アカウント</CardTitle>
        <Button variant="outline" size="sm" onClick={() => void logout()} disabled={busy()}>
          ログアウト
        </Button>
      </CardHeader>
      <CardContent class="space-y-6">
        <form onSubmit={changePassword} class="space-y-3">
          <p class="text-sm font-medium">パスワード変更</p>
          <Input
            type="password"
            autocomplete="current-password"
            placeholder="現在のパスワード"
            value={currentPw()}
            onInput={(e) => setCurrentPw(e.currentTarget.value)}
          />
          <Input
            type="password"
            autocomplete="new-password"
            placeholder="新しいパスワード（8文字以上）"
            value={newPw()}
            onInput={(e) => setNewPw(e.currentTarget.value)}
          />
          <Input
            type="password"
            autocomplete="new-password"
            placeholder="新しいパスワード（確認）"
            value={confirmPw()}
            onInput={(e) => setConfirmPw(e.currentTarget.value)}
          />
          <Show when={error()}>
            {(m) => <p class="text-xs text-destructive">{m()}</p>}
          </Show>
          <Show when={info()}>
            {(m) => <p class="text-xs text-muted-foreground">{m()}</p>}
          </Show>
          <Button
            type="submit"
            disabled={busy() || !currentPw() || newPw().length < 8 || !confirmPw()}
          >
            {busy() ? "変更中..." : "変更する"}
          </Button>
        </form>

        <div class="space-y-2">
          <p class="text-sm font-medium">ログイン中のデバイス</p>
          <Show
            when={(sessions() ?? []).length > 0}
            fallback={<p class="text-xs text-muted-foreground">読み込み中...</p>}
          >
            <ul class="divide-y divide-border">
              <For each={sessions()}>
                {(s) => (
                  <li class="flex items-center gap-3 py-2">
                    <div class="min-w-0 flex-1">
                      <p class="truncate text-xs">
                        {s.label ?? "不明なデバイス"}
                        <Show when={s.current}>
                          <Badge class="ml-2">このデバイス</Badge>
                        </Show>
                      </p>
                      <p class="text-xs text-muted-foreground">
                        最終利用: {fmt(s.last_seen_at)} / 開始: {fmt(s.created_at)}
                      </p>
                    </div>
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={busy()}
                      onClick={() => void revoke(s)}
                    >
                      {s.current ? "ログアウト" : "失効"}
                    </Button>
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
