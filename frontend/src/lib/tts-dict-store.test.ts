import { describe, it, expect, beforeEach } from "vitest";
import {
  STORAGE_KEY,
  userDict,
  initTtsDict,
  addEntry,
  updateEntry,
  removeEntry,
  mergedDict,
} from "./tts-dict-store";
import { BUILTIN_DICT } from "./tts-dict";

beforeEach(() => {
  localStorage.clear();
  initTtsDict(); // signal を localStorage（空）から作り直し、テスト間を分離する。
});

describe("tts-dict-store", () => {
  it("addEntry reflects to both the signal and localStorage", () => {
    addEntry({ match: "K8s", reading: "ケーエイツ", caseSensitive: false });
    expect(userDict()).toHaveLength(1);
    expect(userDict()[0].reading).toBe("ケーエイツ");
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY)!);
    expect(stored).toEqual([
      { match: "K8s", reading: "ケーエイツ", caseSensitive: false },
    ]);
  });

  it("addEntry with a duplicate key replaces instead of appending", () => {
    addEntry({ match: "AI", reading: "アイ", caseSensitive: true });
    addEntry({ match: "AI", reading: "エイアイ", caseSensitive: true });
    expect(userDict()).toHaveLength(1);
    expect(userDict()[0].reading).toBe("エイアイ");
  });

  it("updateEntry patches a row and persists", () => {
    addEntry({ match: "AI", reading: "アイ", caseSensitive: true });
    updateEntry(0, { reading: "エーアイ" });
    expect(userDict()[0].reading).toBe("エーアイ");
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY)!);
    expect(stored[0].reading).toBe("エーアイ");
  });

  it("removeEntry drops a row and persists", () => {
    addEntry({ match: "AI", reading: "アイ", caseSensitive: true });
    addEntry({ match: "UI", reading: "ユーアイ", caseSensitive: true });
    removeEntry(0);
    expect(userDict().map((e) => e.match)).toEqual(["UI"]);
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY)!)).toHaveLength(1);
  });

  it("initTtsDict swallows invalid JSON and yields an empty dict", () => {
    localStorage.setItem(STORAGE_KEY, "{not json");
    initTtsDict();
    expect(userDict()).toEqual([]);
  });

  it("initTtsDict drops malformed entries", () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify([
        { match: "OK", reading: "オーケー", caseSensitive: true },
        { match: "bad" }, // reading/caseSensitive 欠落
        "nope",
      ]),
    );
    initTtsDict();
    expect(userDict()).toEqual([
      { match: "OK", reading: "オーケー", caseSensitive: true },
    ]);
  });

  it("mergedDict merges builtin and user, user wins on key collision", () => {
    const merged0 = mergedDict();
    // 初期状態は組み込みのみ（件数一致）。
    expect(merged0).toHaveLength(BUILTIN_DICT.length);
    const ai0 = merged0.find((e) => e.match === "AI");
    expect(ai0?.reading).toBe("エーアイ");

    // ユーザーが "AI" を上書き。
    addEntry({ match: "AI", reading: "アイ", caseSensitive: true });
    const merged1 = mergedDict();
    expect(merged1).toHaveLength(BUILTIN_DICT.length); // 件数は増えない（上書き）
    expect(merged1.find((e) => e.match === "AI")?.reading).toBe("アイ");
  });
});
