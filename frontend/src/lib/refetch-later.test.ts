import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { scheduleFollowUpRefetch } from "./refetch-later";

describe("scheduleFollowUpRefetch", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("既定では 3 秒後と 10 秒後に 1 回ずつ refetch を呼ぶ", () => {
    const refetch = vi.fn();
    scheduleFollowUpRefetch(refetch);

    expect(refetch).not.toHaveBeenCalled();
    vi.advanceTimersByTime(3000);
    expect(refetch).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(7000);
    expect(refetch).toHaveBeenCalledTimes(2);
    // それ以降は呼ばれない
    vi.advanceTimersByTime(60000);
    expect(refetch).toHaveBeenCalledTimes(2);
  });

  it("delays を指定するとその時刻に呼ぶ", () => {
    const refetch = vi.fn();
    scheduleFollowUpRefetch(refetch, [1000]);

    vi.advanceTimersByTime(999);
    expect(refetch).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(refetch).toHaveBeenCalledTimes(1);
  });

  it("cancel を呼ぶと未発火分は実行されない", () => {
    const refetch = vi.fn();
    const cancel = scheduleFollowUpRefetch(refetch, [1000, 5000]);

    vi.advanceTimersByTime(1000);
    expect(refetch).toHaveBeenCalledTimes(1);
    cancel();
    vi.advanceTimersByTime(60000);
    expect(refetch).toHaveBeenCalledTimes(1);
  });
});
