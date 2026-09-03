/**
 * The paint decoder, driven end to end.
 *
 * A `.pnt` is built here rather than fixtured: the format is a header, a table and a raw
 * deflate stream per sheet, and writing it out is the only way to assert that the reader
 * agrees with `paint.rs` about every offset. The PNG that comes out is decoded again, so the
 * pixels are checked rather than the byte count.
 */

import { describe, expect, it } from "vitest";
import {
  encodePng,
  imageTable,
  pickImage,
  renderThumb,
  type PaintSource,
  type PntImage,
} from "../src/pntthumb";

const HEADER_SIZE = 108;
const NAME_SIZE = 100;
const IMAGE_HEADER_SIZE = NAME_SIZE + 4 + 4 + 16 + 4;
const IMAGE_PADDING = 8;

interface Sheet {
  name: string;
  width: number;
  height: number;
  /** RGBA, row-major, in the file's own row order. */
  rgba: Uint8Array;
}

async function deflateRaw(data: Uint8Array): Promise<Uint8Array> {
  return collect(streamOf(data).pipeThrough(new CompressionStream("deflate-raw")));
}

async function inflate(data: Uint8Array): Promise<Uint8Array> {
  return collect(streamOf(data).pipeThrough(new DecompressionStream("deflate")));
}

function streamOf(data: Uint8Array): ReadableStream<Uint8Array> {
  return new ReadableStream({
    start(c) {
      c.enqueue(data);
      c.close();
    },
  });
}

async function collect(stream: ReadableStream<Uint8Array>): Promise<Uint8Array> {
  const parts: Uint8Array[] = [];
  const reader = stream.getReader();
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    parts.push(value);
  }
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

/** A `.pnt` in the layout `paint.rs` writes. */
async function pnt(sheets: Sheet[]): Promise<Uint8Array> {
  const parts: Uint8Array[] = [];
  const head = new Uint8Array(HEADER_SIZE);
  head.set([0x50, 0x4e, 0x54, 0x00]);
  new DataView(head.buffer).setUint32(HEADER_SIZE - 4, sheets.length, true);
  parts.push(head);

  for (const sheet of sheets) {
    const payload = await deflateRaw(sheet.rgba);
    const header = new Uint8Array(IMAGE_HEADER_SIZE + IMAGE_PADDING);
    const view = new DataView(header.buffer);
    for (let i = 0; i < sheet.name.length; i++) header[i] = sheet.name.charCodeAt(i);
    view.setUint32(NAME_SIZE, sheet.width, true);
    view.setUint32(NAME_SIZE + 4, sheet.height, true);
    // The md5 the format carries between the dimensions and the length is not read back.
    view.setUint32(NAME_SIZE + 8 + 16, payload.length + IMAGE_PADDING, true);
    parts.push(header, payload);
  }
  return concat(parts);
}

/** Reads a paint out of memory, in chunks small enough to cross every row boundary. */
function sourceOf(buf: Uint8Array, chunk = 7): PaintSource {
  return {
    size: buf.length,
    async read(offset, length) {
      return buf.subarray(offset, offset + length);
    },
    async stream(offset, length) {
      const slice = buf.subarray(offset, offset + length);
      let at = 0;
      return new ReadableStream({
        pull(c) {
          if (at >= slice.length) return c.close();
          c.enqueue(slice.subarray(at, at + chunk));
          at += chunk;
        },
      });
    },
  };
}

/** Every pixel of a sheet whose colour is a function of its position. */
function gradient(width: number, height: number): Uint8Array {
  const out = new Uint8Array(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const at = (y * width + x) * 4;
      out[at] = x;
      out[at + 1] = y;
      out[at + 2] = 7;
      out[at + 3] = 255;
    }
  }
  return out;
}

/** The IHDR and the pixels back out of a PNG, so the encoder is checked against a reader. */
async function readPng(png: Uint8Array): Promise<{ width: number; height: number; rgba: Uint8Array }> {
  expect([...png.subarray(0, 8)]).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const view = new DataView(png.buffer, png.byteOffset, png.byteLength);
  let at = 8;
  let width = 0;
  let height = 0;
  const idat: Uint8Array[] = [];
  while (at < png.length) {
    const length = view.getUint32(at);
    const type = String.fromCharCode(...png.subarray(at + 4, at + 8));
    const data = png.subarray(at + 8, at + 8 + length);
    if (type === "IHDR") {
      width = view.getUint32(at + 8);
      height = view.getUint32(at + 12);
      expect(data[8]).toBe(8);
      expect(data[9]).toBe(6);
    }
    if (type === "IDAT") idat.push(data);
    at += 12 + length;
  }

  const raw = await inflate(concat(idat));
  const stride = width * 4;
  const rgba = new Uint8Array(width * height * 4);
  for (let y = 0; y < height; y++) {
    // Every scanline is written unfiltered, so the filter byte is all there is to skip.
    expect(raw[y * (stride + 1)]).toBe(0);
    rgba.set(raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)), y * stride);
  }
  return { width, height, rgba };
}

