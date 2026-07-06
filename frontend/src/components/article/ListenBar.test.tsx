import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  render,
  fireEvent,
  screen,
  waitFor,
  cleanup,
} from "@solidjs/testing-library";
import ListenBar, { type ListenSource } from "./ListenBar";
import { saveTtsPos, loadTtsPos, hashText } from "@/lib/tts-progress";
import { normalizeForTts } from "@/lib/tts-normalize";
import { mergedDict } from "@/lib/tts-dict-store";
import { splitSentences } from "@/lib/tts";

// --- speechSynthesis の最小モック ---
class MockUtterance {
  text: string;
  rate?: number;
  voice: SpeechSynthesisVoice | null = null;
  onboundary: ((e: { charIndex?: number }) => void) | null = null;
  onend: (() => void) | null = null;
  constructor(text: string) {
    this.text = text;
  }
}

class MockSynth {
  paused = false;
  spoken: MockUtterance[] = [];
  current: MockUtterance | null = null;
  cancelCount = 0;
  // autoEnd=true のとき、speak した発話を microtask で自動完了させ、
  // キューを最後まで流して進捗 1.0（＝完了）まで到達させる。
  autoEnd = false;
  speak(u: MockUtterance) {
    this.spoken.push(u);
    this.current = u;
    if (this.autoEnd) queueMicrotask(() => u.onend?.());
  }
  cancel() {
    this.cancelCount++;
    this.current = null;
  }
  pause() {
    this.paused = true;
  }
  resume() {
    this.paused = false;
  }
  getVoices() {
    return [] as SpeechSynthesisVoice[];
  }
  addEventListener() {}
  removeEventListener() {}
}

let synth: MockSynth;

