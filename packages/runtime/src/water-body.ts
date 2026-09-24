import { compileWaterDomain, compileWaterSpectrum } from "@wrela/compiler";
import type { WaterDefinition, WaterRenderState } from "@wrela/model";

import { WaterSimulation } from "./water-simulation";

export class WaterBodyRuntime {
  readonly domain;
  readonly simulation?: WaterSimulation;
  private cached?: WaterRenderState;
  constructor(readonly definition: WaterDefinition) {
    this.domain = compileWaterDomain(definition);
    if (this.domain) this.simulation = new WaterSimulation(this.domain, definition);
  }
  get byteLength() {
    const arrays = [
      this.domain?.cells,
      this.domain?.contact,
      this.domain?.tiles,
      this.domain?.surface.positions,
      this.domain?.surface.normals,
      this.domain?.surface.indices,
      this.domain?.bed.positions,
      this.domain?.bed.normals,
      this.domain?.bed.colors,
      this.domain?.bed.indices,
      this.cached?.cells,
      this.cached?.previous,
      this.cached?.spectrum.carriers,
    ];
    const unique = new Set(
      arrays.filter((array): array is Float32Array | Uint32Array => !!array).map((array) => array.buffer),
    );
    return (
      [...unique].reduce((sum, buffer) => sum + buffer.byteLength, 0) + (this.simulation?.byteLength ?? 0)
    );
  }
  renderState(water = this.definition): WaterRenderState {
    const spectrum = compileWaterSpectrum(water),
      revision = this.simulation?.revision ?? 0;
    if (this.cached?.revision === revision && this.cached.spectrum === spectrum) return this.cached;
    const cells = this.domain ? new Float32Array(this.domain.cells.length) : undefined;
    if (cells) this.simulation?.writeRender(cells);
    let minLevel = water.level,
      maxLevel = water.level;
    if (cells)
      for (let i = 0; i < cells.length; i += 4) {
        minLevel = Math.min(minLevel, cells[i]);
        maxLevel = Math.max(maxLevel, cells[i]);
      }
    this.cached = {
      domain: this.domain,
      spectrum,
      cells,
      wetness: this.simulation?.wetness,
      revision,
      minLevel,
      maxLevel,
    };
    return this.cached;
  }
}
