import { createSignal, For, Show } from "solid-js";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { BUILTIN_DICT } from "@/lib/tts-dict";
import {
  userDict,
  addEntry,
  updateEntry,
  removeEntry,
} from "@/lib/tts-dict-store";

/**
 * 読み上げ発音辞書の編集カード（#33 精度改善）。
 * 英字略語/英単語の誤読（AI→「あい」等）を、カタカナ読みへ置換して補正する辞書を
 * ユーザーが追加/編集/削除できる。永続化は tts-dict-store（localStorage "tts-dict"）。
 * 組み込み辞書は参照表示のみ。同じ表記をユーザーが足すと組み込みを上書きする。
 */
export default function TtsDictCard() {
  const [match, setMatch] = createSignal("");
  const [reading, setReading] = createSignal("");
  // 大小を区別（略語向け）。既定 true＝"AI" のような大文字綴りだけを拾う。
  const [caseSensitive, setCaseSensitive] = createSignal(true);

  const add = (e: Event) => {
    e.preventDefault();
    const m = match().trim();
    const r = reading().trim();
    if (!m || !r) return;
    addEntry({ match: m, reading: r, caseSensitive: caseSensitive() });
    setMatch("");
    setReading("");
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>読み上げ辞書</CardTitle>
      </CardHeader>
      <CardContent class="space-y-4">
        <p class="text-xs text-muted-foreground">
          読み上げ機能が英字を誤読する語（例:「AI」→「あい」）を、カタカナ読みに置き換えます。
          表示される文章は変わらず、音声だけが補正されます。組み込み辞書と同じ表記を追加すると、
          あなたの読みが優先されます。
        </p>

        <form class="flex flex-wrap items-end gap-2" onSubmit={add}>
          <label class="flex flex-col gap-1 text-xs text-muted-foreground">
            表記
            <Input
              class="w-32"
              placeholder="AI"
              value={match()}
              onInput={(e) => setMatch(e.currentTarget.value)}
            />
          </label>
          <label class="flex flex-col gap-1 text-xs text-muted-foreground">
            読み（カタカナ）
            <Input
              class="min-w-40 flex-1"
              placeholder="エーアイ"
              value={reading()}
              onInput={(e) => setReading(e.currentTarget.value)}
            />
          </label>
          <label class="flex items-center gap-1 pb-2 text-xs text-muted-foreground">
            <input
              type="checkbox"
              checked={caseSensitive()}
              onChange={(e) => setCaseSensitive(e.currentTarget.checked)}
            />
            大文字/小文字を区別
          </label>
          <Button type="submit" disabled={!match().trim() || !reading().trim()}>
            追加
          </Button>
        </form>

        {/* ユーザー辞書（編集可能） */}
        <div class="space-y-1">
          <h3 class="text-xs font-semibold text-muted-foreground">
            あなたの辞書
          </h3>
          <Show
            when={userDict().length > 0}
            fallback={
              <p class="text-sm text-muted-foreground">
                まだ登録された語はありません。
              </p>
            }
          >
            <ul class="divide-y divide-border">
              <For each={userDict()}>
                {(entry, i) => (
                  <li class="flex items-center justify-between gap-3 py-2">
                    <div class="flex min-w-0 flex-1 items-center gap-2">
                      <span class="w-28 shrink-0 truncate text-sm font-medium">
                        {entry.match}
                      </span>
                      <span class="text-muted-foreground">→</span>
                      <Input
                        class="min-w-32 flex-1"
                        value={entry.reading}
                        onChange={(e) =>
                          updateEntry(i(), { reading: e.currentTarget.value })
                        }
                      />
                      <Show when={!entry.caseSensitive}>
                        <Badge>大小無視</Badge>
                      </Show>
                    </div>
                    <Button variant="ghost" onClick={() => removeEntry(i())}>
                      削除
                    </Button>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </div>

        {/* 組み込み辞書（参照のみ） */}
        <details class="text-sm">
          <summary class="cursor-pointer text-xs font-semibold text-muted-foreground">
            組み込み辞書（{BUILTIN_DICT.length} 語・参照のみ）
          </summary>
          <ul class="mt-2 grid grid-cols-2 gap-x-4 gap-y-1 sm:grid-cols-3">
            <For each={BUILTIN_DICT}>
              {(entry) => (
                <li class="flex items-center gap-1 truncate text-xs">
                  <span class="font-medium">{entry.match}</span>
                  <span class="text-muted-foreground">→ {entry.reading}</span>
                </li>
              )}
            </For>
          </ul>
        </details>
      </CardContent>
    </Card>
  );
}
