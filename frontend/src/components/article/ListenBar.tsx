import {
  createSignal,
  createEffect,
  onCleanup,
  onMount,
  For,
  Show,
} from "solid-js";
import {
  createTtsController,
  loadVoices,
  pickBestJaVoice,
  ttsSupported,
  type TtsController,
  type TtsState,
} from "@/lib/tts";
import { normalizeForTts } from "@/lib/tts-normalize";
import { mergedDict } from "@/lib/tts-dict-store";
import {
  hashText,
  loadTtsPos,
  saveTtsPos,
  clearTtsPos,
} from "@/lib/tts-progress";
import { Button } from "@/components/ui/button";

const RATE_KEY = "tts-rate";
const VOICE_KEY = "tts-voice";
// 読み上げがこの割合に達したら既読化フックを呼ぶ（聴いて消化した、とみなす）。#33
const READ_AT = 0.8;

/** 読み上げ対象の 1 ソース（本文 / 要約 / 翻訳）。text はプレーン化済み。 */
export type ListenSource = {
  key: string;
  label: string;
  text: string;
  marksRead: boolean;
};

/**
 * リッスンモードの再生バー（#33 v1）。1 記事内の複数ソース（本文/要約/翻訳）を
 * セグメントで切替えて読む単一バー。`window.speechSynthesis` はプロセス唯一の
 * グローバルリソースなので、生きている TtsController を常に高々 1 個に保ち、
 * 「同時に 1 つだけ再生」を構造的に保証する。
 * marksRead=true のソース（本文）だけ、進捗 READ_AT で一度 onListened を呼ぶ。
 */
