import { createResource, createSignal, For, Show } from "solid-js";
import {
  api,
  type Combinator,
  type Condition,
  type KeywordTarget,
  type Rule,
  type RuleAction,
} from "@/lib/api";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";

const sel = "h-9 rounded-md border border-border bg-background px-2 text-sm";

/** カスタムルールエンジン（#28）。キーワード条件 + アクションの If/Then 自動化。 */
export default function Rules() {
  const [rules, { refetch }] = createResource(() => api.listRules());
  const [name, setName] = createSignal("");
  const [combinator, setCombinator] = createSignal<Combinator>("all");
  const [conds, setConds] = createSignal<
    { target: KeywordTarget; value: string }[]
  >([{ target: "any", value: "" }]);
  const [actions, setActions] = createSignal<RuleAction[]>([
    { kind: "mark_read" },
  ]);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [note, setNote] = createSignal<string | null>(null);

  const addCond = () =>
    setConds((c) => [...c, { target: "any", value: "" }]);
  const addAction = () => setActions((a) => [...a, { kind: "mark_read" }]);

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const items: Condition[] = conds()
        .filter((c) => c.value.trim())
        .map((c) => ({
          field: "keyword",
          target: c.target,
          value: c.value.trim(),
        }));
      await api.createRule({
        name: name().trim(),
        conditions: { combinator: combinator(), items },
        actions: actions(),
      });
      setName("");
      setConds([{ target: "any", value: "" }]);
      setActions([{ kind: "mark_read" }]);
      await refetch();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const toggle = async (r: Rule) => {
    await api.updateRule(r.id, {
      name: r.name,
      enabled: !r.enabled,
      position: r.position,
      conditions: r.conditions,
      actions: r.actions,
    });
    await refetch();
  };
  const remove = async (r: Rule) => {
    await api.deleteRule(r.id);
    await refetch();
  };
  const test = async (r: Rule) => {
    const res = await api.testRule(r.id);
    setNote(`「${r.name}」は直近記事 ${res.matched_count} 件に一致します`);
  };
  const applyAll = async () => {
    const res = await api.applyRules();
    setNote(`${res.processed} 件の記事にルールを再適用しました`);
    await refetch();
  };

  const actionLabel = (a: RuleAction) =>
    a.kind === "mark_read"
      ? "既読化"
      : a.kind === "star"
        ? "スター"
        : a.kind === "save"
          ? "後で読む"
          : a.kind === "tag"
            ? `タグ:${a.name}`
            : `スコア${a.delta > 0 ? "+" : ""}${a.delta}`;

  return (
    <div class="mx-auto max-w-3xl space-y-4 px-4 py-6">
      <div class="flex items-center justify-between gap-2">
        <h1 class="text-2xl font-bold tracking-tight">自動ルール</h1>
        <Button variant="outline" onClick={applyAll}>
          すべて再適用
        </Button>
      </div>

      <Show when={note()}>
        <p class="text-sm text-muted-foreground">{note()}</p>
      </Show>

      <Card>
        <CardHeader>
          <CardTitle>新しいルール</CardTitle>
        </CardHeader>
        <CardContent class="space-y-3">
          <Input
            placeholder="ルール名（例: 広告を既読に）"
            value={name()}
            onInput={(e) => setName(e.currentTarget.value)}
          />
          <div class="flex items-center gap-2 text-sm">
            条件の結合:
            <select
              class={sel}
              value={combinator()}
              onChange={(e) => setCombinator(e.currentTarget.value as Combinator)}
            >
              <option value="all">すべて一致 (AND)</option>
              <option value="any">いずれか一致 (OR)</option>
            </select>
          </div>

          <div class="space-y-2">
            <For each={conds()}>
              {(c, i) => (
                <div class="flex items-center gap-2">
                  <select
                    class={sel}
                    value={c.target}
                    onChange={(e) =>
                      setConds((arr) =>
                        arr.map((x, j) =>
                          j === i()
                            ? { ...x, target: e.currentTarget.value as KeywordTarget }
                            : x,
                        ),
                      )
                    }
                  >
                    <option value="any">タイトル/本文</option>
                    <option value="title">タイトル</option>
                    <option value="content">本文</option>
                  </select>
                  <Input
                    class="flex-1"
                    placeholder="含む語"
                    value={c.value}
                    onInput={(e) =>
                      setConds((arr) =>
                        arr.map((x, j) =>
                          j === i() ? { ...x, value: e.currentTarget.value } : x,
                        ),
                      )
                    }
                  />
                  <button
                    type="button"
                    class="px-1 text-muted-foreground hover:text-destructive"
                    onClick={() =>
                      setConds((arr) => arr.filter((_, j) => j !== i()))
                    }
                  >
                    ×
                  </button>
                </div>
              )}
            </For>
            <Button size="sm" variant="ghost" onClick={addCond}>
              ＋ 条件を追加
            </Button>
          </div>

          <div class="space-y-2">
            <For each={actions()}>
              {(a, i) => (
                <div class="flex items-center gap-2">
                  <select
                    class={sel}
                    value={a.kind}
                    onChange={(e) => {
                      const k = e.currentTarget.value;
                      setActions((arr) =>
                        arr.map((x, j) => {
                          if (j !== i()) return x;
                          if (k === "tag") return { kind: "tag", name: "" };
                          if (k === "score") return { kind: "score", delta: 1 };
                          return { kind: k } as RuleAction;
                        }),
                      );
                    }}
                  >
                    <option value="mark_read">既読化</option>
                    <option value="tag">タグ付与</option>
                    <option value="score">スコア加減</option>
                    <option value="save">後で読む</option>
                    <option value="star">スター</option>
                  </select>
                  <Show when={a.kind === "tag"}>
                    <Input
                      class="flex-1"
                      placeholder="タグ名"
                      value={a.kind === "tag" ? a.name : ""}
                      onInput={(e) =>
                        setActions((arr) =>
                          arr.map((x, j) =>
                            j === i()
                              ? { kind: "tag", name: e.currentTarget.value }
                              : x,
                          ),
                        )
                      }
                    />
                  </Show>
                  <Show when={a.kind === "score"}>
                    <Input
                      class="w-24"
                      type="number"
                      value={a.kind === "score" ? String(a.delta) : "1"}
                      onInput={(e) =>
                        setActions((arr) =>
                          arr.map((x, j) =>
                            j === i()
                              ? {
                                  kind: "score",
                                  delta: Number(e.currentTarget.value) || 0,
                                }
                              : x,
                          ),
                        )
                      }
                    />
                  </Show>
                  <button
                    type="button"
                    class="px-1 text-muted-foreground hover:text-destructive"
                    onClick={() =>
                      setActions((arr) => arr.filter((_, j) => j !== i()))
                    }
                  >
                    ×
                  </button>
                </div>
              )}
            </For>
            <Button size="sm" variant="ghost" onClick={addAction}>
              ＋ アクションを追加
            </Button>
          </div>

          <Show when={error()}>
            <p class="text-sm text-destructive">{error()}</p>
          </Show>
          <Button onClick={save} disabled={busy() || !name().trim()}>
            {busy() ? "保存中…" : "ルールを作成"}
          </Button>
        </CardContent>
      </Card>

      <For each={rules()}>
        {(r) => (
          <Card>
            <CardContent class="flex flex-wrap items-center gap-2 py-3">
              <span class="font-medium">{r.name}</span>
              <Badge variant={r.enabled ? "unread" : undefined}>
                {r.enabled ? "有効" : "無効"}
              </Badge>
              <span class="text-xs text-muted-foreground">
                {r.conditions.combinator === "all" ? "AND" : "OR"} ・{" "}
                {r.conditions.items.length} 条件 →{" "}
                {r.actions.map(actionLabel).join(", ")}
              </span>
              <div class="ml-auto flex gap-1">
                <Button size="sm" variant="ghost" onClick={() => void test(r)}>
                  テスト
                </Button>
                <Button size="sm" variant="ghost" onClick={() => void toggle(r)}>
                  {r.enabled ? "無効化" : "有効化"}
                </Button>
                <Button size="sm" variant="ghost" onClick={() => void remove(r)}>
                  削除
                </Button>
              </div>
            </CardContent>
          </Card>
        )}
      </For>
    </div>
  );
}
