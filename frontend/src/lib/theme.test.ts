import { test, expect, vi, beforeEach } from "vitest";
import * as theme from "./theme";

beforeEach(() => {
  localStorage.clear();
  vi.unstubAllGlobals();
  document.documentElement.className = "";
  document.documentElement.style.colorScheme = "";
});

test("initialTheme prefers localStorage (all 4 themes)", () => {
  for (const t of theme.THEMES) {
    localStorage.setItem("theme", t);
    expect(theme.initialTheme()).toBe(t);
  }
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
  expect(theme.initialTheme()).toBe("light");
});

test("applyTheme sets the right class and color-scheme per theme", () => {
  const el = document.documentElement;

  theme.applyTheme("light");
  expect(el.classList.contains("dark")).toBe(false);
  expect(el.className).toBe("");
  expect(el.style.colorScheme).toBe("light");

  theme.applyTheme("dark");
  expect(el.classList.contains("dark")).toBe(true);
  expect(el.style.colorScheme).toBe("dark");

  theme.applyTheme("graphite");
  expect(el.classList.contains("graphite")).toBe(true);
  expect(el.style.colorScheme).toBe("dark"); // graphite は暗色系

  theme.applyTheme("sepia");
  expect(el.classList.contains("sepia")).toBe(true);
  expect(el.style.colorScheme).toBe("light"); // sepia は明色系
});

test("applyTheme removes the previous theme class when switching", () => {
  const el = document.documentElement;
  theme.applyTheme("dark");
  theme.applyTheme("sepia");
  expect(el.classList.contains("dark")).toBe(false);
  expect(el.classList.contains("sepia")).toBe(true);
});

test("setTheme updates signal, storage and DOM", () => {
  theme.setTheme("graphite");
  expect(theme.theme()).toBe("graphite");
  expect(localStorage.getItem("theme")).toBe("graphite");
  expect(document.documentElement.classList.contains("graphite")).toBe(true);
});

test("toggleTheme cycles light -> dark -> graphite -> sepia -> light", () => {
  theme.setTheme("light");
  theme.toggleTheme();
  expect(theme.theme()).toBe("dark");
  theme.toggleTheme();
  expect(theme.theme()).toBe("graphite");
  theme.toggleTheme();
  expect(theme.theme()).toBe("sepia");
  theme.toggleTheme();
  expect(theme.theme()).toBe("light");
});

test("importing theme has no side effects", () => {
  expect(document.documentElement.className).toBe("");
  expect(localStorage.getItem("theme")).toBeNull();
});
