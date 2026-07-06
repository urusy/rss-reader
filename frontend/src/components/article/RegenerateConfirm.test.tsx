// 再要約/再翻訳の確認ガード: 誤タップ1回で Claude が呼ばれないこと、
// 確認して初めて onConfirm が走ることを固定する。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@solidjs/testing-library";
import RegenerateConfirm from "./RegenerateConfirm";

function renderConfirm(onConfirm: () => void, busy = false) {
  return render(() => (
    <RegenerateConfirm
      label="要約"
      trigger="再要約 (Claude)"
      busyText="要約中…"
      busy={busy}
      disabled={busy}
      variant="default"
      onConfirm={onConfirm}
    />
  ));
}

beforeEach(() => cleanup());

describe("RegenerateConfirm", () => {
  it("トリガーを押しただけでは onConfirm を呼ばない（ダイアログ表示のみ）", async () => {
    const onConfirm = vi.fn();
    renderConfirm(onConfirm);
    fireEvent.click(screen.getByRole("button", { name: "再要約 (Claude)" }));
    expect(await screen.findByText("要約を作り直しますか？")).toBeTruthy();
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("キャンセルでは onConfirm を呼ばない", async () => {
    const onConfirm = vi.fn();
    renderConfirm(onConfirm);
    fireEvent.click(screen.getByRole("button", { name: "再要約 (Claude)" }));
    await screen.findByText("要約を作り直しますか？");
    fireEvent.click(screen.getByText("キャンセル"));
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("「作り直す」で onConfirm が一度だけ呼ばれる", async () => {
    const onConfirm = vi.fn();
    renderConfirm(onConfirm);
    fireEvent.click(screen.getByRole("button", { name: "再要約 (Claude)" }));
    await screen.findByText("要約を作り直しますか？");
    fireEvent.click(screen.getByText("作り直す"));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("busy 中は busyText を表示し disabled", () => {
    renderConfirm(vi.fn(), true);
    const btn = screen.getByRole("button", { name: "要約中…" });
    expect(btn.hasAttribute("disabled")).toBe(true);
  });
});
