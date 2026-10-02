// The PNG encoder, checked by decoding its output with zlib and undoing the filter.

import { expect, test } from "bun:test";
import { inflateSync } from "node:zlib";
import { crc32, encodePng } from "../src/png.ts";

const ascii = (text: string) => new TextEncoder().encode(text);

/** Decodes an 8-bit RGBA PNG with only "None" and "Up" filters, checking every chunk's CRC. */
function decode(png: Uint8Array): {
  width: number;
  height: number;
  rgba: Uint8Array;
  chunks: string[];
} {
  expect([...png.subarray(0, 8)]).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const view = new DataView(png.buffer, png.byteOffset, png.byteLength);
  const chunks: string[] = [];
  const idat: number[] = [];
  let width = 0;
  let height = 0;
  for (let at = 8; at < png.length; ) {
    const length = view.getUint32(at);
    const type = String.fromCharCode(...png.subarray(at + 4, at + 8));
    const data = png.subarray(at + 8, at + 8 + length);
    expect(view.getUint32(at + 8 + length)).toBe(crc32(png.subarray(at + 4, at + 8 + length)));
    chunks.push(type);
    if (type === "IHDR") {
      width = view.getUint32(at + 8);
      height = view.getUint32(at + 12);
      expect([...data.subarray(8)]).toEqual([8, 6, 0, 0, 0]);
    }
    if (type === "IDAT") {
      idat.push(...data);
    }
    at += 12 + length;
  }
  const raw = inflateSync(Uint8Array.from(idat));
  const stride = width * 4;
  const rgba = new Uint8Array(stride * height);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    expect(filter === 0 || filter === 2).toBe(true);
    for (let x = 0; x < stride; x++) {
      const above = filter === 2 && y > 0 ? (rgba[(y - 1) * stride + x] ?? 0) : 0;
      rgba[y * stride + x] = ((raw[y * (stride + 1) + 1 + x] ?? 0) + above) & 0xff;
    }
  }
  return { width, height, rgba, chunks };
}

test("crc32 matches the standard check value", () => {
  expect(crc32(ascii("123456789"))).toBe(0xcbf43926);
  expect(crc32(new Uint8Array())).toBe(0);
  expect(crc32(ascii("IEND"))).toBe(0xae426082);
});

test("pixels survive a round trip", async () => {
  for (const [width, height] of [
    [1, 1],
    [3, 2],
    [17, 9],
  ] as const) {
    const rgba = Uint8Array.from(
      { length: width * height * 4 },
      (_, i) => (i * 37 + (i >> 5)) & 0xff,
    );
    const decoded = decode(await encodePng(width, height, rgba));
    expect([decoded.width, decoded.height]).toEqual([width, height]);
    expect(decoded.rgba).toEqual(rgba);
    expect(decoded.chunks).toEqual(["IHDR", "IDAT", "IEND"]);
  }
});

test("sizes that don't match the bytes are errors", async () => {
  for (const [w, h, len] of [
    [0, 1, 0],
    [2, 2, 15],
    [1.5, 2, 12],
  ] as const) {
    let error: unknown;
    try {
      await encodePng(w, h, new Uint8Array(len));
    } catch (e) {
      error = e;
    }
    expect(error).toBeInstanceOf(RangeError);
  }
});
