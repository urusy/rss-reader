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
  ttsSupported,
  type TtsController,
  type TtsState,
} from "@/lib/tts";
import { Button } from "@/components/ui/button";

const RATE_KEY = "tts-rate";
const VOICE_KEY = "tts-voice";
// 読み上げがこの割合に達したら既読化フックを呼ぶ（聴いて消化した、とみなす）。#33
const READ_AT = 0.8;

/**
 * リッスンモードの再生バー（#33 v1）。`text` はプレーン化済み本文。
 * 進捗が READ_AT を超えたら一度だけ onListened を呼び、既読化に繋ぐ。
 */
export default function ListenBar(props: {
  articleId: string;
  text: string;
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

  let controller: TtsController | undefined;
  let listened = false;

  onMount(() => {
    void loadVoices().then(setVoices);
  });

  const pickedVoice = () =>
    voices().find((v) => v.voiceURI === voiceUri()) ?? null;

  const build = () =>
    createTtsController(
      props.text,
      { rate: rate(), voice: pickedVoice() },
      {
        onState: setState,
        onProgress: (r) => {
          setProgress(r);
          if (!listened && r >= READ_AT) {
            listened = true;
            props.onListened?.();
          }
        },
        onEnd: () => setProgress(1),
      },
    );

  const play = () => {
    controller?.dispose();
    controller = build();
    controller.play();
  };

  // 記事が切り替わったら（articleId 変化）読み上げを止めてリセット。
  createEffect((prev: string | undefined) => {
    const id = props.articleId;
    if (prev !== undefined && prev !== id) {
      controller?.dispose();
      controller = undefined;
      listened = false;
      setProgress(0);
      setState("idle");
    }
    return id;
  });

  onCleanup(() => controller?.dispose());

  // 速度変更は次回再生から（再生中なら作り直して継続位置リセット）。設定は永続化。
  const onRate = (v: number) => {
    setRate(v);
    localStorage.setItem(RATE_KEY, String(v));
    if (state() !== "idle") play();
  };
  const onVoice = (uri: string) => {
    setVoiceUri(uri);
    localStorage.setItem(VOICE_KEY, uri);
    if (state() !== "idle") play();
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
          <option value="">既定の声</option>
          <For each={voices()}>
            {(v) => <option value={v.voiceURI}>{v.name}</option>}
          </For>
        </select>
      </Show>
    </section>
  );
}
