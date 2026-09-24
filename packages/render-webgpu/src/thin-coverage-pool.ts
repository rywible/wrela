import type { ThinCoverage } from "@wrela/model";

import { createThinCoverageGpu, type ThinCoverageGpu } from "./thin-coverage";

/** Content-addressed residency. A pending upload is shared too; every consumer
 * waits for the same complete chain. The owner accounts bytes once, not per draw. */
export class ThinCoveragePool {
  private entries = new Map<string, { resource: ThinCoverageGpu; references: number; signature: string }>();
  bytes = 0;
  get size() {
    return this.entries.size;
  }
  private signature(source: ThinCoverage) {
    return `${source.format ?? "r8"}:${source.width}:${source.height}:${source.layers ?? 1}:${source.levels.map((v) => v.byteLength).join(",")}`;
  }
  retain(source: ThinCoverage): ThinCoverageGpu | undefined {
    const entry = this.entries.get(source.key);
    if (!entry) return;
    if (entry.signature !== this.signature(source))
      throw Error("Coverage content identity has incompatible storage");
    entry.references++;
    return entry.resource;
  }
  create(device: GPUDevice, source: ThinCoverage): ThinCoverageGpu {
    const existing = this.retain(source);
    if (existing) return existing;
    const resource = createThinCoverageGpu(device, source, { deferUpload: true });
    this.entries.set(source.key, { resource, references: 1, signature: this.signature(source) });
    this.bytes += resource.bytes;
    return resource;
  }
  release(key: string) {
    const entry = this.entries.get(key);
    if (!entry) throw Error("Releasing an unowned coverage product");
    if (--entry.references === 0) {
      entry.resource.texture.destroy();
      this.bytes -= entry.resource.bytes;
      this.entries.delete(key);
    }
  }
}
