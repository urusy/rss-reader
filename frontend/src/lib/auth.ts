// Client-side auth token store: localStorage + a module-scope signal, same shape
// as lib/theme.ts. Single-user client state lives here, not in the global store.
import { createSignal } from "solid-js";

const KEY = "auth_token";

const [token, setTokenSignal] = createSignal<string | null>(
  typeof localStorage !== "undefined" ? localStorage.getItem(KEY) : null,
);

/** Reactive token accessor (the login gate subscribes to this). */
export const authToken = token;

export function setToken(t: string): void {
  if (typeof localStorage !== "undefined") localStorage.setItem(KEY, t);
  setTokenSignal(t);
}

export function clearToken(): void {
  if (typeof localStorage !== "undefined") localStorage.removeItem(KEY);
  setTokenSignal(null);
}

export function getToken(): string | null {
  return token();
}
