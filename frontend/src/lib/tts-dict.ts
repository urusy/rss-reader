/**
 * 読み上げ (TTS) の発音辞書 — 型と組み込み初期辞書（純データ・副作用なし）。
 *
 * 日本語音声エンジンは英字略語/英単語を誤読する（例: "AI" → 「あい」）。
 * 読み上げ直前に本文をこの辞書でカタカナ読みへ置換して補正する（tts-normalize.ts）。
 * ここはデータ層なので置換ロジックや localStorage には触れない。
 */

export interface DictEntry {
  /** 元表記。例: "AI" / "Google"。 */
  match: string;
  /** カタカナ読み。例: "エーアイ" / "グーグル"。 */
  reading: string;
  /**
   * 綴りの大小を区別するか。
   * true  = 略語向け（大文字綴りだけを拾い、"aid"/"Air" 等の小文字英単語を誤爆させない）。
   * false = 一般英単語向け（"google"/"Google" どちらも拾う）。
   */
  caseSensitive: boolean;
}

const abbr = (match: string, reading: string): DictEntry => ({
  match,
  reading,
  caseSensitive: true,
});
const word = (match: string, reading: string): DictEntry => ({
  match,
  reading,
  caseSensitive: false,
});

/**
 * 組み込み初期辞書。ユーザー辞書（tts-dict-store）とマージして使う（ユーザー優先）。
 * v1 は誤読が目立つ IT 略語＋主要固有名詞に絞る。過不足はユーザーが Settings で補える。
 */
export const BUILTIN_DICT: DictEntry[] = [
  // --- 英字略語（大小区別あり） ---
  abbr("AI", "エーアイ"),
  abbr("UI", "ユーアイ"),
  abbr("UX", "ユーエックス"),
  abbr("API", "エーピーアイ"),
  abbr("URL", "ユーアールエル"),
  abbr("LLM", "エルエルエム"),
  abbr("RSS", "アールエスエス"),
  abbr("HTTP", "エイチティーティーピー"),
  abbr("HTTPS", "エイチティーティーピーエス"),
  abbr("CPU", "シーピーユー"),
  abbr("GPU", "ジーピーユー"),
  abbr("NPU", "エヌピーユー"),
  abbr("OS", "オーエス"),
  abbr("SDK", "エスディーケー"),
  abbr("CLI", "シーエルアイ"),
  abbr("IDE", "アイディーイー"),
  abbr("JSON", "ジェイソン"),
  abbr("HTML", "エイチティーエムエル"),
  abbr("CSS", "シーエスエス"),
  abbr("SQL", "エスキューエル"),
  abbr("DB", "ディービー"),
  abbr("IoT", "アイオーティー"),
  abbr("SNS", "エスエヌエス"),
  abbr("PDF", "ピーディーエフ"),
  abbr("USB", "ユーエスビー"),
  abbr("PC", "ピーシー"),
  abbr("ID", "アイディー"),
  abbr("GPT", "ジーピーティー"),
  abbr("ML", "エムエル"),
  abbr("VR", "ブイアール"),
  abbr("AR", "エーアール"),
  abbr("OSS", "オーエスエス"),
  abbr("CI", "シーアイ"),
  // --- 一般英単語・固有名詞（大小区別なし） ---
  word("OpenAI", "オープンエーアイ"),
  word("ChatGPT", "チャットジーピーティー"),
  word("Anthropic", "アンソロピック"),
  word("Claude", "クロード"),
  word("Google", "グーグル"),
  word("GitHub", "ギットハブ"),
  word("Amazon", "アマゾン"),
  word("Microsoft", "マイクロソフト"),
  word("Apple", "アップル"),
  word("YouTube", "ユーチューブ"),
  word("iPhone", "アイフォン"),
  word("iPad", "アイパッド"),
  word("Android", "アンドロイド"),
  word("Windows", "ウィンドウズ"),
  word("Linux", "リナックス"),
  word("Chrome", "クローム"),
];
