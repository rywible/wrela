import { expect, test } from "bun:test";
import { quad } from "../../../tools/fixtures/indirect-scenes";
import { compactRadianceReceiverTable } from "./radiance-receiver-table";

test("receiver interning preserves complete float32 payloads, sides and enclosure certificates", () => {
  const a = [1, 2, 0, 0, 0.4, 0.6, 0, 0],
    b = [2, 3, 0, 0, 0.4, 0.6, 0, 0];
  const records = new Float32Array([...a, ...a, ...b, ...a]);
  const emission = new Float32Array([0, 0, 0, 0, 0, 0, 0, 0, 0, 0.01, 0, 0]);
  const mesh = {
    ...quad(
      "surface",
      [
        [0, 0, 0],
        [1, 0, 0],
        [1, 1, 0],
        [0, 1, 0],
      ],
      [1, 1, 1],
    ).mesh,
    radianceProbes: new Float32Array([1024.5, 0, 1025, 0, 1026, 0, 1027.5, 0]),
  };
  const before = mesh.radianceProbes.slice();
  const steps = compactRadianceReceiverTable(records, emission, new Map([["surface", mesh]]));
  let next = steps.next();
  while (!next.done) next = steps.next();
  const result = next.value;
  expect(result.removed).toBe(1);
  for (let at = 0; at < before.length; at += 2) {
    const old = Math.floor(before[at]) - 1024,
      now = Math.floor(mesh.radianceProbes[at]) - 1024;
    expect(result.receivers.slice(now * 8, now * 8 + 8)).toEqual(records.slice(old * 8, old * 8 + 8));
    expect(result.emission?.slice(now * 3, now * 3 + 3)).toEqual(emission.slice(old * 3, old * 3 + 3));
    expect(mesh.radianceProbes[at] % 1).toBe(before[at] % 1);
  }
  expect(result.receivers.buffer.byteLength).toBe(result.receivers.byteLength);
  expect(result.emission?.buffer.byteLength).toBe(result.emission?.byteLength);
});
