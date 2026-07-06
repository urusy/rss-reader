import { describe, it, expect, beforeEach } from "vitest";
import { authState, setAuthState, onUnauthorized } from "./auth";

describe("lib/auth state machine", () => {
  beforeEach(() => {
    setAuthState("unknown");
  });

  it("starts unknown and transitions via setAuthState", () => {
    expect(authState()).toBe("unknown");
    setAuthState("setup");
    expect(authState()).toBe("setup");
    setAuthState("authed");
    expect(authState()).toBe("authed");
  });

  it("onUnauthorized drops an authed session to login", () => {
    setAuthState("authed");
    onUnauthorized();
    expect(authState()).toBe("login");
  });

  it("onUnauthorized does not disturb setup flow", () => {
    // セットアップ画面表示中に保護 API が 401 を返しても setup 判定を壊さない。
    setAuthState("setup");
    onUnauthorized();
    expect(authState()).toBe("setup");
  });

  it("onUnauthorized keeps unknown until status resolves", () => {
    onUnauthorized();
    expect(authState()).toBe("unknown");
  });

  it("never persists anything to localStorage (cookie is HttpOnly)", () => {
    setAuthState("authed");
    expect(localStorage.getItem("auth_token")).toBeNull();
    expect(localStorage.length).toBe(0);
  });
});
