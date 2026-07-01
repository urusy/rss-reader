import { createEffect, createResource, createSignal, For, Show } from "solid-js";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

// 素の select/input/textarea に当てる共通クラス（既存 MuteRulesManager 等と揃える）。
const FIELD_CLASS =
  "h-9 w-full rounded-md border border-border bg-background px-2 text-sm";
const TEXTAREA_CLASS =
  "min-h-24 w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm";

// プリセット。将来のモデルは「カスタム…」で自由入力できる。value="" は既定(env)。
const MODEL_OPTIONS: { value: string; label: string }[] = [
  { value: "", label: "既定 (環境変数)" },
  { value: "claude-opus-4-8", label: "Claude Opus 4.8" },
  { value: "claude-sonnet-4-6", label: "Claude Sonnet 4.6" },
  { value: "claude-haiku-4-5-20251001", label: "Claude Haiku 4.5" },
];
const PRESET_VALUES = MODEL_OPTIONS.map((o) => o.value);

/**
 * 要約/翻訳のモデル・プロンプトを設定するカード（#llm_settings）。
 * モデル「既定」= env の ANTHROPIC_MODEL、プロンプト空欄 = 組込み既定。
 * テンプレート内の {lang} は対象言語に置換される。
 */
export default function LlmSettingsCard() {
  const [settings, { mutate }] = createResource(() => api.getLlmSettings());

  const [sModel, setSModel] = createSignal("");
  const [sCustom, setSCustom] = createSignal(false);
  const [sPrompt, setSPrompt] = createSignal("");
  const [tModel, setTModel] = createSignal("");
  const [tCustom, setTCustom] = createSignal(false);
  const [tPrompt, setTPrompt] = createSignal("");

  const [busy, setBusy] = createSignal(false);
  const [saved, setSaved] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  // 入力を変えたら「保存しました ✓」を消す（未保存を保存済みと誤表示しないため）。
  const touch = () => setSaved(false);

  // サーバ値が届いたら一度だけ入力へ反映（以後はユーザー編集を優先）。
  let hydrated = false;
  createEffect(() => {
    const s = settings();
    if (!s || hydrated) return;
    hydrated = true;
    const sm = s.summarize_model ?? "";
    setSModel(sm);
    setSCustom(sm !== "" && !PRESET_VALUES.includes(sm));
    setSPrompt(s.summarize_prompt ?? "");
    const tm = s.translate_model ?? "";
    setTModel(tm);
    setTCustom(tm !== "" && !PRESET_VALUES.includes(tm));
    setTPrompt(s.translate_prompt ?? "");
  });

  const save = async () => {
    setBusy(true);
    setError(null);
    setSaved(false);
    try {
      // 空文字はバックエンドが override 解除（NULL）に正規化する。
      const next = await api.updateLlmSettings({
        summarize_model: sModel(),
        summarize_prompt: sPrompt(),
        translate_model: tModel(),
        translate_prompt: tPrompt(),
      });
      mutate(next);
      setSaved(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>AI 要約・翻訳</CardTitle>
      </CardHeader>
      <CardContent class="space-y-4">
        <p class="text-xs text-muted-foreground">
          要約・翻訳に使う Claude モデルと system プロンプトを、それぞれ個別に指定できます。
          モデルを「既定」にすると環境変数 <code>ANTHROPIC_MODEL</code>（
          {settings()?.default_model ?? "—"}）を使います。プロンプトを空欄にすると組込みの既定に戻ります。
          テンプレート内の <code>{"{lang}"}</code> は対象言語（例: ja）に置換されます。
        </p>

        {/* 読み込み完了までフォームを描画しない：ハイドレーションが編集中の入力を
            上書きする競合を防ぐ（入力は初回反映後にのみ操作可能）。 */}
        <Show
          when={!settings.loading}
          fallback={<p class="text-sm text-muted-foreground">読み込み中…</p>}
        >
        <Show when={error()}>
          <p class="text-sm text-destructive">{error()}</p>
        </Show>

        {/* 要約 */}
        <div class="space-y-2">
          <h3 class="text-sm font-semibold">要約</h3>
          <div class="space-y-1">
            <label class="text-xs text-muted-foreground">モデル</label>
            <select
              class={FIELD_CLASS}
              value={sCustom() ? "__custom__" : sModel()}
              onChange={(e) => {
                touch();
                const v = e.currentTarget.value;
                if (v === "__custom__") {
                  setSCustom(true);
                  setSModel("");
                } else {
                  setSCustom(false);
                  setSModel(v);
                }
              }}
            >
              <For each={MODEL_OPTIONS}>
                {(o) => <option value={o.value}>{o.label}</option>}
              </For>
              <option value="__custom__">カスタム…</option>
            </select>
            <Show when={sCustom()}>
              <input
                class={FIELD_CLASS}
                placeholder="モデル id（例: claude-opus-4-8）"
                value={sModel()}
                onInput={(e) => {
                  touch();
                  setSModel(e.currentTarget.value);
                }}
              />
            </Show>
          </div>
          <div class="space-y-1">
            <label class="text-xs text-muted-foreground">プロンプト（空欄で既定）</label>
            <textarea
              class={TEXTAREA_CLASS}
              rows={3}
              placeholder={settings()?.default_summarize_prompt}
              value={sPrompt()}
              onInput={(e) => {
                touch();
                setSPrompt(e.currentTarget.value);
              }}
            />
          </div>
        </div>

        {/* 翻訳 */}
        <div class="space-y-2 border-t border-border pt-3">
          <h3 class="text-sm font-semibold">翻訳</h3>
          <div class="space-y-1">
            <label class="text-xs text-muted-foreground">モデル</label>
            <select
              class={FIELD_CLASS}
              value={tCustom() ? "__custom__" : tModel()}
              onChange={(e) => {
                touch();
                const v = e.currentTarget.value;
                if (v === "__custom__") {
                  setTCustom(true);
                  setTModel("");
                } else {
                  setTCustom(false);
                  setTModel(v);
                }
              }}
            >
              <For each={MODEL_OPTIONS}>
                {(o) => <option value={o.value}>{o.label}</option>}
              </For>
              <option value="__custom__">カスタム…</option>
            </select>
            <Show when={tCustom()}>
              <input
                class={FIELD_CLASS}
                placeholder="モデル id（例: claude-opus-4-8）"
                value={tModel()}
                onInput={(e) => {
                  touch();
                  setTModel(e.currentTarget.value);
                }}
              />
            </Show>
          </div>
          <div class="space-y-1">
            <label class="text-xs text-muted-foreground">プロンプト（空欄で既定）</label>
            <textarea
              class={TEXTAREA_CLASS}
              rows={3}
              placeholder={settings()?.default_translate_prompt}
              value={tPrompt()}
              onInput={(e) => {
                touch();
                setTPrompt(e.currentTarget.value);
              }}
            />
          </div>
        </div>

        <div class="flex items-center gap-2">
          <Button onClick={save} disabled={busy() || settings.loading}>
            {busy() ? "保存中…" : "保存"}
          </Button>
          <Show when={saved()}>
            <span class="text-sm text-muted-foreground">保存しました ✓</span>
          </Show>
        </div>
        </Show>
      </CardContent>
    </Card>
  );
}
