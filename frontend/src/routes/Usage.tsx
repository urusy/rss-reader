import { createResource, createSignal, For, Show } from "solid-js";
import { api } from "@/lib/api";
import {
  bucketForDays,
  cacheHitRate,
  featureLabel,
  fillBuckets,
  formatTokens,
  purposeLabel,
  totalsByFeature,
  ttsSourceLabel,
} from "@/lib/usage-format";
import { estimateCostUsd, formatUsd } from "@/lib/llm-cost";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

const PERIODS = [
  { days: 7, label: "7日" },
  { days: 30, label: "30日" },
  { days: 90, label: "90日" },
  { days: 365, label: "1年" },
] as const;

/**
 * 利用状況ページ。どの機能をいつ・どれだけ使ったか、LLM のトークン消費と
 * 概算コスト、読み上げの内訳を可視化する。チャートは既存の div バー方式
 * （ListenBar のプログレスバーと同型）で、チャートライブラリは使わない。
 */
export default function Usage() {
  const [days, setDays] = createSignal<number>(30);
  const [summary] = createResource(days, (d) =>
    api.getUsageSummary(d, bucketForDays(d)),
  );

  const isEmpty = () => {
    const s = summary();
    return (
      s !== undefined &&
      s.buckets.length === 0 &&
      s.llm.length === 0 &&
      s.tts_sources.length === 0
    );
  };

  const activity = () => {
    const s = summary();
    if (!s) return [];
    return fillBuckets(s.buckets, days(), bucketForDays(days()));
  };
  const maxActivity = () => Math.max(1, ...activity().map((b) => b.total));

  const features = () => totalsByFeature(summary()?.buckets ?? []);
  const maxFeature = () => Math.max(1, ...features().map((f) => f.count));

  const cost = () => estimateCostUsd(summary()?.llm ?? []);
  const hitRate = () => {
    const s = summary();
    return s ? cacheHitRate(s.buckets, s.llm) : null;
  };

  const ttsTotal = () =>
    (summary()?.tts_sources ?? []).reduce((sum, r) => sum + r.count, 0);

  const dateLabel = (d: Date) => `${d.getMonth() + 1}/${d.getDate()}`;

  return (
    <div class="mx-auto max-w-3xl space-y-4 px-4 py-6 pb-safe">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <h1 class="text-2xl font-bold tracking-tight">利用状況</h1>
        {/* 期間セレクタ（自前 Tailwind のボタングループ） */}
        <div class="flex rounded-md border border-border" role="group" aria-label="期間">
          <For each={PERIODS}>
            {(p) => (
              <button
                type="button"
                aria-pressed={days() === p.days}
                onClick={() => setDays(p.days)}
                class="px-3 py-1.5 text-sm first:rounded-l-md last:rounded-r-md pointer-coarse:min-h-11"
                classList={{
                  "bg-primary text-primary-foreground": days() === p.days,
                  "hover:bg-accent": days() !== p.days,
                }}
              >
                {p.label}
              </button>
            )}
          </For>
        </div>
      </div>

      <Show when={summary.error}>
        <p class="text-sm text-destructive">読み込みに失敗しました: {String(summary.error)}</p>
      </Show>

      <Show when={isEmpty()}>
        <Card>
          <CardContent class="py-10 text-center text-sm text-muted-foreground">
            <p>まだ記録がありません</p>
            <p class="mt-1">
              記事を読んだり要約・検索を使うと、ここに利用状況が表示されます。
            </p>
          </CardContent>
        </Card>
      </Show>

      <Show when={summary() && !isEmpty()}>
        {/* アクティビティ（バケット別の総イベント数） */}
        <Card>
          <CardHeader>
            <CardTitle>アクティビティ</CardTitle>
          </CardHeader>
          <CardContent>
            <div class="flex h-28 items-end gap-px" role="img" aria-label="期間内の利用回数の推移">
              <For each={activity()}>
                {(b) => (
                  <div
                    class="group relative flex-1 rounded-t bg-primary/80 transition-colors hover:bg-primary"
                    style={{ height: `${Math.max(2, (b.total / maxActivity()) * 100)}%` }}
                    title={`${dateLabel(b.start)}: ${b.total}件`}
                  />
                )}
              </For>
            </div>
            <div class="mt-1 flex justify-between text-xs text-muted-foreground">
              <span>{activity().length > 0 ? dateLabel(activity()[0].start) : ""}</span>
              <span>
                {activity().length > 0
                  ? dateLabel(activity()[activity().length - 1].start)
                  : ""}
              </span>
            </div>
          </CardContent>
        </Card>

        {/* 機能別利用回数（成功のみ・降順横バー） */}
        <Show when={features().length > 0}>
          <Card>
            <CardHeader>
              <CardTitle>機能別の利用回数</CardTitle>
            </CardHeader>
            <CardContent class="space-y-2">
              <For each={features()}>
                {(f) => (
                  <div class="flex items-center gap-2 text-sm">
                    <span class="w-36 shrink-0 truncate" title={f.feature}>
                      {featureLabel(f.feature)}
                    </span>
                    <div class="h-2.5 flex-1 overflow-hidden rounded-full bg-muted">
                      <div
                        class="h-full rounded-full bg-primary"
                        style={{ width: `${(f.count / maxFeature()) * 100}%` }}
                      />
                    </div>
                    <span class="w-12 shrink-0 text-right tabular-nums text-muted-foreground">
                      {f.count}
                    </span>
                  </div>
                )}
              </For>
            </CardContent>
          </Card>
        </Show>

        {/* LLM 利用（実呼び出しのみ。キャッシュヒットは含まない） */}
        <Show when={(summary()?.llm.length ?? 0) > 0}>
          <Card>
            <CardHeader>
              <CardTitle>AI（Claude）の利用</CardTitle>
            </CardHeader>
            <CardContent class="space-y-3">
              <div class="overflow-x-auto">
                <table class="w-full text-sm">
                  <thead>
                    <tr class="border-b border-border text-left text-xs text-muted-foreground">
                      <th class="py-1.5 pr-2 font-medium">用途</th>
                      <th class="py-1.5 pr-2 font-medium">モデル</th>
                      <th class="py-1.5 pr-2 text-right font-medium">回数</th>
                      <th class="py-1.5 pr-2 text-right font-medium">入力</th>
                      <th class="py-1.5 text-right font-medium">出力</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={summary()?.llm}>
                      {(r) => (
                        <tr class="border-b border-border/50 last:border-0">
                          <td class="py-1.5 pr-2">{purposeLabel(r.purpose)}</td>
                          <td class="py-1.5 pr-2 text-muted-foreground">{r.model}</td>
                          <td class="py-1.5 pr-2 text-right tabular-nums">{r.calls}</td>
                          <td class="py-1.5 pr-2 text-right tabular-nums">
                            {formatTokens(r.input_tokens)}
                          </td>
                          <td class="py-1.5 text-right tabular-nums">
                            {formatTokens(r.output_tokens)}
                          </td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>
              <div class="flex flex-wrap gap-x-6 gap-y-1 text-sm">
                <Show when={cost()}>
                  {(c) => (
                    <span>
                      概算コスト:{" "}
                      <span class="font-semibold tabular-nums">{formatUsd(c().usd)}</span>
                      <Show when={c().hasUnknownModel}>
                        <span class="text-muted-foreground">（未知モデル分を除く）</span>
                      </Show>
                    </span>
                  )}
                </Show>
                <Show when={hitRate() !== null}>
                  <span>
                    キャッシュ節約率:{" "}
                    <span class="font-semibold tabular-nums">{hitRate()}%</span>
                  </span>
                </Show>
              </div>
              <p class="text-xs text-muted-foreground">
                コストは価格表による概算です。実際の請求は Anthropic Console を確認してください。
              </p>
            </CardContent>
          </Card>
        </Show>

        {/* 読み上げ（クライアント申告 tts_play の内訳） */}
        <Show when={(summary()?.tts_sources.length ?? 0) > 0}>
          <Card>
            <CardHeader>
              <CardTitle>読み上げの内訳</CardTitle>
            </CardHeader>
            <CardContent class="space-y-2">
              <For each={summary()?.tts_sources}>
                {(r) => (
                  <div class="flex items-center gap-2 text-sm">
                    <span class="w-36 shrink-0">{ttsSourceLabel(r.source)}</span>
                    <div class="h-2.5 flex-1 overflow-hidden rounded-full bg-muted">
                      <div
                        class="h-full rounded-full bg-primary"
                        style={{ width: `${(r.count / Math.max(1, ttsTotal())) * 100}%` }}
                      />
                    </div>
                    <span class="w-12 shrink-0 text-right tabular-nums text-muted-foreground">
                      {r.count}
                    </span>
                  </div>
                )}
              </For>
            </CardContent>
          </Card>
        </Show>
      </Show>
    </div>
  );
}
