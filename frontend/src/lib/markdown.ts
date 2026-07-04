import { marked } from "marked";
import { sanitizeArticleHtml } from "@/lib/sanitize";

/**
 * LLM 由来の Markdown（要約・翻訳・ダイジェスト）を表示用の安全な HTML に変換する。
 * marked で HTML 化 → DOMPurify で浄化（既存 sanitize と同じ多層防御）。
 * コードのシンタックスハイライトは描画後に highlight.js が付与する（`Prose` 参照）。
 * marked は fenced code を `<pre><code class="language-xxx">` に展開し、DOMPurify は
 * class を保持するので、その言語ヒントが highlight.js にそのまま渡る。
 */
export function renderMarkdown(md: string | null | undefined): string {
  const html = marked.parse(md ?? "", { async: false }) as string;
  return sanitizeArticleHtml(html ?? "");
}
