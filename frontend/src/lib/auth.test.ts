import { describe, it, expect, beforeEach } from "vitest";
import { getToken, setToken, clearToken, authToken } from "./auth";

describe("lib/auth token store", () => {
  beforeEach(() => {
    clearToken();
  });

  it("setToken persists to localStorage and signal", () => {
    setToken("abc123");
    expect(getToken()).toBe("abc123");
    expect(authToken()).toBe("abc123");
    expect(localStorage.getItem("auth_token")).toBe("abc123");
  });

  it("clearToken removes from localStorage and signal", () => {
    setToken("abc123");
    clearToken();
    expect(getToken()).toBeNull();
    expect(authToken()).toBeNull();
    expect(localStorage.getItem("auth_token")).toBeNull();
  });
});
