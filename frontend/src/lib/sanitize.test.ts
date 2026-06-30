import { describe, it, expect } from "vitest";
import { sanitizeArticleHtml } from "./sanitize";

describe("sanitizeArticleHtml", () => {
  it("埋め込み <style>（負マージンでレイアウトを壊す）を除去する", () => {
    // 実際に崩れていたメルペイ記事の再現。
    const dirty =
      '<style>p.codeboxbefore{margin-bottom:-70px;}</style>' +
      '<p class="codeboxbefore fontbold">As-Is</p>' +
      "<pre><code>func main() {}</code></pre>";
    const clean = sanitizeArticleHtml(dirty);
    expect(clean).not.toContain("<style");
    expect(clean).not.toContain("margin-bottom:-70px");
    // ラベルとコードは残る。
    expect(clean).toContain("As-Is");
    expect(clean).toContain("func main()");
  });

  it("<script> を除去する", () => {
    const clean = sanitizeArticleHtml('<p>hi</p><script>alert(1)</script>');
    expect(clean).not.toContain("<script");
    expect(clean).not.toContain("alert(1)");
    expect(clean).toContain("hi");
  });

  it("on* イベント属性（img onerror）を除去する", () => {
    const clean = sanitizeArticleHtml('<img src="x" onerror="alert(1)">');
    expect(clean).not.toContain("onerror");
    expect(clean).not.toContain("alert(1)");
  });

  it("inline style 属性を除去する", () => {
    const clean = sanitizeArticleHtml(
      '<p style="margin-bottom:-70px">x</p>',
    );
    expect(clean).not.toContain("style=");
    expect(clean).not.toContain("margin-bottom");
    expect(clean).toContain("x");
  });

  it("javascript: URL を除去する", () => {
    const clean = sanitizeArticleHtml('<a href="javascript:alert(1)">link</a>');
    expect(clean).not.toContain("javascript:");
    expect(clean).toContain("link");
  });

  it("安全な本文（段落・コード・リンク）は保持する", () => {
    const safe =
      '<h2>見出し</h2><p>本文</p>' +
      '<pre><code class="language-go">x := 1</code></pre>' +
      '<a href="https://example.com">元記事</a>';
    const clean = sanitizeArticleHtml(safe);
    expect(clean).toContain("見出し");
    expect(clean).toContain("本文");
    expect(clean).toContain("x := 1");
    expect(clean).toContain('href="https://example.com"');
  });
});
