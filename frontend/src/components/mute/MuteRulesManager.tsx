import { createResource, createSignal, For, Show } from "solid-js";
import { api, type MuteRule } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";

const FIELD_LABEL: Record<MuteRule["field"], string> = {
  title: "タイトル",
  content: "本文",
  url: "URL（ドメイン）",
};
const ACTION_LABEL: Record<MuteRule["action"], string> = {
  hide: "非表示",
  mark_read: "既読化",
};

export default function MuteRulesManager() {
  const [rules, { refetch }] = createResource(() => api.listMuteRules());
  const [field, setField] = createSignal<MuteRule["field"]>("title");
  const [pattern, setPattern] = createSignal("");
  const [action, setAction] = createSignal<MuteRule["action"]>("hide");
  const [busy, setBusy] = createSignal(false);
  const [err, setErr] = createSignal<string | null>(null);

  const add = async (e: Event) => {
    e.preventDefault();
    if (!pattern().trim()) return;
    setBusy(true);
    setErr(null);
    try {
      await api.createMuteRule({
        field: field(),
        pattern: pattern().trim(),
        action: action(),
      });
      setPattern("");
      await refetch();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const toggle = async (r: MuteRule) => {
    await api.updateMuteRule(r.id, { enabled: !r.enabled });
    await refetch();
  };

  const remove = async (r: MuteRule) => {
    await api.deleteMuteRule(r.id);
    await refetch();
  };

  return (
    <section class="space-y-3">
      <form class="flex flex-wrap items-center gap-2" onSubmit={add}>
        <select
          class="h-9 rounded-md border border-border bg-background px-2 text-sm"
          value={field()}
          onChange={(e) => setField(e.currentTarget.value as MuteRule["field"])}
        >
          <option value="title">タイトル</option>
          <option value="content">本文</option>
          <option value="url">URL（ドメイン）</option>
        </select>
        <Input
          class="min-w-40 flex-1"
          placeholder="NGワード（部分一致）"
          value={pattern()}
          onInput={(e) => setPattern(e.currentTarget.value)}
        />
        <select
          class="h-9 rounded-md border border-border bg-background px-2 text-sm"
          value={action()}
          onChange={(e) => setAction(e.currentTarget.value as MuteRule["action"])}
        >
          <option value="hide">非表示</option>
          <option value="mark_read">既読化</option>
        </select>
        <Button type="submit" disabled={busy() || !pattern().trim()}>
          追加
        </Button>
      </form>
      <Show when={err()}>
        <p class="text-xs text-destructive">{err()}</p>
      </Show>

      <Show
        when={(rules()?.length ?? 0) > 0}
        fallback={<p class="text-sm text-muted-foreground">ルールはありません。</p>}
      >
        <ul class="divide-y divide-border">
          <For each={rules()}>
            {(r) => (
              <li class="flex items-center justify-between gap-3 py-2">
                <div class="flex min-w-0 items-center gap-2">
                  <Badge>{FIELD_LABEL[r.field]}</Badge>
                  <span class="truncate text-sm font-medium">{r.pattern}</span>
                  <span class="text-xs text-muted-foreground">
                    {ACTION_LABEL[r.action]}
                  </span>
                </div>
                <div class="flex items-center gap-2">
                  <Switch
                    checked={r.enabled}
                    onCheckedChange={() => void toggle(r)}
                  />
                  <Button variant="ghost" onClick={() => void remove(r)}>
                    削除
                  </Button>
                </div>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </section>
  );
}