function concat(parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

const px = (img: { width: number; rgba: Uint8Array }, x: number, y: number) =>
  [...img.rgba.subarray((y * img.width + x) * 4, (y * img.width + x) * 4 + 4)];

describe("walking the image table", () => {
  it("reads every sheet's name and size without inflating one", async () => {
    const file = await pnt([
      { name: "plastics", width: 4, height: 4, rgba: gradient(4, 4) },
      { name: "w_plate", width: 2, height: 2, rgba: gradient(2, 2) },
    ]);
    const images = await imageTable(sourceOf(file));
    expect(images.map((i) => i.name)).toEqual(["plastics", "w_plate"]);
    expect(images.map((i) => [i.width, i.height])).toEqual([
      [4, 4],
      [2, 2],
    ]);
    expect(images[1].end).toBe(file.length);
  });

  it("calls a file without the magic sealed, because locked content is", async () => {
    const sealed = new Uint8Array(HEADER_SIZE + 32).fill(0x5a);
    await expect(imageTable(sourceOf(sealed))).rejects.toThrow("sealed");
  });

  it("refuses a file too short to hold a header", async () => {
    await expect(imageTable(sourceOf(new Uint8Array(12)))).rejects.toThrow("unreadable");
  });

  it("stops at a truncated table rather than reading past the end", async () => {
    const file = await pnt([{ name: "plastics", width: 4, height: 4, rgba: gradient(4, 4) }]);
    const cut = file.subarray(0, file.length - 4);
    await expect(imageTable(sourceOf(cut))).rejects.toThrow("empty");
  });
});

describe("picking the sheet that stands for the paint", () => {
  const sheet = (name: string, edge: number): PntImage => ({
    name,
    width: edge,
    height: edge,
    start: 0,
    end: 1,
  });

  it("takes the largest", () => {
    expect(pickImage([sheet("numbers", 128), sheet("plastics", 512)]).name).toBe("plastics");
  });

  it("never takes the number plate, which no author paints", () => {
    expect(pickImage([sheet("w_plate", 1024), sheet("plastics", 256)]).name).toBe("plastics");
  });

  it("falls back to it when there is nothing else", () => {
    expect(pickImage([sheet("w_plate", 64)]).name).toBe("w_plate");
  });
});

describe("cutting the thumbnail", () => {
  it("flips the sheet, which is stored the way the mesh samples it", async () => {
    const file = await pnt([{ name: "plastics", width: 4, height: 4, rgba: gradient(4, 4) }]);
    const src = sourceOf(file);
    const thumb = await renderThumb(src, (await imageTable(src))[0], 4);

    expect([thumb.width, thumb.height]).toEqual([4, 4]);
    // Green carries the source row. The top row out is the bottom row in.
    expect(px(thumb, 0, 0)[1]).toBe(3);
    expect(px(thumb, 0, 3)[1]).toBe(0);
    // Red carries the column, which is not flipped.
    expect(px(thumb, 2, 0)[0]).toBe(2);
  });

  it("samples down without holding the sheet", async () => {
    const file = await pnt([{ name: "plastics", width: 64, height: 64, rgba: gradient(64, 64) }]);
    const src = sourceOf(file, 13);
    const thumb = await renderThumb(src, (await imageTable(src))[0], 8);
    expect([thumb.width, thumb.height]).toEqual([8, 8]);
    // Row 4 of 8 samples source row 36, and the flip puts it three rows from the bottom.
    expect(px(thumb, 0, 3)[1]).toBe(36);
    expect(px(thumb, 0, 0)[2]).toBe(7);
  });

  it("leaves a sheet smaller than the target alone rather than blowing it up", async () => {
    const file = await pnt([{ name: "plastics", width: 3, height: 2, rgba: gradient(3, 2) }]);
    const src = sourceOf(file);
    const thumb = await renderThumb(src, (await imageTable(src))[0], 256);
    expect([thumb.width, thumb.height]).toEqual([3, 2]);
  });

  it("refuses a payload that stops before the last row it promised", async () => {
    const file = await pnt([{ name: "plastics", width: 8, height: 8, rgba: gradient(8, 8) }]);
    const src = sourceOf(file);
    const image = (await imageTable(src))[0];
    const truncated = { ...src, stream: (o: number, l: number) => src.stream(o, Math.max(1, l - 12)) };
    await expect(renderThumb(truncated, image, 8)).rejects.toThrow("unreadable");
  });
});

describe("the PNG that comes out", () => {
  it("round-trips the pixels it was given", async () => {
    const rgba = gradient(6, 5);
    const decoded = await readPng(await encodePng(rgba, 6, 5));
    expect([decoded.width, decoded.height]).toEqual([6, 5]);
    expect([...decoded.rgba]).toEqual([...rgba]);
  });

  it("keeps alpha, so a transparent sheet is not a white one", async () => {
    const rgba = new Uint8Array([1, 2, 3, 0, 4, 5, 6, 128]);
    const decoded = await readPng(await encodePng(rgba, 2, 1));
    expect(px(decoded, 0, 0)).toEqual([1, 2, 3, 0]);
    expect(px(decoded, 1, 0)).toEqual([4, 5, 6, 128]);
  });

  it("draws a whole paint, table to picture", async () => {
    const file = await pnt([
      { name: "w_plate", width: 32, height: 32, rgba: gradient(32, 32) },
      { name: "plastics", width: 16, height: 16, rgba: gradient(16, 16) },
    ]);
    const src = sourceOf(file, 101);
    const image = pickImage(await imageTable(src));
    expect(image.name).toBe("plastics");

    const thumb = await renderThumb(src, image, 8);
    const decoded = await readPng(await encodePng(thumb.rgba, thumb.width, thumb.height));
    expect([decoded.width, decoded.height]).toEqual([8, 8]);
    expect(px(decoded, 0, 7)[1]).toBe(1);
  });
});
