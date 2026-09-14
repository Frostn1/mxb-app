import { describe, expect, it } from "vitest";
import {
  currentMasterVersion,
  rewrapToCurrent,
  unwrapContentKey,
  wrapContentKey,
  wrappedVersion,
} from "../src/assetkey";

/** A master key secret: base64 of 32 bytes. */
const MASTER = btoa(String.fromCharCode(...Array.from({ length: 32 }, (_, i) => i + 1)));
const OTHER = btoa(String.fromCharCode(...Array.from({ length: 32 }, (_, i) => 200 - i)));

/** The env shapes callers pass. */
const V1 = { MXB_ASSET_MASTER_KEY: MASTER };
const V1_OTHER = { MXB_ASSET_MASTER_KEY: OTHER };
const V1_AND_V2 = {
  MXB_ASSET_MASTER_KEYS: JSON.stringify({ "1": MASTER, "2": OTHER }),
  MXB_ASSET_MASTER_KEY_VERSION: "2",
};
const ONLY_V2 = { MXB_ASSET_MASTER_KEYS: JSON.stringify({ "2": OTHER }), MXB_ASSET_MASTER_KEY_VERSION: "2" };

/** A content key: 32 bytes. */
const CEK = new Uint8Array(Array.from({ length: 32 }, (_, i) => (i * 7) & 0xff));

/** The base64 body of a stored value, without its `<version>:` tag. */
const body = (wrapped: string) => wrapped.slice(wrapped.indexOf(":") + 1);

describe("wrap/unwrap", () => {
  it("round-trips a content key under the master key", async () => {
    const wrapped = await wrapContentKey(CEK, V1);
    expect(wrapped).not.toBeNull();
    expect(wrappedVersion(wrapped!)).toBe("1");
    const back = await unwrapContentKey(wrapped!, V1);
    expect(back).not.toBeNull();
    expect(Array.from(back!)).toEqual(Array.from(CEK));
  });

  it("does not put the content key in the wrapped form", async () => {
    const wrapped = await wrapContentKey(CEK, V1);
    const bytes = Uint8Array.from(atob(body(wrapped!)), (c) => c.charCodeAt(0));
    let found = false;
    for (let i = 0; i + CEK.length <= bytes.length; i++) {
      if (CEK.every((b, j) => bytes[i + j] === b)) found = true;
    }
    expect(found).toBe(false);
  });

  it("wraps the same key to different bytes each time", async () => {
    expect(await wrapContentKey(CEK, V1)).not.toEqual(await wrapContentKey(CEK, V1));
  });

  it("will not unwrap under a different master key", async () => {
    const wrapped = await wrapContentKey(CEK, V1);
    expect(await unwrapContentKey(wrapped!, V1_OTHER)).toBeNull();
  });

  it("refuses a tampered wrapped value", async () => {
    const wrapped = await wrapContentKey(CEK, V1);
    const bytes = Uint8Array.from(atob(body(wrapped!)), (c) => c.charCodeAt(0));
    bytes[bytes.length - 1] ^= 0x01; // flip a tag byte
    const tampered = `1:${btoa(String.fromCharCode(...bytes))}`;
    expect(await unwrapContentKey(tampered, V1)).toBeNull();
  });

  it("is off when there is no master key", async () => {
    expect(await wrapContentKey(CEK, {})).toBeNull();
    expect(await unwrapContentKey("anything", {})).toBeNull();
    expect(await wrapContentKey(CEK, { MXB_ASSET_MASTER_KEY: "not-32-bytes" })).toBeNull();
  });

  it("refuses a content key that is not 32 bytes", async () => {
    expect(await wrapContentKey(new Uint8Array(16), V1)).toBeNull();
  });
});

describe("rotation", () => {
  it("reports the current version, and null when unconfigured", () => {
    expect(currentMasterVersion(V1)).toBe("1");
    expect(currentMasterVersion(V1_AND_V2)).toBe("2");
    expect(currentMasterVersion({})).toBeNull();
  });

  it("treats a legacy untagged value as version 1", async () => {
    // A row wrapped before versioning is plain base64, no `<version>:` tag.
    const legacy = body(await wrapContentKey(CEK, V1).then((w) => w!));
    expect(wrappedVersion(legacy)).toBe("1");
    const back = await unwrapContentKey(legacy, V1);
    expect(Array.from(back!)).toEqual(Array.from(CEK));
  });

  it("re-wraps a v1 value to the current version, and the old key can then be dropped", async () => {
    const v1 = await wrapContentKey(CEK, V1);
    expect(wrappedVersion(v1!)).toBe("1");

    const rewrapped = await rewrapToCurrent(v1!, V1_AND_V2);
    expect(rewrapped).not.toBeNull();
    expect(wrappedVersion(rewrapped!)).toBe("2");
    expect(Array.from((await unwrapContentKey(rewrapped!, V1_AND_V2))!)).toEqual(Array.from(CEK));

    // Once rotated, the row opens with only the new key present.
    expect(Array.from((await unwrapContentKey(rewrapped!, ONLY_V2))!)).toEqual(Array.from(CEK));
  });

  it("cannot re-wrap a row whose old key is gone", async () => {
    const v1 = await wrapContentKey(CEK, V1);
    // Only v2 is configured, but the row is v1 — its key is missing, so it must fail loudly.
    expect(await rewrapToCurrent(v1!, ONLY_V2)).toBeNull();
  });
});
