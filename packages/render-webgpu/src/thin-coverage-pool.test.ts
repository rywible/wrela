import { expect, test } from "bun:test";
import type { ThinCoverage } from "@wrela/model";

import { ThinCoveragePool } from "./thin-coverage-pool";

test("coverage residency shares pending uploads and frees storage only after the last consumer", () => {
  Object.assign(globalThis, { GPUTextureUsage: { TEXTURE_BINDING: 1, COPY_DST: 2 } });
  let allocated = 0,
    destroyed = 0;
  const device = {
    createTexture: () => {
      allocated++;
      return { createView: () => ({}), destroy: () => destroyed++ };
    },
  } as unknown as GPUDevice;
  const source: ThinCoverage = {
    version: 1,
    key: "a",
    width: 1,
    height: 1,
    uv: new Float32Array([0, 0]),
    levels: [new Uint8Array([100])],
  };
  const pool = new ThinCoveragePool(),
    first = pool.create(device, source);
  expect(first.uploaded).toBe(0);
  expect(pool.create(device, structuredClone(source))).toBe(first);
  expect(allocated).toBe(1);
  expect(pool.bytes).toBe(1);
  first.uploaded = 1;
  pool.release("a");
  expect(destroyed).toBe(0);
  expect(pool.retain(source)?.uploaded).toBe(1);
  expect(() => pool.retain({ ...source, format: "coverage-normal" })).toThrow();
  pool.release("a");
  pool.release("a");
  expect(destroyed).toBe(1);
  expect(pool.bytes).toBe(0);
  expect(pool.size).toBe(0);
  expect(() => pool.release("a")).toThrow();
});
