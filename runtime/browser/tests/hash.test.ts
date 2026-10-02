// The state hash: wrela-abi's FNV-1a 64 vectors (runtime/abi/src/hash.rs), and test mode's times.

import { expect, test } from "bun:test";
import { testMode } from "../src/abi.ts";
import { StateHash } from "../src/hash.ts";
import { contract } from "./helpers.ts";

const hash = (bytes: Uint8Array) => {
  const h = new StateHash();
  h.update(bytes);
  return h;
};
const ascii = (text: string) => new TextEncoder().encode(text);

test("the published FNV-1a 64 test vectors, and a version-0 header", () => {
  expect(hash(ascii("")).value()).toBe(0xcbf2_9ce4_8422_2325n);
  expect(hash(ascii("a")).value()).toBe(0xaf63_dc4c_8601_ec8cn);
  expect(hash(ascii("foobar")).value()).toBe(0x8594_4171_f739_67e8n);
  expect(hash(ascii("WR\0\0")).hex()).toBe("f9ff4f026da78e4c");
});

test("the split arithmetic agrees with BigInt arithmetic on every byte value", () => {
  // FNV-1a over 0..=255, repeated with a running state, computed the slow, obvious way.
  const bytes = Uint8Array.from({ length: 4096 }, (_, i) => (i * 167 + (i >> 8)) & 0xff);
  let reference = 0xcbf2_9ce4_8422_2325n;
  for (const byte of bytes) {
    reference = ((reference ^ BigInt(byte)) * 0x100_0000_01b3n) & 0xffff_ffff_ffff_ffffn;
  }
  expect(hash(bytes).value()).toBe(reference);
});

test("pieces hash like the whole", () => {
  const bytes = Uint8Array.from({ length: 256 }, (_, i) => i);
  const whole = hash(bytes).hex();
  for (const split of [0, 1, 7, 128, 255, 256]) {
    const h = new StateHash();
    h.update(bytes.subarray(0, split));
    h.update(bytes.subarray(split));
    expect(h.hex()).toBe(whole);
  }
});

test("hex keeps leading zeros", () => {
  // Search for a short input whose hash has a leading zero nibble, then check its spelling.
  for (let i = 0; i < 1000; i++) {
    const h = hash(ascii(String(i)));
    expect(h.hex()).toBe(h.value().toString(16).padStart(16, "0"));
    expect(h.toString()).toBe(h.hex());
  }
});

test("test-mode times round once from f64", () => {
  expect(testMode.time(0)).toBe(0);
  expect(testMode.time(30)).toBe(0.5);
  expect(testMode.time(60)).toBe(1);
  const view = new DataView(new ArrayBuffer(4));
  view.setFloat32(0, testMode.time(59));
  expect(view.getUint32(0)).toBe(0x3f7b_bbbc);
  expect([testMode.WIDTH, testMode.HEIGHT, testMode.FRAMES]).toEqual([1920, 1080, 60]);
});

test("the contract states this version", async () => {
  expect((await contract()).startsWith("# wrela runtime contract, version 0\n")).toBe(true);
});
