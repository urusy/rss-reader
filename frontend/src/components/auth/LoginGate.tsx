import {
  Show,
  createResource,
  createSignal,
  type ParentComponent,
} from "solid-js";
import { api } from "@/lib/api";
import { authToken, setToken } from "@/lib/auth";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/**
 * 最外殻のアクセスゲート。認証不要(required:false)か有効トークン保持済みなら子を描画。
 * それ以外はログインフォームを出す。getAuthStatus は公開エンドポイントなので
 * トークン無しでも判定でき、鶏卵問題にならない。
 */
const LoginGate: ParentComponent = (props) => {
  const [status] = createResource(() => api.getAuthStatus());
  const [input, setInput] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);

  const authed = () => status()?.required === false || !!authToken();

  const submit = async (e: Event) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.login(input()); // 401 なら throw
      setToken(input()); // 検証通過後に保存（以降のヘッダに載る）
      setInput("");
    } catch {
      setError("トークンが正しくありません");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Show
      when={authed()}
      fallback={
        <div class="flex min-h-dvh items-center justify-center bg-background p-4">
          <Card class="w-full max-w-sm p-6">
            <h1 class="mb-1 text-lg font-semibold">サインイン</h1>
            <p class="mb-4 text-xs text-muted-foreground">
              アクセストークンを入力してください。
            </p>
            <form onSubmit={submit} class="space-y-3">
              <Input
                type="password"
                autocomplete="current-password"
                placeholder="AUTH_TOKEN"
                value={input()}
                onInput={(e) => setInput(e.currentTarget.value)}
              />
              <Show when={error()}>
                {(m) => <p class="text-xs text-destructive">{m()}</p>}
              </Show>
              <Button type="submit" class="w-full" disabled={busy() || !input()}>
                {busy() ? "確認中..." : "サインイン"}
              </Button>
            </form>
          </Card>
        </div>
      }
    >
      {props.children}
    </Show>
  );
};

export default LoginGate;
