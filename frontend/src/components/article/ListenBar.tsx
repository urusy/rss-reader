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
  watchVoices,
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
import { estimateTotalSecs, formatClock } from "@/lib/tts-time";
import { api } from "@/lib/api";
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

  // --- 時間表示（経過 / 約全体）---
  // 全体時間は API から取れないため推定: 初期値=文字数÷(想定読速×rate)、
  // 実再生が進んだら 実再生秒÷進捗増分 に較正（tts-time.ts）。経過は
  // 進捗率×推定全体 で導出し、バーと常に整合させる。セッションは play() 毎に
  // リセット（rate/声変更も play 経由なので速度変化後は測り直しになる）。
  const [sessMs, setSessMs] = createSignal(0); // 実再生ms（一時停止中は凍結）
  const [sessP0, setSessP0] = createSignal<number | null>(null); // セッション開始時の進捗
  let playingSince: number | null = null;
  const resetTimeSession = () => {
    setSessMs(0);
    setSessP0(null);
    playingSince = null;
  };
  createEffect(() => {
    if (state() === "playing") {
      playingSince = Date.now();
      if (sessP0() === null) setSessP0(progress());
    } else if (playingSince !== null) {
      setSessMs((m) => m + (Date.now() - playingSince!));
      playingSince = null;
    }
  });
  // 進捗ティック（約250ms毎）に再評価される。正規化前の表示テキスト長で十分
  // （初期推定用途。毎ティックの辞書正規化は避ける）。
  const totalSecs = () => {
    const playedSecs =
      (sessMs() +
        (state() === "playing" && playingSince !== null
          ? Date.now() - playingSince
          : 0)) /
      1000;
    return estimateTotalSecs({
      totalChars: current()?.text.length ?? 0,
      rate: rate(),
      playedSecs,
      progressDelta: Math.max(0, progress() - (sessP0() ?? progress())),
    });
  };

  onMount(() => {
    // iOS Safari は getVoices() が遅延充填されるため、一度きりの取得では空のまま
    // 固まる。watchVoices で即時値＋voiceschanged 後着を購読し続ける。
    onCleanup(watchVoices(setVoices));

    // バックグラウンド遷移で自動一時停止（iOS はタブ非表示で発話が停止/シンセが
    // 壊れ、再開・進捗・既読化が破綻するため、明示的に paused へ畳んでおく）。
    const onVisibility = () => {
      if (document.hidden && state() === "playing") controller?.pause();
    };
    document.addEventListener("visibilitychange", onVisibility);
    onCleanup(() =>
      document.removeEventListener("visibilitychange", onVisibility),
    );
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
    resetTimeSession(); // 時間較正はセッション（play〜stop）単位で測り直す
    controller.play(start);
    // 利用状況の記録（読み上げ対象の内訳つき）。テレメトリなので失敗は握りつぶす。
    const src = current().key === "body" ? "content" : current().key;
    api.recordUsage("tts_play", { source: src }).catch(() => {});
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
      {/* --- 再生系（トランスポート）: 主操作は塗り、補助操作は輪郭で統一 ---
          対象選択のセグメントとは縦罫線で区切り、役割を視覚的に分ける。 */}
      <div class="flex items-center gap-1" role="group" aria-label="再生操作">
        <Show
          when={state() === "playing"}
          fallback={
            <Button
              size="sm"
              onClick={() =>
                state() === "paused" ? controller?.resume() : play()
              }
            >
              {state() === "paused" ? "▶ 再開" : "▶ 読み上げ"}
            </Button>
          }
        >
          <Button size="sm" onClick={() => controller?.pause()}>
            ⏸ 一時停止
          </Button>
        </Show>

        {/* 「最初から再生」: 再開ポイントがある時（一時停止中／保存位置のある idle）だけ、
            再開ボタンに加えて先頭からの再生を提供する。play(0) で保存 chunk を無視する。 */}
        <Show
          when={
            state() === "paused" ||
            (state() === "idle" && progress() > 0 && progress() < 1)
          }
        >
          <Button size="sm" variant="outline" onClick={() => play(0)}>
            ⏮ 最初から
          </Button>
        </Show>

        <Show when={state() !== "idle"}>
          <Button size="sm" variant="outline" onClick={() => controller?.stop()}>
            ⏹ 停止
          </Button>
        </Show>
      </div>

      {/* --- 対象選択: セグメントコントロール（枠つきグループ・選択中は塗り）。
          要約/翻訳が生成されて選択肢が 2 つ以上ある時だけ出す。 --- */}
      <Show when={props.sources().length > 1}>
        <div aria-hidden="true" class="h-5 w-px shrink-0 bg-border" />
        <div
          role="group"
          aria-label="読み上げ対象"
          class="inline-flex items-center rounded-md border border-border bg-background p-0.5"
        >
          <For each={props.sources()}>
            {(s) => (
              <button
                type="button"
                aria-pressed={s.key === sourceKey()}
                class={`rounded px-2.5 py-1 text-xs transition-colors pointer-coarse:min-h-9 ${
                  s.key === sourceKey()
                    ? "bg-primary text-primary-foreground"
                    : "text-muted-foreground hover:text-foreground"
                }`}
                onClick={() => onSource(s.key)}
              >
                {s.label}
              </button>
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

      {/* 経過 / 全体（推定）。全体は較正されるまで想定読速ベースなので「約」を明示。 */}
      <Show when={(current()?.text.length ?? 0) > 0}>
        <span class="shrink-0 text-xs tabular-nums text-muted-foreground">
          {formatClock(progress() * totalSecs())} / 約{formatClock(totalSecs())}
        </span>
      </Show>

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
