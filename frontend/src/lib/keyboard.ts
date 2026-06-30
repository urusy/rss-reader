// Global keyboard shortcuts for the Reader. Two layers like lib/theme.ts:
// pure logic (resolveAction / isEditableTarget / stepId — unit-tested) and a
// side-effecting hook (useKeyboardShortcuts) that wires the window listener.
import { onCleanup, onMount } from "solid-js";
import { useNavigate, useSearchParams } from "@solidjs/router";
import { api } from "@/lib/api";
import { useApp } from "@/lib/store";

export type KeyAction =
  | "next"
  | "prev"
  | "open"
  | "markRead"
  | "openOriginal"
  | "search"
  | "gotoList"
  | "toggleHelp";

export interface KeyEventLike {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
}

export const KEY_BINDINGS: Readonly<Record<string, KeyAction>> = {
  j: "next",
  k: "prev",
  Enter: "open",
  m: "markRead",
  o: "openOriginal",
  "/": "search",
  g: "gotoList",
  "?": "toggleHelp",
};

/** Resolve an event-like object to an action. ctrl/meta/alt → null (don't steal
 * browser/OS shortcuts). shift is allowed (needed for "?"). */
export function resolveAction(e: KeyEventLike): KeyAction | null {
  if (e.ctrlKey || e.metaKey || e.altKey) return null;
  return KEY_BINDINGS[e.key] ?? null;
}

/** True when focus is in a text-editing element (suppress shortcuts there). */
export function isEditableTarget(el: EventTarget | null): boolean {
  if (!(el instanceof HTMLElement)) return false;
  const tag = el.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    // isContentEditable is unimplemented in jsdom (undefined); fall back to the
    // attribute so the result is always a real boolean.
    el.isContentEditable === true ||
    el.getAttribute("contenteditable") === "true"
  );
}

/** Step the selection in the list. Clamps at the ends (no wrap). Empty → null.
 * Unknown/absent current → first (dir 1) or last (dir -1). */
export function stepId(
  ids: readonly string[],
  current: string | null,
  dir: 1 | -1,
): string | null {
  if (ids.length === 0) return null;
  const i = current ? ids.indexOf(current) : -1;
  if (i === -1) return dir === 1 ? ids[0] : ids[ids.length - 1];
  const next = i + dir;
  if (next < 0 || next >= ids.length) return ids[i];
  return ids[next];
}

/** Side-effecting hook: register a single window keydown listener. Call inside
 * AppProvider + Router context (mount <KeyboardShortcuts /> once). */
export function useKeyboardShortcuts(): void {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const app = useApp();

  const selectedId = (): string | null => {
    const a = searchParams.article;
    return (Array.isArray(a) ? a[0] : a) ?? null;
  };

  const dispatch = (action: KeyAction): void => {
    const items = app.state.navItems;
    const ids = items.map((it) => it.id);
    const cur = selectedId();

    switch (action) {
      case "next":
      case "prev": {
        const id = stepId(ids, cur, action === "next" ? 1 : -1);
        if (id) setSearchParams({ article: id });
        return;
      }
      case "open": {
        const id = cur ?? ids[0] ?? null;
        if (id) setSearchParams({ article: id });
        return;
      }
      case "markRead": {
        if (!cur) return;
        void api
          .markRead(cur, true)
          .catch((e) => console.error("keyboard mark-read failed", e));
        app.markReadLocal(cur);
        return;
      }
      case "openOriginal": {
        const it = items.find((x) => x.id === cur);
        if (it) window.open(it.url, "_blank", "noopener,noreferrer");
        return;
      }
      case "search":
        navigate("/search");
        return;
      case "gotoList":
        setSearchParams({ article: null });
        return;
      case "toggleHelp":
        app.toggleHelp();
        return;
    }
  };

  const onKeyDown = (e: KeyboardEvent): void => {
    // Esc closes the help overlay regardless of focus.
    if (e.key === "Escape" && app.state.helpOpen) {
      app.closeHelp();
      return;
    }
    if (isEditableTarget(e.target)) return;
    const action = resolveAction(e);
    if (!action) return;
    e.preventDefault();
    dispatch(action);
  };

  onMount(() => window.addEventListener("keydown", onKeyDown));
  onCleanup(() => window.removeEventListener("keydown", onKeyDown));
}
