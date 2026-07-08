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
  onChunk?: (idx: number, count: number) => void; // チャンク（文）開始通知。位置保存用。
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

/**
 * 環境で最も自然に読めそうな日本語音声を選ぶ（声未選択時の既定）。
 *
 * `speechSynthesis` の既定音声は OS 依存（macOS/iOS は Kyoko/O-ren といった機械的な声）
 * なので、明示選択が無いと自然さを取りこぼす。ここでは ja 音声を品質順に採点して選ぶ。
 * ja 音声が 1 つも無ければ null（エンジン既定に委ねる）。
 */
export function pickBestJaVoice(
  voices: SpeechSynthesisVoice[],
): SpeechSynthesisVoice | null {
  const ja = voices.filter((v) => (v.lang || "").toLowerCase().startsWith("ja"));
  if (ja.length === 0) return null;
  const score = (v: SpeechSynthesisVoice): number => {
    const n = `${v.name} ${v.voiceURI}`.toLowerCase();
    let s = 0;
    // ニューラル/高品質声。iOS の日本語ロケールでは表示名が「（拡張）」等に
    // 訳されることがあるため日本語ラベルも見る（voiceURI は英語識別子のまま）。
    if (/natural|neural|enhanced|premium|拡張|プレミアム/.test(n)) s += 100;
    if (v.localService === false) s += 40; // オンライン/ネットワーク声は概して高品質
    if (/nanami|keita|hattori|ayumi|ichiro/.test(n)) s += 20; // 既知の良質声
    if (/google/.test(n)) s += 15; // Chrome/Android の Google 日本語
    if (/kyoko|o-?ren|otoya|siri/.test(n)) s += 5; // 既定級（下限のフォールバック）
    return s;
  };
  return ja.reduce((best, v) => (score(v) > score(best) ? v : best), ja[0]);
}

/**
 * voices の変化を購読する。iOS Safari は getVoices() が遅れて（ときに
 * ユーザー操作の後で）埋まるため、一度きりの取得＋短いフォールバックでは
 * 空のまま固まり、声の選択肢も pickBestJaVoice も効かなくなる。
 * 即時に現在値を届け、以後 voiceschanged のたびに再通知する。戻り値は解除関数。
 */
export function watchVoices(
  cb: (voices: SpeechSynthesisVoice[]) => void,
): () => void {
  if (!ttsSupported()) {
    cb([]);
    return () => {};
  }
  const synth = window.speechSynthesis;
  const push = () => cb(synth.getVoices());
  push();
  // Safari は歴史的に speechSynthesis への addEventListener が機能せず、
  // onvoiceschanged プロパティ代入だけがイベントを受け取れた。両方に登録する
  // （両方発火しても同じ一覧を再通知するだけで無害）。
  synth.addEventListener("voiceschanged", push);
  synth.onvoiceschanged = push;
  return () => {
    synth.removeEventListener("voiceschanged", push);
    if (synth.onvoiceschanged === push) synth.onvoiceschanged = null;
  };
}

export interface TtsController {
  state: () => TtsState;
  play: (fromChunk?: number) => void;
  pause: () => void;
  resume: () => void;
  stop: () => void;
  dispose: () => void;
}

/** 進捗補間の想定読了速度（rate=1 時・1 秒あたり文字数）。おおよその体感値。
 *  時間表示の初期推定（tts-time.ts）とも共有する。 */
