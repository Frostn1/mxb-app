import { describe, expect, it } from "vitest";
import { unwrapContentKey, wrapContentKey } from "../src/assetkey";

/** A master key secret: base64 of 32 bytes. */
const MASTER = btoa(String.fromCharCode(...Array.from({ length: 32 }, (_, i) => i + 1)));
const OTHER = btoa(String.fromCharCode(...Array.from({ length: 32 }, (_, i) => 200 - i)));

/** A content key: 32 bytes. */
const CEK = new Uint8Array(Array.from({ length: 32 }, (_, i) => (i * 7) & 0xff));

describe("wrap/unwrap", () => {
  it("round-trips a content key under the master key", async () => {
    const wrapped = await wrapContentKey(CEK, MASTER);
    expect(wrapped).not.toBeNull();
    const back = await unwrapContentKey(wrapped!, MASTER);
    expect(back).not.toBeNull();
    expect(Array.from(back!)).toEqual(Array.from(CEK));
  });

  it("does not put the content key in the wrapped form", async () => {
    // The point of wrapping: the stored string must not contain the key it protects.
    const wrapped = await wrapContentKey(CEK, MASTER);
    const raw = atob(wrapped!);
    const bytes = Uint8Array.from(raw, (c) => c.charCodeAt(0));
    // Slide the CEK across the wrapped bytes; it must not appear.
    let found = false;
    for (let i = 0; i + CEK.length <= bytes.length; i++) {
      if (CEK.every((b, j) => bytes[i + j] === b)) found = true;
    }
    expect(found).toBe(false);
  });

  it("wraps the same key to different bytes each time", async () => {
    // A fresh IV per wrap, so two assets sharing a key — or one re-registered — don't
    // produce an identical stored value that leaks the fact.
    expect(await wrapContentKey(CEK, MASTER)).not.toEqual(await wrapContentKey(CEK, MASTER));
  });

  it("will not unwrap under a different master key", async () => {
    const wrapped = await wrapContentKey(CEK, MASTER);
    expect(await unwrapContentKey(wrapped!, OTHER)).toBeNull();
  });

  it("refuses a tampered wrapped value", async () => {
    const wrapped = await wrapContentKey(CEK, MASTER);
    const raw = atob(wrapped!);
    const bytes = Uint8Array.from(raw, (c) => c.charCodeAt(0));
    bytes[bytes.length - 1] ^= 0x01; // flip a tag byte
    const tampered = btoa(String.fromCharCode(...bytes));
    expect(await unwrapContentKey(tampered, MASTER)).toBeNull();
  });

  it("is off when there is no master key", async () => {
    // A deployment with no secret cannot secure content, and must not fall back to some
    // fixed key — it produces nothing, and the caller turns that into a 503.
    expect(await wrapContentKey(CEK, undefined)).toBeNull();
    expect(await unwrapContentKey("anything", undefined)).toBeNull();
    expect(await wrapContentKey(CEK, "not-32-bytes")).toBeNull();
  });

  it("refuses a content key that is not 32 bytes", async () => {
    expect(await wrapContentKey(new Uint8Array(16), MASTER)).toBeNull();
  });
});
