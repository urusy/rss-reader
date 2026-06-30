import { describe, it, expect } from "vitest";
import { htmlToPlainText, splitSentences } from "./tts";

describe("htmlToPlainText", () => {
  it("strips tags and normalizes whitespace", () => {
    expect(htmlToPlainText("<p>Hello   <b>world</b></p>\n<p>Bye</p>")).toBe(
      "Hello world Bye",
    );
  });

  it("returns empty string for tag-only html", () => {
    expect(htmlToPlainText("<br><hr>")).toBe("");
  });
});

describe("splitSentences", () => {
  it("splits on Japanese and Latin sentence enders", () => {
    expect(splitSentences("これは一文。次の文！最後？")).toEqual([
      "これは一文。",
      "次の文！",
      "最後？",
    ]);
    expect(splitSentences("First. Second! Third?")).toEqual([
      "First.",
      "Second!",
      "Third?",
    ]);
  });

  it("splits on newlines and drops empty chunks", () => {
    expect(splitSentences("a\n\n\nb\n")).toEqual(["a", "b"]);
  });

  it("keeps a single unterminated sentence", () => {
    expect(splitSentences("no terminator here")).toEqual([
      "no terminator here",
    ]);
  });
});
