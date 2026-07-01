/**
 * 読み上げ (TTS) ユーザー辞書の永続化＋組み込み辞書とのマージ。
 * theme.ts に倣い、import 時に localStorage を読まない（signal は [] で seed、
 * 起動時に initTtsDict() で読み込む＝jsdom 安全・テスト容易）。
 */

import { createSignal } from "solid-js";
import { BUILTIN_DICT, type DictEntry } from "./tts-dict";

export const STORAGE_KEY = "tts-dict";

function isEntry(v: unknown): v is DictEntry {
  if (typeof v !== "object" || v === null) return false;
  const e = v as Record<string, unknown>;
  return (
    typeof e.match === "string" &&
    typeof e.reading === "string" &&
    typeof e.caseSensitive === "boolean"
  );
}

/** localStorage からユーザー辞書を読む。壊れていれば []（副作用なしの純粋関数）。 */
export function load(): DictEntry[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isEntry);
  } catch {
    return [];
  }
}

const [userDict, setUserDict] = createSignal<DictEntry[]>([]);
export { userDict };

/** 起動時に一度呼ぶ（index.tsx）。localStorage → signal へ反映。 */
export function initTtsDict(): void {
  setUserDict(load());
}

function persist(next: DictEntry[]): void {
  setUserDict(next);
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  }
}

/** マージ／重複判定のキー。大小区別ありは綴りそのまま、なしは小文字化。 */
function keyOf(e: DictEntry): string {
  return e.caseSensitive ? e.match : e.match.toLowerCase();
}

/** 追加（同キーは置換して重複を作らない）。 */
export function addEntry(entry: DictEntry): void {
  const k = keyOf(entry);
  persist([...userDict().filter((e) => keyOf(e) !== k), entry]);
}

/** index 行を部分更新。 */
export function updateEntry(index: number, patch: Partial<DictEntry>): void {
  persist(userDict().map((e, i) => (i === index ? { ...e, ...patch } : e)));
}

/** index 行を削除。 */
export function removeEntry(index: number): void {
  persist(userDict().filter((_, i) => i !== index));
}

/** 組み込み ∪ ユーザー（同キーはユーザー優先）。読み上げ時に使う。 */
export function mergedDict(): DictEntry[] {
  const map = new Map<string, DictEntry>();
  for (const e of BUILTIN_DICT) map.set(keyOf(e), e);
  for (const e of userDict()) map.set(keyOf(e), e);
  return [...map.values()];
}
