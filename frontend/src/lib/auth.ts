// Client-side auth state. The session cookie is HttpOnly (invisible to JS), so
// the client only tracks a coarse state machine driven by GET /api/auth/status
// and 401 responses. localStorage に秘密は一切置かない。
import { createSignal } from "solid-js";

export type AuthState =
  | "unknown" // status 未取得（起動直後）
  | "setup" // 初回セットアップ（パスワード未設定）
  | "login" // パスワード入力待ち（セッションなし/失効）
  | "authed"; // 有効なセッションあり

const [state, setState] = createSignal<AuthState>("unknown");

/** Reactive auth state accessor (the login gate subscribes to this). */
export const authState = state;

export function setAuthState(s: AuthState): void {
  setState(s);
}

/**
 * api.ts が 401 を受けたときの合流点。セッション失効を検知してゲートへ戻す。
 * setup/unknown 中は上書きしない（初回セットアップ判定を壊さない）。
 */
export function onUnauthorized(): void {
  if (state() === "authed") setState("login");
}
