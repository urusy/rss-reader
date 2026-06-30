import { createResource, createSignal, Show } from "solid-js";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";

export default function Settings() {
  const [status, { refetch }] = createResource(() => api.getInstapaperStatus());
  const [username, setUsername] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.saveInstapaperCredentials({
        username: username().trim(),
        password: password(),
      });
      setUsername("");
      setPassword("");
      await refetch();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.deleteInstapaperCredentials();
      await refetch();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="mx-auto max-w-3xl space-y-6 px-4 py-6">
      <h1 class="text-2xl font-bold tracking-tight">設定</h1>

      <Card>
        <CardHeader>
          <CardTitle>
            <div class="flex items-center gap-2">
              Instapaper 連携
              <Show
                when={status()?.configured}
                fallback={<Badge>未接続</Badge>}
              >
                <Badge variant="unread">接続済み</Badge>
              </Show>
            </div>
          </CardTitle>
        </CardHeader>
        <CardContent class="space-y-3">
          <p class="text-xs text-muted-foreground">
            記事を Instapaper に送るには、Instapaper のメールアドレスとパスワードを登録してください。
          </p>

          <Show when={error()}>
            <p class="text-sm text-destructive">{error()}</p>
          </Show>

          <div class="space-y-2">
            <Input
              type="email"
              placeholder="you@example.com"
              autocomplete="off"
              value={username()}
              onInput={(e) => setUsername(e.currentTarget.value)}
            />
            <Input
              type="password"
              placeholder="パスワード"
              autocomplete="off"
              value={password()}
              onInput={(e) => setPassword(e.currentTarget.value)}
            />
          </div>

          <div class="flex gap-2">
            <Button
              onClick={save}
              disabled={busy() || !username().trim() || !password()}
            >
              {busy() ? "保存中…" : "保存"}
            </Button>
            <Show when={status()?.configured}>
              <Button variant="destructive" onClick={remove} disabled={busy()}>
                資格情報を削除
              </Button>
            </Show>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
