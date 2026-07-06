import {
  Match,
  Show,
  Switch,
  createEffect,
  createResource,
  createSignal,
  type ParentComponent,
} from "solid-js";
import { api, errorStatus } from "@/lib/api";
import { authState, setAuthState } from "@/lib/auth";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/**
 * 最外殻のアクセスゲート。/api/auth/status（公開）で
 * 「初回セットアップ / ログイン / 認証済み」の3状態に振り分ける。
 * セッションは HttpOnly Cookie なので JS はパスワードもトークンも保持しない。
 */
const LoginGate: ParentComponent = (props) => {
  const [status, { refetch }] = createResource(() => api.getAuthStatus());

  createEffect(() => {
    const s = status();
    if (!s) return;
    setAuthState(s.setup_required ? "setup" : s.authenticated ? "authed" : "login");
  });

  return (
    <Switch
      fallback={
        // unknown: status 取得中（一瞬）。まだ何も描画しない。
        <div class="min-h-dvh bg-background" />
      }
    >
      <Match when={status.error !== undefined}>
        <Shell title="接続エラー">
          <p class="mb-4 text-xs text-muted-foreground">
            サーバーに接続できませんでした。
          </p>
          <Button class="w-full" onClick={() => void refetch()}>
            再試行
          </Button>
        </Shell>
      </Match>
      <Match when={authState() === "authed"}>{props.children}</Match>
      <Match when={authState() === "setup"}>
        <SetupForm onDone={() => setAuthState("authed")} />
      </Match>
      <Match when={authState() === "login"}>
        <LoginForm
          onDone={() => setAuthState("authed")}
          onSetupRequired={() => void refetch()}
        />
      </Match>
    </Switch>
  );
};

/** フルスクリーン中央寄せの共通シェル。 */
const Shell: ParentComponent<{ title: string }> = (props) => (
  <div class="flex min-h-dvh items-center justify-center bg-background p-4">
    <Card class="w-full max-w-sm p-6">
      <h1 class="mb-1 text-lg font-semibold">{props.title}</h1>
      {props.children}
    </Card>
  </div>
);

/** 初回セットアップ: パスワードを決めて即ログイン。 */
function SetupForm(props: { onDone: () => void }) {
  const [password, setPassword] = createSignal("");
  const [confirm, setConfirm] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);

  const submit = async (e: Event) => {
    e.preventDefault();
    if (password() !== confirm()) {
      setError("パスワードが一致しません");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.setupPassword(password());
      props.onDone();
    } catch (err) {
      setError(
        errorStatus(err) === 409
          ? "すでに設定済みです。再読み込みしてください。"
          : "パスワードは8文字以上128文字以下で設定してください",
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <Shell title="初回セットアップ">
      <p class="mb-4 text-xs text-muted-foreground">
        このリーダーを保護するパスワードを設定してください（8文字以上）。
      </p>
      <form onSubmit={submit} class="space-y-3">
        <Input
          type="password"
          autocomplete="new-password"
          placeholder="新しいパスワード"
          value={password()}
          onInput={(e) => setPassword(e.currentTarget.value)}
        />
        <Input
          type="password"
          autocomplete="new-password"
          placeholder="パスワード（確認）"
          value={confirm()}
          onInput={(e) => setConfirm(e.currentTarget.value)}
        />
        <Show when={error()}>
          {(m) => <p class="text-xs text-destructive">{m()}</p>}
        </Show>
        <Button
          type="submit"
          class="w-full"
          disabled={busy() || password().length < 8 || !confirm()}
        >
          {busy() ? "設定中..." : "設定してはじめる"}
        </Button>
      </form>
    </Shell>
  );
}

/** ログインフォーム。失敗理由は出し分けない（バックオフ中の 429 だけ案内）。 */
function LoginForm(props: { onDone: () => void; onSetupRequired: () => void }) {
  const [password, setPassword] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);

  const submit = async (e: Event) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.login(password());
      setPassword("");
      props.onDone();
    } catch (err) {
      const code = errorStatus(err);
      if (code === 429) {
        setError("試行回数が多すぎます。しばらく待ってから再試行してください。");
      } else if (code === 409) {
        // credential 未設定（リセット直後など）→ セットアップへ。
        props.onSetupRequired();
      } else {
        setError("パスワードが正しくありません");
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <Shell title="サインイン">
      <p class="mb-4 text-xs text-muted-foreground">パスワードを入力してください。</p>
      <form onSubmit={submit} class="space-y-3">
        <Input
          type="password"
          autocomplete="current-password"
          placeholder="パスワード"
          value={password()}
          onInput={(e) => setPassword(e.currentTarget.value)}
        />
        <Show when={error()}>
          {(m) => <p class="text-xs text-destructive">{m()}</p>}
        </Show>
        <Button type="submit" class="w-full" disabled={busy() || !password()}>
          {busy() ? "確認中..." : "サインイン"}
        </Button>
      </form>
    </Shell>
  );
}

export default LoginGate;
