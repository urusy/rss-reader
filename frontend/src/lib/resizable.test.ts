import { describe, it, expect, beforeEach } from "vitest";
import { clampWidth, readStoredWidth } from "./resizable";

describe("clampWidth", () => {
  it("範囲内はそのまま返す", () => {
    expect(clampWidth(300, 200, 480)).toBe(300);
  });
  it("下限未満は min に丸める", () => {
    expect(clampWidth(120, 200, 480)).toBe(200);
  });
  it("上限超過は max に丸める", () => {
    expect(clampWidth(900, 200, 480)).toBe(480);
  });
  it("境界値はそのまま", () => {
    expect(clampWidth(200, 200, 480)).toBe(200);
    expect(clampWidth(480, 200, 480)).toBe(480);
  });
});

describe("readStoredWidth", () => {
  beforeEach(() => localStorage.clear());

  it("未保存なら fallback を（クランプして）返す", () => {
    expect(readStoredWidth("sidebar-w", 280, 200, 480)).toBe(280);
  });
  it("保存済みの有効値を数値として返す", () => {
    localStorage.setItem("sidebar-w", "360");
    expect(readStoredWidth("sidebar-w", 280, 200, 480)).toBe(360);
  });
  it("保存値が範囲外なら min/max にクランプする", () => {
    localStorage.setItem("sidebar-w", "9999");
    expect(readStoredWidth("sidebar-w", 280, 200, 480)).toBe(480);
    localStorage.setItem("sidebar-w", "10");
    expect(readStoredWidth("sidebar-w", 280, 200, 480)).toBe(200);
  });
  it("非数値・空文字は fallback にフォールバックする", () => {
    localStorage.setItem("sidebar-w", "abc");
    expect(readStoredWidth("sidebar-w", 280, 200, 480)).toBe(280);
    localStorage.setItem("sidebar-w", "");
    expect(readStoredWidth("sidebar-w", 280, 200, 480)).toBe(280);
  });
  it("fallback 自体も範囲にクランプして返す", () => {
    expect(readStoredWidth("missing-key", 1000, 200, 480)).toBe(480);
  });
});
