import { describe, it, expect, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@solidjs/testing-library";
import SourceLabel from "./SourceLabel";

afterEach(cleanup);

describe("SourceLabel", () => {
  it("ソース名を表示する", () => {
    render(() => (
      <SourceLabel name="Example Blog" url="https://example.com/post" />
    ));
    expect(screen.getByText("Example Blog")).toBeTruthy();
  });

  it("記事URLのオリジンから favicon を読み込む", () => {
    const { container } = render(() => (
      <SourceLabel name="Example" url="https://blog.example.com/2026/x" />
    ));
    const img = container.querySelector("img");
    expect(img).toBeTruthy();
    expect(img!.getAttribute("src")).toBe("https://blog.example.com/favicon.ico");
  });

  it("favicon 読み込み失敗で頭文字アバターに切り替わる", () => {
    const { container } = render(() => (
      <SourceLabel name="Example" url="https://example.com/post" />
    ));
    const img = container.querySelector("img")!;
    fireEvent.error(img);
    expect(container.querySelector("img")).toBeNull();
    // 頭文字 "E" のアバターが出る（名前スパンは "Example" なので別要素）。
    expect(screen.getByText("E")).toBeTruthy();
  });

  it("URL が無効なら favicon を出さずアバターにする", () => {
    const { container } = render(() => (
      <SourceLabel name="名無しブログ" url="not-a-url" />
    ));
    expect(container.querySelector("img")).toBeNull();
    expect(screen.getByText("名")).toBeTruthy();
  });
});