export const CHARS_PER_SEC = 7;
/** 進捗補間タイマーの間隔（ms）。 */
const PROGRESS_TICK_MS = 250;

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
  // 現チャンクの進捗補間タイマー。チャンク遷移・stop・dispose で必ず解除する。
  let progressTimer: ReturnType<typeof setInterval> | undefined;

  const clearTimer = () => {
    if (progressTimer !== undefined) {
      clearInterval(progressTimer);
      progressTimer = undefined;
    }
  };

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
    clearTimer(); // 前チャンクの補間タイマーが残らないよう先に解除。
    if (i >= chunks.length) {
      setState("idle");
      cb.onProgress?.(1);
      cb.onEnd?.();
      return;
    }
    idx = i;
    // これから読む文の index を通知（disposed は冒頭で早期 return 済み）。
    // 完了 terminal 分岐（i>=length）は idx=i の前に return するので発火しない
    // ＝ chunk===count は保存されず、クリアは onEnd が担う。
    cb.onChunk?.(i, chunks.length);
    const chunk = chunks[i];
    const u = new SpeechSynthesisUtterance(chunk);
    const rate = opts.rate && opts.rate > 0 ? opts.rate : 1;
    if (opts.rate) u.rate = opts.rate;
    if (opts.voice) u.voice = opts.voice;

    // チャンク内の進捗（文字位置）。onboundary（正確）と時間補間（onboundary を
    // 発火しないニューラル音声向けの推定）のうち大きい方を採り、単調増加を保証する。
    let charInChunk = 0;
    let boundaryFired = false;
    const advance = (c: number) => {
      const clamped = Math.min(Math.max(c, 0), chunk.length);
      if (!disposed && clamped > charInChunk) {
        charInChunk = clamped;
        report(charInChunk);
      }
    };
    // dispose 済みコントローラの遅延 onboundary が進捗を汚さないよう advance がガード。
    u.onboundary = (e) => {
      boundaryFired = true; // 以後は正確な onboundary を信頼し補間を止める。
      advance(e.charIndex ?? 0);
    };
    u.onend = () => {
      clearTimer();
      if (!disposed && state === "playing") speakFrom(i + 1);
    };

    // 時間補間: onboundary が来ない音声でも進捗バー/既読化/位置保存を前進させる。
    // 一時停止中（state!=="playing"）は加算せず凍結。onboundary が来たら補間を止める。
    let elapsedMs = 0;
    const msPerChar = 1000 / (CHARS_PER_SEC * rate);
    progressTimer = setInterval(() => {
      if (disposed) {
        clearTimer();
        return;
      }
      if (state !== "playing" || boundaryFired) return;
      elapsedMs += PROGRESS_TICK_MS;
      advance(elapsedMs / msPerChar);
    }, PROGRESS_TICK_MS);

    synth.speak(u);
  };

  // 指定チャンクの先頭から再生を開始する共通処理（play / resume 共用）。
  // pause 中に paused 固着したエンジン（Chrome 既知）を先に解除し、前の発話を
  // クリアしてから読み直す。
  const start = (fromChunk: number) => {
    if (synth.paused) synth.resume();
    synth.cancel(); // 前の発話をクリア
    setState("playing");
    // fromChunk >= length は terminal 分岐で即 onProgress(1)+onEnd（clamp しない）。
    speakFrom(fromChunk);
  };

  return {
    state: () => state,
    play: (fromChunk = 0) => start(fromChunk),
    // 一時停止: speechSynthesis.pause() は現在の1文(utterance)を境界まで読み切ってから
    // しか止まらない（macOS/iOS のローカル音声等）ため、押した瞬間に cancel して即無音に
    // する。state を先に paused へ落とし、cancel が誘発しうる onend が次チャンクへ進むのを
    // 防ぐ（onend は state==="playing" ガード）。再開は resume が現在文(idx)の先頭から。
    pause: () => {
      if (state === "playing") {
        setState("paused");
        clearTimer();
        synth.cancel();
      }
    },
    // 再開: Web Speech は文の途中から再開できないので、現在の文チャンクの先頭から読み直す。
    resume: () => {
      if (state === "paused") start(idx);
    },
    stop: () => {
      clearTimer();
      synth.cancel();
      setState("idle");
      cb.onProgress?.(0);
    },
    dispose: () => {
      disposed = true;
      clearTimer();
      synth.cancel();
    },
  };
}
