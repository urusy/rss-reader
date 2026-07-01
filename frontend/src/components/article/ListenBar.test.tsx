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
  // autoEnd=true のとき、speak した発話を microtask で自動完了させ、
  // キューを最後まで流して進捗 1.0（＝完了）まで到達させる。
  autoEnd = false;
  speak(u: MockUtterance) {
    this.spoken.push(u);
    this.current = u;
    if (this.autoEnd) queueMicrotask(() => u.onend?.());
  }
  cancel() {
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
});
