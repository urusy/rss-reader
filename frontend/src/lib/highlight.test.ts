import { describe, it, expect } from "vitest";
import { highlightWithin } from "./highlight";
import { renderMarkdown } from "./markdown";

describe("highlightWithin", () => {
  it("adds hljs classes to code blocks with a declared language", () => {
    const el = document.createElement("div");
    el.innerHTML = renderMarkdown("```js\nconst x = 1;\n```");
    highlightWithin(el);
    const code = el.querySelector("pre code")!;
    expect(code.classList.contains("hljs")).toBe(true);
    // トークンが span で色分けされている（keyword 等）。
    expect(code.querySelector("span[class^='hljs-']")).not.toBeNull();
  });

  it("auto-detects and highlights a fenced block without a language", () => {
    const el = document.createElement("div");
    el.innerHTML = renderMarkdown("```\ndef greet():\n    return 42\n```");
    highlightWithin(el);
    const code = el.querySelector("pre code")!;
    expect(code.classList.contains("hljs")).toBe(true);
  });

  it("does not re-highlight an already highlighted block", () => {
    const el = document.createElement("div");
    el.innerHTML = renderMarkdown("```js\nconst x = 1;\n```");
    highlightWithin(el);
    const first = el.querySelector("pre code")!.innerHTML;
    highlightWithin(el); // 二回目は data-highlighted ガードで無視
    expect(el.querySelector("pre code")!.innerHTML).toBe(first);
  });

  it("leaves prose without code blocks untouched", () => {
    const el = document.createElement("div");
    el.innerHTML = renderMarkdown("ただの段落です。");
    expect(() => highlightWithin(el)).not.toThrow();
    expect(el.querySelector("pre code")).toBeNull();
  });
});
