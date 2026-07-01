import { describe, it, expect } from "vitest";
import { resolveAction, isEditableTarget, stepId } from "./keyboard";

describe("resolveAction", () => {
  it("maps every binding", () => {
    expect(resolveAction({ key: "j" })).toBe("next");
    expect(resolveAction({ key: "k" })).toBe("prev");
    expect(resolveAction({ key: "Enter" })).toBe("open");
    expect(resolveAction({ key: "m" })).toBe("markRead");
    expect(resolveAction({ key: "o" })).toBe("openOriginal");
    expect(resolveAction({ key: "/" })).toBe("search");
    expect(resolveAction({ key: "g" })).toBe("gotoList");
    expect(resolveAction({ key: "?" })).toBe("toggleHelp");
  });

  it("returns null with ctrl/meta/alt", () => {
    expect(resolveAction({ key: "j", ctrlKey: true })).toBeNull();
    expect(resolveAction({ key: "j", metaKey: true })).toBeNull();
    expect(resolveAction({ key: "j", altKey: true })).toBeNull();
  });

  it("returns null for an unbound key", () => {
    expect(resolveAction({ key: "x" })).toBeNull();
  });
});

describe("isEditableTarget", () => {
  it("true for input/textarea/select", () => {
    for (const tag of ["input", "textarea", "select"]) {
      expect(isEditableTarget(document.createElement(tag))).toBe(true);
    }
  });

  it("true for contenteditable", () => {
    const div = document.createElement("div");
    div.setAttribute("contenteditable", "true");
    expect(isEditableTarget(div)).toBe(true);
  });

  it("false for button/div/null", () => {
    expect(isEditableTarget(document.createElement("button"))).toBe(false);
    expect(isEditableTarget(document.createElement("div"))).toBe(false);
    expect(isEditableTarget(null)).toBe(false);
  });
});

describe("stepId", () => {
  const ids = ["a", "b", "c"];

  it("next/prev from middle", () => {
    expect(stepId(ids, "b", 1)).toBe("c");
    expect(stepId(ids, "b", -1)).toBe("a");
  });

  it("from null picks first/last", () => {
    expect(stepId(ids, null, 1)).toBe("a");
    expect(stepId(ids, null, -1)).toBe("c");
  });

  it("clamps at the ends", () => {
    expect(stepId(ids, "c", 1)).toBe("c");
    expect(stepId(ids, "a", -1)).toBe("a");
  });

  it("unknown current treated as none", () => {
    expect(stepId(ids, "zzz", 1)).toBe("a");
    expect(stepId(ids, "zzz", -1)).toBe("c");
  });

  it("empty list returns null", () => {
    expect(stepId([], "a", 1)).toBeNull();
  });

  it("single item stays", () => {
    expect(stepId(["a"], "a", 1)).toBe("a");
    expect(stepId(["a"], "a", -1)).toBe("a");
  });
});
