import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { htmlToPlainText, splitSentences, createTtsController } from "./tts";

// --- speechSynthesis の最小モック（jsdom には無いので自前で用意する） ---
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
  // 実 Chrome の cancel() は paused フラグを残す挙動を再現（resume でのみ解除）。
  speak(u: MockUtterance) {
    this.spoken.push(u);
    this.current = u;
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

function installSynth() {
  synth = new MockSynth();
  vi.spyOn(synth, "resume");
  vi.spyOn(synth, "cancel");
  (window as unknown as { speechSynthesis: MockSynth }).speechSynthesis = synth;
  (globalThis as unknown as { SpeechSynthesisUtterance: typeof MockUtterance }).SpeechSynthesisUtterance =
    MockUtterance;
  return synth;
}

describe("htmlToPlainText", () => {
  it("strips tags and normalizes whitespace", () => {
    expect(htmlToPlainText("<p>Hello   <b>world</b></p>\n<p>Bye</p>")).toBe(
      "Hello world Bye",
    );
  });

  it("returns empty string for tag-only html", () => {
    expect(htmlToPlainText("<br><hr>")).toBe("");
  });
});

describe("splitSentences", () => {
  it("splits on Japanese and Latin sentence enders", () => {
    expect(splitSentences("これは一文。次の文！最後？")).toEqual([
      "これは一文。",
      "次の文！",
      "最後？",
    ]);
    expect(splitSentences("First. Second! Third?")).toEqual([
      "First.",
      "Second!",
      "Third?",
    ]);
  });

  it("splits on newlines and drops empty chunks", () => {
    expect(splitSentences("a\n\n\nb\n")).toEqual(["a", "b"]);
  });

  it("keeps a single unterminated sentence", () => {
    expect(splitSentences("no terminator here")).toEqual([
      "no terminator here",
    ]);
  });
});

describe("createTtsController", () => {
  beforeEach(() => installSynth());
  afterEach(() => vi.restoreAllMocks());

  it("resumes a globally-paused engine before speaking (Chrome fix)", () => {
    synth.paused = true;
    const ctrl = createTtsController(
      "Hello world. Second sentence.",
      {},
      {},
    );
    ctrl.play();
    expect(synth.resume).toHaveBeenCalled();
    expect(synth.paused).toBe(false);
    expect(synth.spoken.length).toBeGreaterThan(0);
  });

  it("ignores a late onboundary after dispose (no progress pollution)", () => {
    const onProgress = vi.fn();
    const ctrl = createTtsController("Hello world foo bar.", {}, { onProgress });
    ctrl.play();
    const u = synth.current!;
    onProgress.mockClear();
    ctrl.dispose();
    u.onboundary?.({ charIndex: 3 });
    expect(onProgress).not.toHaveBeenCalled();
  });

  it("does not advance a disposed controller's queue on a late onend", () => {
    const ctrl = createTtsController("A. B. C.", {}, {});
    ctrl.play();
    const u0 = synth.current!;
    const before = synth.spoken.length;
    ctrl.dispose();
    u0.onend?.();
    expect(synth.spoken.length).toBe(before);
  });

  it("play() with no arg starts from the first chunk (backward compatible)", () => {
    const ctrl = createTtsController("A. B. C.", {}, {});
    ctrl.play();
    expect(synth.spoken[0].text).toBe("A.");
  });

  it("play(fromChunk) resumes from the given chunk", () => {
    const ctrl = createTtsController("A. B. C.", {}, {});
    ctrl.play(2);
    expect(synth.spoken[0].text).toBe("C.");
  });

  it("emits onChunk at each chunk start but never for the terminal branch", () => {
    const chunkCalls: Array<[number, number]> = [];
    let ended = 0;
    const ctrl = createTtsController(
      "A. B. C.",
      {},
      { onChunk: (i, count) => chunkCalls.push([i, count]), onEnd: () => ended++ },
    );
    ctrl.play();
    // 手動で末尾までキューを進める。
    while (synth.current) {
      const u = synth.current;
      synth.current = null;
      u.onend?.();
    }
    expect(chunkCalls).toEqual([
      [0, 3],
      [1, 3],
      [2, 3],
    ]);
    expect(ended).toBe(1); // 完了は onEnd のみ（onChunk(3,..) は発火しない）
  });

  it("treats fromChunk >= length as immediate completion (no clamp)", () => {
    let ended = 0;
    const onProgress = vi.fn();
    const ctrl = createTtsController(
      "A. B.",
      {},
      { onProgress, onEnd: () => ended++ },
    );
    ctrl.play(5); // 2 チャンクしかない
    expect(synth.spoken.length).toBe(0); // 何も speak しない
    expect(onProgress).toHaveBeenLastCalledWith(1);
    expect(ended).toBe(1);
  });
});
