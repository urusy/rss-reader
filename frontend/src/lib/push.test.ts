import { describe, expect, it } from "vitest";

import { urlBase64ToUint8Array } from "./push";

describe("urlBase64ToUint8Array", () => {
  it("decodes standard base64url into bytes", () => {
    // "hello" -> base64 "aGVsbG8=" -> base64url "aGVsbG8"
    const out = urlBase64ToUint8Array("aGVsbG8");
    expect(Array.from(out)).toEqual([104, 101, 108, 108, 111]);
  });

  it("restores url-safe chars (- _) to (+ /)", () => {
    // bytes [251, 255] -> base64 "+/8=" -> base64url "-_8"
    const out = urlBase64ToUint8Array("-_8");
    expect(Array.from(out)).toEqual([251, 255]);
  });

  it("handles a 65-byte VAPID public key length", () => {
    // 65 raw bytes encode to 88 base64url chars (no padding). Round-trip length check.
    const bytes = new Uint8Array(65).map((_v, i) => i);
    let bin = "";
    for (const b of bytes) bin += String.fromCharCode(b);
    const b64url = btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
    const out = urlBase64ToUint8Array(b64url);
    expect(out.length).toBe(65);
    expect(Array.from(out)).toEqual(Array.from(bytes));
  });
});
