import { createSignal, onMount, Show } from "solid-js";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import { api, errorStatus } from "@/lib/api";
import { disablePush, enablePush, isSubscribed, pushSupported } from "@/lib/push";

/** #31 Web Push 通知の設定カード。この端末の購読 on/off とテスト送信。 */
export default function PushSettingsCard() {
  const supported = pushSupported();
  const [subscribed, setSubscribed] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [info, setInfo] = createSignal<string | null>(null);

  onMount(async () => {
    if (!supported) return;
    setSubscribed(await isSubscribed().catch(() => false));
  });

  const notEnabledMsg = "サーバで Web Push が未設定です（VAPID 鍵が未登録）。";

  const onToggle = async (checked: boolean) => {
    setBusy(true);
    setError(null);
    setInfo(null);
    try {
      if (checked) {
        const ok = await enablePush();
        setSubscribed(ok);
        if (!ok) {
          setError("通知が許可されませんでした。ブラウザの通知設定をご確認ください。");
        }
      } else {
        await disablePush();
        setSubscribed(false);
      }
    } catch (e) {
      setError(errorStatus(e) === 503 ? notEnabledMsg : String(e));
    } finally {
      setBusy(false);
    }
  };

  const sendTest = async () => {
    setBusy(true);
    setError(null);
    setInfo(null);
    try {
      const { delivered } = await api.testPush();
      setInfo(`テスト通知を ${delivered} 件の購読へ送信しました。`);
    } catch (e) {
      setError(errorStatus(e) === 503 ? notEnabledMsg : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>
          <div class="flex items-center gap-2">
            プッシュ通知
            <Show when={supported} fallback={<Badge>非対応</Badge>}>
              <Show when={subscribed()} fallback={<Badge>未購読</Badge>}>
                <Badge variant="unread">購読中</Badge>
              </Show>
            </Show>
          </div>
        </CardTitle>
      </CardHeader>
      <CardContent class="space-y-3">
        <p class="text-xs text-muted-foreground">
          優先度「高」のフィードに新着が入ったとき、この端末へ通知します。iOS はホーム画面に追加した
          PWA でのみ利用できます（iOS 16.4 以降）。
        </p>

        <Show when={!supported}>
          <p class="text-sm text-muted-foreground">
            このブラウザは Web Push に対応していません。
          </p>
        </Show>

        <Show when={error()}>
          <p class="text-sm text-destructive">{error()}</p>
        </Show>
        <Show when={info()}>
          <p class="text-sm text-muted-foreground">{info()}</p>
        </Show>

        <Show when={supported}>
          <div class="flex items-start justify-between gap-3 border-t border-border pt-3">
            <div>
              <p class="text-sm">この端末で通知を受け取る</p>
              <p class="text-xs text-muted-foreground">
                通知を許可し、この端末を購読に登録します。
              </p>
            </div>
            <Switch
              checked={subscribed()}
              disabled={busy()}
              onCheckedChange={(d) => void onToggle(d.checked)}
            />
          </div>

          <Show when={subscribed()}>
            <Button variant="ghost" onClick={sendTest} disabled={busy()}>
              テスト通知を送る
            </Button>
          </Show>
        </Show>
      </CardContent>
    </Card>
  );
}
