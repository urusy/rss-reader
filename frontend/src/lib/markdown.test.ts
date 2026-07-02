import { describe, it, expect } from "vitest";
import { renderMarkdown } from "./markdown";

describe("renderMarkdown", () => {
  it("converts headings, bold and lists to HTML", () => {
    const html = renderMarkdown("# 見出し\n\n**太字** と *斜体*\n\n- a\n- b");
    expect(html).toContain("<h1");
    expect(html).toContain("見出し");
    expect(html).toContain("<strong>太字</strong>");
    expect(html).toContain("<em>斜体</em>");
    expect(html).toContain("<li>a</li>");
  });

  it("emits fenced code as <pre><code> with a language class for highlighting", () => {
    const html = renderMarkdown("```js\nconst x = 1;\n```");
    expect(html).toMatch(/<pre><code[^>]*class="[^"]*language-js/);
    expect(html).toContain("const x = 1;");
  });

  it("keeps a plain fenced block (no language) as <pre><code>", () => {
    const html = renderMarkdown("```\nvoid f() {}\n```");
    expect(html).toMatch(/<pre><code/);
    expect(html).toContain("void f() {}");
  });

  it("sanitizes dangerous HTML embedded in the markdown", () => {
    const html = renderMarkdown("正常\n\n<script>alert(1)</script>");
    expect(html).not.toContain("<script>");
    expect(html).toContain("正常");
  });

  it("handles null/undefined without throwing", () => {
    expect(renderMarkdown(null)).toBe("");
    expect(renderMarkdown(undefined)).toBe("");
  });
});
