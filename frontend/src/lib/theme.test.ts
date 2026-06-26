import { test, expect, vi, beforeEach } from "vitest";
import * as theme from "./theme";

beforeEach(() => {
  localStorage.clear();
  vi.unstubAllGlobals();
  document.documentElement.className = "";
  document.documentElement.style.colorScheme = "";
});

test("initialTheme prefers localStorage", () => {
  localStorage.setItem("theme", "dark");
  expect(theme.initialTheme()).toBe("dark");
  localStorage.setItem("theme", "light");
  expect(theme.initialTheme()).toBe("light");
});

test("initialTheme ignores invalid stored value and falls back to prefers", () => {
  localStorage.setItem("theme", "blue");
  vi.stubGlobal("matchMedia", () => ({ matches: true }));
  expect(theme.initialTheme()).toBe("dark");
});

test("initialTheme follows prefers when unset", () => {
  vi.stubGlobal("matchMedia", () => ({ matches: true }));
  expect(theme.initialTheme()).toBe("dark");
  vi.stubGlobal("matchMedia", () => ({ matches: false }));
  expect(theme.initialTheme()).toBe("light");
});

test("initialTheme defaults to light when matchMedia is unavailable", () => {
  // jsdom 既定で window.matchMedia は undefined → optional-chaining ガードで light
  expect(theme.initialTheme()).toBe("light");
});

test("applyTheme reflects to DOM", () => {
  theme.applyTheme("dark");
  expect(document.documentElement.classList.contains("dark")).toBe(true);
  expect(document.documentElement.style.colorScheme).toBe("dark");
  theme.applyTheme("light");
  expect(document.documentElement.classList.contains("dark")).toBe(false);
  expect(document.documentElement.style.colorScheme).toBe("light");
});

test("setTheme updates signal, storage and DOM", () => {
  theme.setTheme("dark");
  expect(theme.theme()).toBe("dark");
  expect(localStorage.getItem("theme")).toBe("dark");
  expect(document.documentElement.classList.contains("dark")).toBe(true);
});

test("toggleTheme round-trips", () => {
  theme.setTheme("light");
  theme.toggleTheme();
  expect(theme.theme()).toBe("dark");
  theme.toggleTheme();
  expect(theme.theme()).toBe("light");
});

test("importing theme has no side effects", () => {
  // モジュール eval は環境を読まない（jsdom で matchMedia 未定義でも安全）
  expect(document.documentElement.classList.contains("dark")).toBe(false);
  expect(localStorage.getItem("theme")).toBeNull();
});
