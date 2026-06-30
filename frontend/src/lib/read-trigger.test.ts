import { describe, it, expect } from "vitest";
import { scrolledEnough } from "./read-trigger";

describe("scrolledEnough", () => {
  it("先頭（スクロールなし・長文）は false", () => {
    expect(
      scrolledEnough({ scrollTop: 0, clientHeight: 500, scrollHeight: 2000 }),
    ).toBe(false);
  });
  it("px しきい値を超えてスクロールしたら true", () => {
    expect(
      scrolledEnough({ scrollTop: 200, clientHeight: 500, scrollHeight: 2000 }),
    ).toBe(true);
  });
  it("短文で表示下端が割合しきい値に達したら true", () => {
    expect(
      scrolledEnough({ scrollTop: 50, clientHeight: 500, scrollHeight: 700 }),
    ).toBe(true);
  });
  it("px も割合も未達なら false", () => {
    expect(
      scrolledEnough({ scrollTop: 50, clientHeight: 300, scrollHeight: 2000 }),
    ).toBe(false);
  });
  it("scrollHeight が 0 のときは false（ゼロ除算を避ける）", () => {
    expect(
      scrolledEnough({ scrollTop: 0, clientHeight: 0, scrollHeight: 0 }),
    ).toBe(false);
  });
  it("オプションでしきい値を上書きできる", () => {
    expect(
      scrolledEnough(
        { scrollTop: 100, clientHeight: 0, scrollHeight: 2000 },
        { minPx: 50 },
      ),
    ).toBe(true);
  });
});
