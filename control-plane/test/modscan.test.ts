import { describe, expect, it } from "vitest";
import {
  ScanError,
  ZipStreamScanner,
  badName,
  checkEntries,
  checkPntHead,
  executableMagic,
  readCentralDirectory,
} from "../src/modscan";
import { fakePe, makeZip, type ZipEntry } from "./modfakes";

/** Run the full check over a zip, streamed in awkward chunk sizes. */
async function scan(zip: Uint8Array, chunk = 997): Promise<string | null> {
  try {
    const read = async (o: number, n: number) => zip.subarray(o, o + n);
    const entries = await readCentralDirectory(read, zip.length);
    checkEntries(entries);
    const s = new ZipStreamScanner(entries);
    for (let o = 0; o < zip.length; o += chunk) {
      await s.push(zip.slice(o, o + chunk));
      if (s.failed) throw s.failed;
    }
    await s.finish();
    if (s.failed) throw s.failed;
    return null;
  } catch (err) {
    if (err instanceof ScanError) return err.message;
    throw err;
  }
}

const texture = new Uint8Array(20_000).map((_, i) => (i * 13) & 0xff);
const clean: ZipEntry[] = [
  { name: "tracks/Careless/", data: "" },
  { name: "tracks/Careless/terrain.hmap", data: texture },
  { name: "tracks/Careless/track.ini", data: "[track]\nname=Careless\n", method: 0 },
];

describe("names", () => {
  it("refuses traversal, absolute paths, programs and archives it can't open", () => {
    expect(badName("../../evil.pnt")).toMatch(/escapes/);
    expect(badName("/etc/x")).toMatch(/absolute/);
    expect(badName("C:\\x.pnt")).toMatch(/absolute/);
    expect(badName("bikes/plugin.DLL")).toMatch(/executable/);
    expect(badName("readme.lnk")).toMatch(/executable/);
    expect(badName("extra.rar")).toMatch(/can't be checked/);
    expect(badName("tracks/a/b.edf")).toBeNull();
    expect(badName("paints/My Paint.pnt")).toBeNull();
  });
});

describe("signatures", () => {
  it("knows a program by its bytes, not by two letters", () => {
    expect(executableMagic(fakePe())).toMatch(/PE/);
    expect(executableMagic(new Uint8Array([0x7f, 0x45, 0x4c, 0x46, 1]))).toMatch(/ELF/);
    expect(executableMagic(new TextEncoder().encode("#!/bin/sh\n"))).toMatch(/script/);
    const mzText = new Uint8Array(0x100).fill(0x41);
    mzText[0] = 0x4d;
    mzText[1] = 0x5a;
    // e_lfanew pointing inside the head at something that isn't "PE\0\0": not a program.
    new DataView(mzText.buffer).setUint32(0x3c, 0x40, true);
    expect(executableMagic(mzText)).toBeNull();
    expect(executableMagic(new TextEncoder().encode("PNT\0paint"))).toBeNull();
  });
});

describe("archives", () => {
  it("passes a clean track", async () => {
    expect(await scan(await makeZip(clean))).toBeNull();
    expect(await scan(await makeZip(clean), 7)).toBeNull();
  });

  it("refuses a DLL by name", async () => {
    expect(await scan(await makeZip([...clean, { name: "plugins/hook.dll", data: "x" }]))).toMatch(/hook\.dll/);
  });

  it("refuses a program renamed to a texture, stored or deflated", async () => {
    for (const method of [0, 8] as const) {
      const zip = await makeZip([...clean, { name: "tracks/Careless/sky.tga", data: fakePe(), method }]);
      expect(await scan(zip)).toMatch(/sky\.tga is a Windows program/);
    }
  });

  it("refuses an encrypted entry, which it can't look into", async () => {
    expect(await scan(await makeZip([{ name: "a.pnt", data: "x", encrypted: true }]))).toMatch(/encrypted/);
  });

  it("looks inside a nested pkz and refuses what it hides", async () => {
    const inner = await makeZip([{ name: "x/readme.txt", data: "hi" }, { name: "x/run.exe", data: "MZ" }]);
    expect(await scan(await makeZip([{ name: "Careless.pkz", data: inner }]))).toMatch(/run\.exe.*nested/);
    const sneaky = await makeZip([{ name: "x/data.bin", data: fakePe() }]);
    expect(await scan(await makeZip([{ name: "Careless.pkz", data: sneaky, method: 0 }]))).toMatch(/data\.bin is a Windows program/);
  });

  it("passes a clean nested pkz, the common track-in-a-zip shape", async () => {
    const inner = await makeZip(clean);
    expect(await scan(await makeZip([{ name: "readme.txt", data: "drop the pkz in mods/tracks" }, { name: "Careless.pkz", data: inner }]), 4096)).toBeNull();
  });

  it("refuses three levels of archive", async () => {
    const deep = await makeZip([{ name: "a.pnt", data: "x" }]);
    const mid = await makeZip([{ name: "deep.zip", data: deep }]);
    expect(await scan(await makeZip([{ name: "outer.pkz", data: mid }]))).toMatch(/inside an archive inside/);
  });

  it("refuses what isn't a zip at all", async () => {
    expect(await scan(new TextEncoder().encode("this is not an archive at all, just text"))).toMatch(/not a zip/);
  });
});

describe("paints", () => {
  it("takes a paint, refuses a program or an archive dressed as one", () => {
    expect(() => checkPntHead(new TextEncoder().encode("PNT\0data"), 8)).not.toThrow();
    expect(() => checkPntHead(fakePe(), 512)).toThrow(/Windows program/);
    expect(() => checkPntHead(new Uint8Array([0x50, 0x4b, 3, 4]), 4)).toThrow(/zip/);
    expect(() => checkPntHead(new Uint8Array(4), 65 * 1024 * 1024)).toThrow(/at most/);
  });
});
