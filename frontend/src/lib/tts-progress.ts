/**
 * リッスンモードの読み上げ位置永続化（#33 v1・途中再開）。
 * resizable.ts / tts-dict-store.ts 流の純粋関数（typeof ガード + try/catch + 形状検証）。
 *
 * 単位は article × source（本文/要約/翻訳を別々）。単一キー `tts-pos` に JSON マップで
 * 集約し、記事削除で残留しても CAP=200 の LRU 剪定で無限成長を防ぐ。
 *
 * 再開の粒度は chunk index（文）。Web Speech は utterance＝文境界でしか途中開始できない
 * ため ratio/文字オフセットは復元に使わず、ratio は進捗バーのマーカー表示専用。
 * 無効化は len（text.length）＋ hash の両一致で判定する。text は「読み上げる正規化後
 * テキスト」を渡すこと（辞書変更でチャンク境界が変わっても自己修復する）。
 */

const TTS_POS_KEY = "tts-pos";
const CAP = 200;
// articleId（UUID）と sourceKey（body/summary/translation）の連結区切り。
// どちらにも現れない "|" を使う。
const SEP = "|";

interface TtsPos {
  chunk: number;
  len: number;
  hash: number;
  ratio: number;
  t: number; // 更新時刻（LRU 剪定用）
}
type TtsPosMap = Record<string, TtsPos>;

/** djb2（>>>0 で符号なし 32bit）。len と併せて同一テキスト判定に使う。 */
export function hashText(s: string): number {
  let h = 5381;
  for (let i = 0; i < s.length; i++) h = ((h << 5) + h + s.charCodeAt(i)) | 0;
  return h >>> 0;
}

function posKey(articleId: string, sourceKey: string): string {
  return articleId + SEP + sourceKey;
}

function readMap(): TtsPosMap {
  try {
    if (typeof localStorage === "undefined") return {};
    const raw = localStorage.getItem(TTS_POS_KEY);
    const m: unknown = raw ? JSON.parse(raw) : {};
    return m && typeof m === "object" ? (m as TtsPosMap) : {};
  } catch {
    return {};
  }
}

function writeMap(m: TtsPosMap): void {
  try {
    localStorage.setItem(TTS_POS_KEY, JSON.stringify(m));
  } catch {
    /* private mode 等で quota 超過しても無視 */
  }
}

/**
 * 純粋 read。len+hash 一致時のみ {chunk, ratio}、不一致/未保存は null（副作用なし）。
 * text は「実際に読み上げる正規化後テキスト」を渡す。
 */
export function loadTtsPos(
  articleId: string,
  sourceKey: string,
  text: string,
): { chunk: number; ratio: number } | null {
  const e = readMap()[posKey(articleId, sourceKey)];
  if (!e || typeof e.chunk !== "number") return null;
  if (e.len !== text.length || e.hash !== hashText(text)) return null;
  return { chunk: e.chunk, ratio: typeof e.ratio === "number" ? e.ratio : 0 };
}

export function saveTtsPos(
  articleId: string,
  sourceKey: string,
  pos: { chunk: number; len: number; hash: number; ratio: number; t: number },
): void {
  const m = readMap();
  m[posKey(articleId, sourceKey)] = pos;
  const keys = Object.keys(m);
  if (keys.length > CAP) {
    keys.sort((a, b) => (m[a].t ?? 0) - (m[b].t ?? 0));
    for (const k of keys.slice(0, keys.length - CAP)) delete m[k];
  }
  writeMap(m);
}

export function clearTtsPos(articleId: string, sourceKey: string): void {
  const m = readMap();
  delete m[posKey(articleId, sourceKey)];
  writeMap(m);
}