export default function ListenBar(props: {
  articleId: string;
  sources: () => ListenSource[];
  onListened?: () => void;
}) {
  if (!ttsSupported()) return null;

  const [state, setState] = createSignal<TtsState>("idle");
  const [progress, setProgress] = createSignal(0);
  const [voices, setVoices] = createSignal<SpeechSynthesisVoice[]>([]);
  const [rate, setRate] = createSignal(
    Number(localStorage.getItem(RATE_KEY)) || 1,
  );
  const [voiceUri, setVoiceUri] = createSignal(
    localStorage.getItem(VOICE_KEY) ?? "",
  );
  // どのソースを聴くか（記事ローカルの一時状態・既定は本文）。
  const [sourceKey, setSourceKey] = createSignal("body");

  let controller: TtsController | undefined;
  let listened = false;
  let lastChunk = 0; // onChunk で更新。onRate/onVoice の位置維持に使う。

  onMount(() => {
    void loadVoices().then(setVoices);
  });

  // 声が明示選択されていればそれを、未選択（"" ＝おまかせ）なら環境で最良の
  // 日本語音声を自動選択する。OS 既定（Mac の Kyoko 等）の機械的な声を避ける。
  const pickedVoice = () => {
    const uri = voiceUri();
    if (uri) return voices().find((v) => v.voiceURI === uri) ?? null;
    return pickBestJaVoice(voices());
  };

  // 選択中のソース（消えていれば先頭にフォールバック）。
  const current = () =>
    props.sources().find((s) => s.key === sourceKey()) ?? props.sources()[0];

  // 実際に読み上げる（辞書正規化後の）テキスト。位置の len/hash はこれを基準にする
  // ので、辞書変更でチャンク境界がズレても loadTtsPos の照合で自己修復する。
  const spokenText = () => normalizeForTts(current().text, mergedDict());

  const build = () => {
    // 正規化後テキストを一度だけ確定し、chunk 分割と位置保存の len/hash を一致させる。
    const spoken = spokenText();
    const id = props.articleId;
    const src = sourceKey();
    return createTtsController(
      // 表示テキストは変えず音声だけ補正する。進捗は正規化後テキスト基準で一貫。
      spoken,
      { rate: rate(), voice: pickedVoice() },
      {
        onState: setState,
        onProgress: (r) => {
          setProgress(r);
          // 本文（marksRead=true）のみ既読化する。要約/翻訳は既読化しない。
          if (current().marksRead && !listened && r >= READ_AT) {
            listened = true;
            props.onListened?.();
          }
        },
        // 各文の開始で「これから読む文」を先取り保存（dispose 前 flush 不要）。
        onChunk: (i) => {
          lastChunk = i;
          saveTtsPos(id, src, {
            chunk: i,
            len: spoken.length,
            hash: hashText(spoken),
            ratio: progress(),
            t: Date.now(),
          });
        },
        onEnd: () => {
          setProgress(1);
          clearTtsPos(id, src); // 最後まで聴いた → 次回は先頭から。
          lastChunk = 0;
        },
      },
    );
  };

  // 保存位置があれば続きから（無ければ先頭）。play 時に正規化後テキストで再照合し、
  // テキスト/辞書変更を必ず自己修復する（stale な signal は使わない）。
  const play = (fromChunk?: number) => {
    const start =
      fromChunk ??
      loadTtsPos(props.articleId, sourceKey(), spokenText())?.chunk ??
      0;
    controller?.dispose(); // 単一所有: 必ず dispose→build
    controller = build();
    controller.play(start);
  };

  // ソース切替: idle に戻して再生ボタン待ち（位置が変わる別テキストなので auto-play しない）。
  const onSource = (k: string) => {
    if (k === sourceKey()) return; // 再選択ガード（再クリックで停止させない）
    controller?.dispose();
    controller = undefined;
    setSourceKey(k);
    listened = false;
    lastChunk = 0;
    setState("idle");
    setProgress(0); // 切替先の保存位置はマーカー effect が反映する。
  };

  // 記事が切り替わったら（articleId 変化）読み上げを止めて本文起点へリセット。
  // 保存位置の復元は下のマーカー effect に委譲する（progress(0) は後で上書きされる）。
  createEffect((prev: string | undefined) => {
    const id = props.articleId;
    if (prev !== undefined && prev !== id) {
      controller?.dispose();
      controller = undefined;
      listened = false;
      lastChunk = 0;
      setSourceKey("body");
      setProgress(0);
      setState("idle");
    }
    return id;
  });

  // 再生中のソースが消えた（要約/翻訳が削除された）ら本文へフォールバック。
  // setSourceKey("body") を先に置き、直後の onProgress が本文 marksRead=true で
  // 誤既読しない窓を塞ぐ。消えたソースの保存位置も破棄する。
  createEffect(() => {
    const key = sourceKey();
    if (!props.sources().some((s) => s.key === key)) {
      clearTtsPos(props.articleId, key);
      controller?.dispose();
      controller = undefined;
      setSourceKey("body");
      setState("idle");
      setProgress(0);
    }
  });

  // idle のときだけ、保存位置があれば進捗バーにマーカー（ratio）を復元表示する。
  // ⚠ この effect は上の記事切替 effect より「後」に登録すること（同一 articleId
  //   変化バッチで後実行＝マーカーが最終値を勝ち取る）。並べ替え禁止。
  // 保存が無い（null）ときは progress を触らない＝完了時 onEnd の 1 を尊重し、
  // バーが 0 に落ちるフリッカーを防ぐ（切替時の 0 は各 effect が別途設定済み）。
  createEffect(() => {
    props.articleId; // deps
    const cur = current();
    const t = cur ? normalizeForTts(cur.text, mergedDict()) : "";
    if (state() !== "idle") return;
    const p = loadTtsPos(props.articleId, sourceKey(), t);
    if (p) setProgress(p.ratio);
  });

  onCleanup(() => controller?.dispose());

  // 速度/声変更は次回再生から。再生中なら作り直すが、lastChunk で位置は維持する。設定は永続化。
  const onRate = (v: number) => {
    setRate(v);
    localStorage.setItem(RATE_KEY, String(v));
    if (state() !== "idle") play(lastChunk);
  };
  const onVoice = (uri: string) => {
    setVoiceUri(uri);
    localStorage.setItem(VOICE_KEY, uri);
    if (state() !== "idle") play(lastChunk);
  };

  return (
    <section class="flex flex-wrap items-center gap-2 rounded-lg border border-border bg-muted/30 p-2">
      <Show
        when={state() === "playing"}
        fallback={
          <Button
            size="sm"
            variant="outline"
            onClick={() =>
              state() === "paused" ? controller?.resume() : play()
            }
          >
            {state() === "paused" ? "▶ 再開" : "▶ 読み上げ"}
          </Button>
        }
      >
        <Button size="sm" variant="outline" onClick={() => controller?.pause()}>
          ⏸ 一時停止
        </Button>
      </Show>

      <Show when={state() !== "idle"}>
        <Button size="sm" variant="ghost" onClick={() => controller?.stop()}>
          ⏹ 停止
        </Button>
      </Show>

      {/* ソースセグメント: 要約/翻訳が生成されて選択肢が 2 つ以上ある時だけ出す。 */}
      <Show when={props.sources().length > 1}>
        <div class="flex gap-1">
          <For each={props.sources()}>
            {(s) => (
              <Button
                size="sm"
                variant={s.key === sourceKey() ? "outline" : "ghost"}
                onClick={() => onSource(s.key)}
              >
                {s.label}
              </Button>
            )}
          </For>
        </div>
      </Show>

      <div class="h-1.5 min-w-24 flex-1 overflow-hidden rounded-full bg-border">
        <div
          class="h-full bg-primary transition-[width]"
          style={{ width: `${Math.round(progress() * 100)}%` }}
        />
      </div>

      <label class="flex items-center gap-1 text-xs text-muted-foreground">
        速度
        <select
          class="rounded border border-border bg-background px-1 py-0.5 text-xs"
          value={String(rate())}
          onChange={(e) => onRate(Number(e.currentTarget.value))}
        >
          <For each={[0.75, 1, 1.25, 1.5, 2]}>
            {(r) => <option value={String(r)}>{r}×</option>}
          </For>
        </select>
      </label>

      <Show when={voices().length > 0}>
        <select
          class="max-w-40 rounded border border-border bg-background px-1 py-0.5 text-xs"
          value={voiceUri()}
          onChange={(e) => onVoice(e.currentTarget.value)}
        >
          <option value="">自動（おすすめ）</option>
          <For each={voices()}>
            {(v) => (
              <option value={v.voiceURI}>
                {v.name}
                {/^ja/i.test(v.lang) &&
                (/natural|neural|enhanced|premium/i.test(v.name) ||
                  v.localService === false)
                  ? "（自然）"
                  : ""}
              </option>
            )}
          </For>
        </select>
      </Show>
    </section>
  );
}
