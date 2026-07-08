// iOS Safari は getVoices() が遅れて（ときにユーザー操作後に）埋まるため、
// 一度きりの取得では空のまま固まり、声の選択も自動選択も効かなくなる。
// watchVoices は即時値 + voiceschanged 後着を購読し続けることを保証する。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { pickBestJaVoice, watchVoices } from "./tts";

type Listener = () => void;

class FakeSynth {
  voices: { name: string; lang: string }[] = [];
  private listeners = new Set<Listener>();
  getVoices() {
    return this.voices as unknown as SpeechSynthesisVoice[];
  }
  addEventListener(_: string, fn: Listener) {
    this.listeners.add(fn);
  }
  removeEventListener(_: string, fn: Listener) {
    this.listeners.delete(fn);
  }
  fireVoicesChanged() {
    for (const fn of this.listeners) fn();
  }
}

let synth: FakeSynth;

beforeEach(() => {
  synth = new FakeSynth();
  (window as unknown as { speechSynthesis: FakeSynth }).speechSynthesis = synth;
  (
    globalThis as unknown as { SpeechSynthesisUtterance: unknown }
  ).SpeechSynthesisUtterance = class {};
});

describe("watchVoices", () => {
  it("購読開始時に現在の voices を即時に届ける", () => {
    synth.voices = [{ name: "Kyoko", lang: "ja-JP" }];
    const cb = vi.fn();
    watchVoices(cb);
    expect(cb).toHaveBeenCalledTimes(1);
    expect(cb.mock.calls[0][0]).toHaveLength(1);
  });

  it("voiceschanged の後着（iOS の遅延ロード）を反映する", () => {
    const cb = vi.fn();
    watchVoices(cb); // 最初は空
    expect(cb.mock.lastCall?.[0]).toHaveLength(0);

    synth.voices = [
      { name: "Kyoko", lang: "ja-JP" },
      { name: "Kyoko (Enhanced)", lang: "ja-JP" },
    ];
    synth.fireVoicesChanged();
    expect(cb).toHaveBeenCalledTimes(2);
    expect(cb.mock.lastCall?.[0]).toHaveLength(2);
  });

  it("解除後は voiceschanged が届かない", () => {
    const cb = vi.fn();
    const stop = watchVoices(cb);
    stop();
    synth.voices = [{ name: "Kyoko", lang: "ja-JP" }];
    synth.fireVoicesChanged();
    expect(cb).toHaveBeenCalledTimes(1); // 初回の即時通知のみ
  });

  it("addEventListener が効かない Safari 旧実装でも onvoiceschanged 経由で届く", () => {
    // 歴史的に Safari は speechSynthesis の addEventListener が機能せず、
    // onvoiceschanged プロパティ代入だけがイベントを受け取れた。
    class LegacySynth {
      voices: { name: string; lang: string }[] = [];
      onvoiceschanged: (() => void) | null = null;
      getVoices() {
        return this.voices as unknown as SpeechSynthesisVoice[];
      }
      addEventListener() {} // 登録しても発火しない
      removeEventListener() {}
      fire() {
        this.onvoiceschanged?.();
      }
    }
    const legacy = new LegacySynth();
    (window as unknown as { speechSynthesis: LegacySynth }).speechSynthesis =
      legacy;

    const cb = vi.fn();
    const stop = watchVoices(cb);
    legacy.voices = [{ name: "Kyoko", lang: "ja-JP" }];
    legacy.fire();
    expect(cb).toHaveBeenCalledTimes(2); // 即時 + onvoiceschanged 後着
    expect(cb.mock.lastCall?.[0]).toHaveLength(1);

    stop();
    legacy.fire();
    expect(cb).toHaveBeenCalledTimes(2); // 解除後は届かない
  });
});

describe("pickBestJaVoice (iOS の声名バリエーション)", () => {
  const v = (name: string, voiceURI: string) =>
    ({ name, voiceURI, lang: "ja-JP", localService: true }) as SpeechSynthesisVoice;

  it("voiceURI の enhanced 識別子で高品質を選ぶ（iOS の実形式）", () => {
    const compact = v("Kyoko", "com.apple.ttsbundle.Kyoko-compact");
    const enhanced = v("Kyoko", "com.apple.voice.enhanced.ja-JP.Kyoko");
    expect(pickBestJaVoice([compact, enhanced])).toBe(enhanced);
  });

  it("日本語ロケールの表示名（拡張/プレミアム）でも高品質を選ぶ", () => {
    const compact = v("Kyoko", "kyoko-compact");
    const kakuchou = v("Kyoko（拡張）", "kyoko-ja");
    expect(pickBestJaVoice([compact, kakuchou])).toBe(kakuchou);
  });
});