beforeEach(() => {
  synth = new MockSynth();
  (window as unknown as { speechSynthesis: MockSynth }).speechSynthesis = synth;
  (
    globalThis as unknown as {
      SpeechSynthesisUtterance: typeof MockUtterance;
    }
  ).SpeechSynthesisUtterance = MockUtterance;
  localStorage.clear();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const body: ListenSource = {
  key: "body",
  label: "本文",
  text: "B one. B two.",
  marksRead: true,
};
const summary: ListenSource = {
  key: "summary",
  label: "要約",
  text: "S one. S two.",
  marksRead: false,
};

describe("ListenBar", () => {
  it("marks read only for the body source when playback completes", async () => {
    synth.autoEnd = true;
    const onListened = vi.fn();
    render(() => (
      <ListenBar articleId="a1" sources={() => [body]} onListened={onListened} />
    ));
    fireEvent.click(screen.getByText("▶ 読み上げ"));
    await waitFor(() => expect(onListened).toHaveBeenCalledTimes(1));
  });

  it("does not mark read when a non-body source is played to completion", async () => {
    synth.autoEnd = true;
    const onListened = vi.fn();
    render(() => (
      <ListenBar
        articleId="a1"
        sources={() => [body, summary]}
        onListened={onListened}
      />
    ));
    // 要約セグメントを選んでから再生（本文の既読フックはゲートされる）。
    fireEvent.click(screen.getByText("要約"));
    fireEvent.click(screen.getByText("▶ 読み上げ"));
    await waitFor(() =>
      expect(synth.spoken.length).toBeGreaterThanOrEqual(2),
    );
    await Promise.resolve();
    expect(onListened).not.toHaveBeenCalled();
  });

  it("does not stop playback when re-clicking the already-selected source", async () => {
    // autoEnd=false → 再生は playing のまま留まる。
    render(() => (
      <ListenBar
        articleId="a1"
        sources={() => [body, summary]}
        onListened={vi.fn()}
      />
    ));
    fireEvent.click(screen.getByText("▶ 読み上げ"));
    await waitFor(() => screen.getByText("⏸ 一時停止"));
    // 選択中の本文セグメントを再クリックしても停止しない（再選択ガード）。
    fireEvent.click(screen.getByText("本文"));
    expect(screen.getByText("⏸ 一時停止")).toBeTruthy();
  });

  // 実際に読み上げる（正規化後）テキストと、その文チャンク。位置 seed に使う。
  const spokenOf = (s: ListenSource) => normalizeForTts(s.text, mergedDict());
  const chunksOf = (s: ListenSource) => splitSentences(spokenOf(s));

  it("resumes playback from the saved chunk on play", async () => {
    const t = spokenOf(body);
    saveTtsPos("a1", "body", {
      chunk: 1,
      len: t.length,
      hash: hashText(t),
      ratio: 0.5,
      t: 1,
    });
    render(() => (
      <ListenBar articleId="a1" sources={() => [body]} onListened={vi.fn()} />
    ));
    fireEvent.click(screen.getByText("▶ 読み上げ"));
    // 先頭ではなく保存 chunk(1) の文から読み始める。
    expect(synth.spoken[0].text).toBe(chunksOf(body)[1]);
  });

  it("clears the saved position after playing to completion", async () => {
    synth.autoEnd = true;
    render(() => (
      <ListenBar articleId="a1" sources={() => [body]} onListened={vi.fn()} />
    ));
    fireEvent.click(screen.getByText("▶ 読み上げ"));
    await waitFor(() =>
      expect(loadTtsPos("a1", "body", spokenOf(body))).toBeNull(),
    );
  });

  it("keeps the saved position when stopped (⏹ resumes later)", async () => {
    render(() => (
      <ListenBar articleId="a1" sources={() => [body]} onListened={vi.fn()} />
    ));
    fireEvent.click(screen.getByText("▶ 読み上げ")); // onChunk(0) が位置を保存
    fireEvent.click(screen.getByText("⏹ 停止"));
    expect(loadTtsPos("a1", "body", spokenOf(body))).not.toBeNull();
  });

  it("offers a restart button when a saved position exists and plays from the top", async () => {
    const t = spokenOf(body);
    // 保存 chunk(1) があるので idle でも「最初から」を出す。
    saveTtsPos("a1", "body", {
      chunk: 1,
      len: t.length,
      hash: hashText(t),
      ratio: 0.5,
      t: 1,
    });
    render(() => (
      <ListenBar articleId="a1" sources={() => [body]} onListened={vi.fn()} />
    ));
    // 通常の再開（▶ 読み上げ）は保存 chunk から。最初からは先頭 chunk(0) から。
    fireEvent.click(screen.getByText("⏮ 最初から"));
    expect(synth.spoken[0].text).toBe(chunksOf(body)[0]);
  });

  it("shows both resume and restart while paused", async () => {
    render(() => (
      <ListenBar articleId="a1" sources={() => [body]} onListened={vi.fn()} />
    ));
    fireEvent.click(screen.getByText("▶ 読み上げ"));
    await waitFor(() => screen.getByText("⏸ 一時停止"));
    fireEvent.click(screen.getByText("⏸ 一時停止"));
    // paused: 再開ボタンに加えて最初から再生ボタンも出る。
    expect(screen.getByText("▶ 再開")).toBeTruthy();
    fireEvent.click(screen.getByText("⏮ 最初から"));
    // 最新の発話は先頭 chunk（作り直して 0 から読み直す）。
    expect(synth.spoken[synth.spoken.length - 1].text).toBe(chunksOf(body)[0]);
  });

  it("cancels the current utterance immediately on pause (no wait for sentence end)", async () => {
    render(() => (
      <ListenBar articleId="a1" sources={() => [body]} onListened={vi.fn()} />
    ));
    fireEvent.click(screen.getByText("▶ 読み上げ"));
    await waitFor(() => screen.getByText("⏸ 一時停止"));
    const before = synth.cancelCount;
    fireEvent.click(screen.getByText("⏸ 一時停止"));
    // 一時停止は現在の発話を即 cancel する（文末まで待たない）。
    expect(synth.cancelCount).toBeGreaterThan(before);
    expect(screen.getByText("▶ 再開")).toBeTruthy();
  });

  it("resumes the current chunk from its start after pause", async () => {
    render(() => (
      <ListenBar articleId="a1" sources={() => [body]} onListened={vi.fn()} />
    ));
    fireEvent.click(screen.getByText("▶ 読み上げ"));
    await waitFor(() => screen.getByText("⏸ 一時停止"));
    fireEvent.click(screen.getByText("⏸ 一時停止")); // paused（cancel 済み）
    const spokenBefore = synth.spoken.length;
    fireEvent.click(screen.getByText("▶ 再開"));
    await waitFor(() => screen.getByText("⏸ 一時停止"));
    // 再開で現在チャンク（先頭 = chunk 0）が改めて speak される。
    expect(synth.spoken.length).toBeGreaterThan(spokenBefore);
    expect(synth.spoken[synth.spoken.length - 1].text).toBe(chunksOf(body)[0]);
  });

  it("resumes from the current chunk (not the top) when paused mid-article", async () => {
    const t = spokenOf(body);
    // 保存 chunk(1) から再生開始 → 途中で一時停止 → 再開してもその文の先頭から。
    saveTtsPos("a1", "body", {
      chunk: 1,
      len: t.length,
      hash: hashText(t),
      ratio: 0.5,
      t: 1,
    });
    render(() => (
      <ListenBar articleId="a1" sources={() => [body]} onListened={vi.fn()} />
    ));
    fireEvent.click(screen.getByText("▶ 読み上げ"));
    await waitFor(() => screen.getByText("⏸ 一時停止"));
    expect(synth.spoken[synth.spoken.length - 1].text).toBe(chunksOf(body)[1]);
    fireEvent.click(screen.getByText("⏸ 一時停止"));
    const spokenBefore = synth.spoken.length;
    fireEvent.click(screen.getByText("▶ 再開"));
    await waitFor(() => screen.getByText("⏸ 一時停止"));
    // 再開で実際に再発話し、かつ chunk 0 に巻き戻らず chunk 1 の先頭から読み直す。
    expect(synth.spoken.length).toBeGreaterThan(spokenBefore);
    expect(synth.spoken[synth.spoken.length - 1].text).toBe(chunksOf(body)[1]);
  });

  it("does not show restart during fresh idle (no saved position)", () => {
    render(() => (
      <ListenBar articleId="a1" sources={() => [body]} onListened={vi.fn()} />
    ));
    expect(screen.queryByText("⏮ 最初から")).toBeNull();
  });

  it("ignores a stale saved position when the text changed (len mismatch)", async () => {
    // 別テキスト由来の len/hash を仕込む → 照合不一致で先頭から。
    saveTtsPos("a1", "body", {
      chunk: 1,
      len: 99999,
      hash: 12345,
      ratio: 0.5,
      t: 1,
    });
    render(() => (
      <ListenBar articleId="a1" sources={() => [body]} onListened={vi.fn()} />
    ));
    fireEvent.click(screen.getByText("▶ 読み上げ"));
    expect(synth.spoken[0].text).toBe(chunksOf(body)[0]);
  });

  it("時間表示: 再生前から「経過 / 約全体（文字数÷読速の初期推定）」を出す", () => {
    // body は "B one. B two." の13文字 → 13/7 ≒ 1.86秒 → 四捨五入で 0:02。
    render(() => (
      <ListenBar articleId="a1" sources={() => [body]} onListened={vi.fn()} />
    ));
    expect(screen.getByText("0:00 / 約0:02")).toBeTruthy();
  });
});
