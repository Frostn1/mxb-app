/**
 * SHA-256 whose state can be saved between requests.
 *
 * The fetcher uploads a file as R2 multipart parts, one request each (`mirrorfetcher.ts`), and
 * the control plane checks the whole file's SHA-256 as the parts pass. WebCrypto can only
 * hash a buffer at a time, and `crypto.DigestStream` can't outlive the request, so this keeps
 * the eight words, the unprocessed tail and the length, as JSON in D1. Parts arrive in order.
 */

const K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01,
  0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
  0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
  0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08,
  0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
  0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);

const INIT = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];

/** What is saved: the chaining value, the bytes short of a block (hex), and the length so far. */
export interface Sha256State {
  h: number[];
  tail: string;
  length: number;
}

export function sha256Init(): Sha256State {
  return { h: [...INIT], tail: "", length: 0 };
}

function fromHex(s: string): Uint8Array {
  const out = new Uint8Array(s.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(s.slice(i * 2, i * 2 + 2), 16);
  return out;
}

function toHex(b: Uint8Array): string {
  let s = "";
  for (const x of b) s += x.toString(16).padStart(2, "0");
  return s;
}

const W = new Uint32Array(64);

function blocks(h: Uint32Array, data: Uint8Array, end: number): void {
  for (let off = 0; off + 64 <= end; off += 64) {
    for (let i = 0; i < 16; i++) {
      const j = off + i * 4;
      W[i] = (data[j] << 24) | (data[j + 1] << 16) | (data[j + 2] << 8) | data[j + 3];
    }
    for (let i = 16; i < 64; i++) {
      const a = W[i - 15];
      const b = W[i - 2];
      const s0 = ((a >>> 7) | (a << 25)) ^ ((a >>> 18) | (a << 14)) ^ (a >>> 3);
      const s1 = ((b >>> 17) | (b << 15)) ^ ((b >>> 19) | (b << 13)) ^ (b >>> 10);
      W[i] = (W[i - 16] + s0 + W[i - 7] + s1) | 0;
    }
    let a = h[0], b = h[1], c = h[2], d = h[3], e = h[4], f = h[5], g = h[6], k = h[7];
    for (let i = 0; i < 64; i++) {
      const S1 = ((e >>> 6) | (e << 26)) ^ ((e >>> 11) | (e << 21)) ^ ((e >>> 25) | (e << 7));
      const ch = (e & f) ^ (~e & g);
      const t1 = (k + S1 + ch + K[i] + W[i]) | 0;
      const S0 = ((a >>> 2) | (a << 30)) ^ ((a >>> 13) | (a << 19)) ^ ((a >>> 22) | (a << 10));
      const maj = (a & b) ^ (a & c) ^ (b & c);
      const t2 = (S0 + maj) | 0;
      k = g;
      g = f;
      f = e;
      e = (d + t1) | 0;
      d = c;
      c = b;
      b = a;
      a = (t1 + t2) | 0;
    }
    h[0] += a;
    h[1] += b;
    h[2] += c;
    h[3] += d;
    h[4] += e;
    h[5] += f;
    h[6] += g;
    h[7] += k;
  }
}

/** Feed `chunk`; returns the new state. */
export function sha256Update(st: Sha256State, chunk: Uint8Array): Sha256State {
  const h = Uint32Array.from(st.h);
  const tail = fromHex(st.tail);
  let data: Uint8Array;
  if (tail.length) {
    data = new Uint8Array(tail.length + chunk.length);
    data.set(tail);
    data.set(chunk, tail.length);
  } else {
    data = chunk;
  }
  const whole = data.length - (data.length % 64);
  blocks(h, data, whole);
  return { h: Array.from(h), tail: toHex(data.subarray(whole)), length: st.length + chunk.length };
}

export function sha256Digest(st: Sha256State): string {
  const tail = fromHex(st.tail);
  const padLen = tail.length < 56 ? 64 : 128;
  const pad = new Uint8Array(padLen);
  pad.set(tail);
  pad[tail.length] = 0x80;
  const bits = st.length * 8;
  const view = new DataView(pad.buffer);
  view.setUint32(padLen - 8, Math.floor(bits / 2 ** 32));
  view.setUint32(padLen - 4, bits >>> 0);
  const h = Uint32Array.from(st.h);
  blocks(h, pad, padLen);
  const out = new Uint8Array(32);
  const ov = new DataView(out.buffer);
  for (let i = 0; i < 8; i++) ov.setUint32(i * 4, h[i]);
  return toHex(out);
}
