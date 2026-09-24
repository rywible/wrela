import type { MeshData } from "@wrela/model";

/** Intern complete GPU receiver records by their float32 bit patterns. Position
 * and visibility have already been evaluated; only identical answers alias.
 * Emitted irradiance participates in identity, so nearby shadows cannot merge. */
export function* compactRadianceReceiverTable(
  receivers: Float32Array,
  emission: Float32Array | undefined,
  meshes: ReadonlyMap<string, MeshData>,
): Generator<void, { receivers: Float32Array; emission?: Float32Array; removed: number }> {
  const count = receivers.length / 8;
  if (!Number.isInteger(count) || (emission && emission.length !== count * 3))
    throw Error("Invalid receiver table layout");
  const bits = new Uint32Array(receivers.buffer, receivers.byteOffset, receivers.length);
  const emitted = emission && new Uint32Array(emission.buffer, emission.byteOffset, emission.length);
  const heads = new Map<number, number>();
  const next = new Int32Array(count).fill(-1),
    remap = new Uint32Array(count);
  const originals: number[] = [];
  for (let i = 0; i < count; i++) {
    let hash = 2166136261;
    for (let j = 0; j < 8; j++) hash = Math.imul(hash ^ bits[i * 8 + j], 16777619);
    if (emitted) for (let j = 0; j < 3; j++) hash = Math.imul(hash ^ emitted[i * 3 + j], 16777619);
    const head = heads.get(hash) ?? -1;
    let found = -1;
    for (let candidate = head; candidate >= 0; candidate = next[candidate]) {
      let same = true;
      for (let j = 0; j < 8; j++) if (bits[i * 8 + j] !== bits[candidate * 8 + j]) same = false;
      if (emitted)
        for (let j = 0; j < 3; j++) if (emitted[i * 3 + j] !== emitted[candidate * 3 + j]) same = false;
      if (same) {
        found = candidate;
        break;
      }
    }
    if (found >= 0) remap[i] = remap[found];
    else {
      remap[i] = originals.length;
      originals.push(i);
      next[i] = head;
      heads.set(hash, i);
    }
    if (i % 512 === 511) yield;
  }
  const removed = count - originals.length;
  if (!removed) return { receivers, emission, removed };
  const compact = new Float32Array(originals.length * 8);
  const compactEmission = emission && new Float32Array(originals.length * 3);
  for (let i = 0; i < originals.length; i++) {
    const from = originals[i];
    compact.set(receivers.subarray(from * 8, from * 8 + 8), i * 8);
    if (emission) compactEmission?.set(emission.subarray(from * 3, from * 3 + 3), i * 3);
    if (i % 512 === 511) yield;
  }
  for (const mesh of meshes.values()) {
    const ids = mesh.radianceProbes;
    if (!ids) continue;
    for (let i = 0; i < ids.length; i += 2) {
      const id = Math.floor(ids[i]);
      if (id >= 1024) {
        if (id - 1024 >= count) throw Error("Receiver table reference out of range");
        ids[i] = 1024 + remap[id - 1024] + (ids[i] - id);
      }
      if (i % 2048 === 2046) yield;
    }
  }
  return { receivers: compact, emission: compactEmission, removed };
}
