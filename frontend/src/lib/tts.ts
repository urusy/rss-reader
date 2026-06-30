/**
 * 読み上げ (TTS) リッスンモード v1 — ブラウザ標準 Web Speech API の薄いラッパ。
 * バックエンド・トークン消費はゼロ。サニタイズ済み本文をプレーンテキスト化して読む。
 *
 * 長文対策: 多くのブラウザは1発話が長いと途中で止まるため、文単位に分割して
 * キュー再生し、累積文字数で全体進捗を出す。進捗コールバックで既読化フックに繋ぐ。
 */

export type TtsState = "idle" | "playing" | "paused";

export interface TtsCallbacks {
  onState?: (state: TtsState) => void;
  onProgress?: (ratio: number) => void; // 0..1
  onEnd?: () => void;
}

/** TTS が利用可能か（SSR / 非対応ブラウザを弾く）。 */
export function ttsSupported(): boolean {
  return (
    typeof window !== "undefined" &&
    "speechSynthesis" in window &&
    typeof window.SpeechSynthesisUtterance !== "undefined"
  );
}

/** サニタイズ済み HTML をプレーンテキストへ（タグ除去・空白正規化）。 */
export function htmlToPlainText(html: string): string {
  if (typeof window === "undefined" || !("DOMParser" in window)) return html;
  const doc = new DOMParser().parseFromString(html, "text/html");
  return (doc.body.textContent ?? "").replace(/\s+/g, " ").trim();
}

/** 文末（。．！？.!? と改行）でざっくり分割。空チャンクは捨てる。 */
export function splitSentences(text: string): string[] {
  // 日本語の句点（。．！？）は後続スペースが無いので無条件に区切る。
  // ラテン文字の . ! ? は後続スペースがある時だけ（U.S.A・小数の誤分割を避ける）。
  return text
    .split(/(?<=[。．！？])\s*|(?<=[.!?])\s+|\n+/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

/** 利用可能なボイス一覧。getVoices は非同期に埋まるため voiceschanged も待つ。 */
export function loadVoices(): Promise<SpeechSynthesisVoice[]> {
  if (!ttsSupported()) return Promise.resolve([]);
  const synth = window.speechSynthesis;
  const now = synth.getVoices();
  if (now.length > 0) return Promise.resolve(now);
  return new Promise((resolve) => {
    const handler = () => {
      synth.removeEventListener("voiceschanged", handler);
      resolve(synth.getVoices());
    };
    synth.addEventListener("voiceschanged", handler);
    // 念のためのフォールバック（イベントが来ない実装向け）。
    setTimeout(() => resolve(synth.getVoices()), 500);
  });
}

export interface TtsController {
  state: () => TtsState;
  play: () => void;
  pause: () => void;
  resume: () => void;
  stop: () => void;
  dispose: () => void;
}

/**
 * 1記事ぶんの読み上げコントローラを作る。text は呼び出し側でプレーン化済み。
 */
export function createTtsController(
  text: string,
  opts: { rate?: number; voice?: SpeechSynthesisVoice | null },
  cb: TtsCallbacks,
): TtsController {
  const synth = window.speechSynthesis;
  const chunks = splitSentences(text);
  // 各チャンク開始時点の累積文字数（進捗計算用）。
  const total = chunks.reduce((n, c) => n + c.length, 0) || 1;
  const startOffsets: number[] = [];
  {
    let acc = 0;
    for (const c of chunks) {
      startOffsets.push(acc);
      acc += c.length;
    }
  }

  let idx = 0;
  let state: TtsState = "idle";
  let disposed = false;

  const setState = (s: TtsState) => {
    state = s;
    cb.onState?.(s);
  };

  const report = (charInChunk: number) => {
    const done = (startOffsets[idx] ?? 0) + charInChunk;
    cb.onProgress?.(Math.min(1, done / total));
  };

  const speakFrom = (i: number) => {
    if (disposed) return;
    if (i >= chunks.length) {
      setState("idle");
      cb.onProgress?.(1);
      cb.onEnd?.();
      return;
    }
    idx = i;
    const u = new SpeechSynthesisUtterance(chunks[i]);
    if (opts.rate) u.rate = opts.rate;
    if (opts.voice) u.voice = opts.voice;
    u.onboundary = (e) => report(e.charIndex ?? 0);
    u.onend = () => {
      if (!disposed && state === "playing") speakFrom(i + 1);
    };
    synth.speak(u);
  };

  return {
    state: () => state,
    play: () => {
      synth.cancel(); // 前の発話をクリア
      setState("playing");
      speakFrom(0);
    },
    pause: () => {
      if (state === "playing") {
        synth.pause();
        setState("paused");
      }
    },
    resume: () => {
      if (state === "paused") {
        synth.resume();
        setState("playing");
      }
    },
    stop: () => {
      synth.cancel();
      setState("idle");
      cb.onProgress?.(0);
    },
    dispose: () => {
      disposed = true;
      synth.cancel();
    },
  };
}
