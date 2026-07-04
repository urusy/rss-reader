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

  // --- inline style だけでコードを表すフィード（Blogger 等）の救済 ---

  it("安全なタイポ系 inline style（font-family/color/white-space）は残す", () => {
    const clean = sanitizeArticleHtml(
      '<span style="font-family: \'Roboto Mono\', monospace; color: #188038; white-space: pre-wrap;">code</span>',
    );
    expect(clean).toMatch(/font-family/i);
    expect(clean).toMatch(/white-space/i);
    expect(clean.toLowerCase()).toContain("#188038");
  });

  it("レイアウト破壊系 style（margin/position/width/display）は落とし、色だけ残す", () => {
    const clean = sanitizeArticleHtml(
      '<div style="margin: -100px; position: absolute; width: 9999px; display: none; color: red;">x</div>',
    );
    expect(clean).not.toMatch(/margin/i);
    expect(clean).not.toMatch(/position/i);
    expect(clean).not.toMatch(/width/i);
    expect(clean).not.toMatch(/display/i);
    expect(clean).toMatch(/color:\s*red/i);
  });

  it("等幅フォント指定の要素に feed-mono クラスを付ける", () => {
    const clean = sanitizeArticleHtml(
      '<span style="font-family: \'Roboto Mono\', monospace;">class Foo {}</span>',
    );
    expect(clean).toContain("feed-mono");
  });

  it("等幅でない要素には feed-mono を付けない", () => {
    const clean = sanitizeArticleHtml(
      '<span style="font-family: Georgia, serif; color: blue;">prose</span>',
    );
    expect(clean).not.toContain("feed-mono");
  });

  // 監査 LOW: リンクは新規タブ + noopener に統一（reverse tabnabbing 対策）。
  it("href 付きリンクに target=_blank と rel=noopener noreferrer を強制する", () => {
    const clean = sanitizeArticleHtml('<a href="https://example.com">link</a>');
    expect(clean).toContain('target="_blank"');
    expect(clean).toContain('rel="noopener noreferrer"');
  });

  it("feed 側の rel 値（opener 誘発）は上書きされる", () => {
    const clean = sanitizeArticleHtml(
      '<a href="https://example.com" target="_blank" rel="opener">link</a>',
    );
    expect(clean).toContain('rel="noopener noreferrer"');
    expect(clean).not.toContain('rel="opener"');
  });

  it("href の無いアンカー（脚注アンカー等）には付けない", () => {
    const clean = sanitizeArticleHtml('<a name="fn1">note</a>');
    expect(clean).not.toContain("noopener");
  });
});
