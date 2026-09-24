import type { RenderSurface, Vec3 } from "@wrela/model";

export const WATER_BODY_HEADER_FLOATS = 64;
export function waterBodyDynamicFloats(surface: RenderSurface) {
  const state = surface.waterState ?? surface.waterContact?.state;
  return state
    ? WATER_BODY_HEADER_FLOATS +
        state.spectrum.carriers.length +
        (state.cells?.length ?? 0) +
        (surface.waterReflections?.length ?? 0)
    : 4;
}
export function waterBodyFloats(surface: RenderSurface) {
  return (
    waterBodyDynamicFloats(surface) +
    ((surface.waterState ?? surface.waterContact?.state)?.domain?.contact.length ?? 0) +
    ((surface.water ?? surface.waterContact?.water)?.effects?.length ?? 0) * 12
  );
}
/** A contiguous dynamic prefix and immutable contact suffix; never re-upload the bed on a fluid tick. */
export function packWaterBody(
  surface: RenderSurface,
  origin: Vec3 = [0, 0, 0],
  opaqueScene = true,
  dynamicOnly = false,
): Float32Array {
  const state = surface.waterState ?? surface.waterContact?.state,
    water = surface.water ?? surface.waterContact?.water;
  if (!state || !water) return new Float32Array(4);
  const spectrum = state.spectrum,
    domain = state.domain;
  const waves = spectrum.carriers.length / 8,
    offset = 16 + waves * 2;
  const output = new Float32Array(dynamicOnly ? waterBodyDynamicFloats(surface) : waterBodyFloats(surface));
  output.set(
    domain ? [domain.min[0] - origin[0], domain.min[1] - origin[2], ...domain.spacing] : [0, 0, 1, 1],
    0,
  );
  output.set([domain?.resolution ?? 0, offset, waves, 16], 4);
  output.set(
    [
      water.level - origin[1],
      domain ? 0 : spectrum.choppiness,
      spectrum.slopeVariance,
      water.optics?.foam ?? 0.6,
    ],
    8,
  );
  output.set([...(water.optics?.absorption ?? [0.18, 0.055, 0.025]), water.optics?.caustics ?? 0.35], 12);
  output.set([origin[0] % 1024, origin[2] % 1024, domain ? 1 : 0, opaqueScene ? 1 : 0], 16);
  if (spectrum.tiles) output.set([...spectrum.tiles, 1], 20);
  output.set([...(water.optics?.scattering ?? [0.012, 0.025, 0.03]), water.optics?.anisotropy ?? 0.35], 24);
  output.set(
    [
      domain?.renderResolution ?? 0,
      waterBodyDynamicFloats(surface) / 4,
      domain ? domain.size[0] / (domain.renderResolution - 1) : 1,
      domain ? domain.size[1] / (domain.renderResolution - 1) : 1,
    ],
    28,
  );
  output.set([...(water.flow?.velocity ?? [0, 0]), water.optics?.foamLifetime ?? 4, 0], 32);
  output.set([...spectrum.slopeCovariance, spectrum.realization.mapSize], 36);
  output.set([spectrum.realization.layers, ...spectrum.bandSlopeVariance], 40);
  const current = water.flow?.velocity ?? [0, 0];
  const currentLength = Math.hypot(...current);
  const direction = water.spectrum?.direction ?? water.waves[0]?.direction ?? 0;
  output.set(
    currentLength > 0.05
      ? [current[0] / currentLength, current[1] / currentLength]
      : [Math.cos(direction), Math.sin(direction)],
    48,
  );
  const effectsOffset = waterBodyDynamicFloats(surface) + (domain?.contact.length ?? 0);
  output.set([effectsOffset / 4, water.effects?.length ?? 0, 0, 0], 52);
  if (!dynamicOnly)
    for (const [i, effect] of (water.effects ?? []).entries()) {
      output.set(
        [
          effect.start[0] - origin[0],
          effect.start[1] - origin[2],
          effect.end[0] - origin[0],
          effect.end[1] - origin[2],
          effect.height,
          effect.width,
          effect.period,
          effect.phase,
          effect.kind === "breaker" ? 1 : 2,
          0,
          0,
          0,
        ],
        effectsOffset + i * 12,
      );
    }
  const reflectionOffset = offset * 4 + (state.cells?.length ?? 0);
  output.set([reflectionOffset / 4, (surface.waterReflections?.length ?? 0) / 12, 0, 0], 44);
  if (surface.waterReflections) output.set(surface.waterReflections, reflectionOffset);
  output.set(spectrum.carriers, WATER_BODY_HEADER_FLOATS);
  for (let wave = 0; wave < waves; wave++) {
    const at = WATER_BODY_HEADER_FLOATS + wave * 8;
    output[at + 3] =
      (spectrum.carriers[wave * 8 + 3] +
        origin[0] * spectrum.carriers[wave * 8] +
        origin[2] * spectrum.carriers[wave * 8 + 1]) %
      (2 * Math.PI);
  }
  if (state.cells) {
    output.set(state.cells, offset * 4);
    for (let i = 0; i < state.cells.length; i += 4) {
      output[offset * 4 + i] -= origin[1];
      output[offset * 4 + i + 3] =
        Math.min(0.99999, state.cells[i + 3]) + 2 * Math.round((state.wetness?.[i / 4] ?? 0) * 255);
    }
  }
  if (domain && !dynamicOnly) {
    const start = waterBodyDynamicFloats(surface);
    output.set(domain.contact, start);
    for (let i = 0; i < domain.contact.length; i += 4) {
      output[start + i] -= origin[1];
      output[start + i + 1] -= origin[1];
    }
  }
  return output;
}

/** Bed, base surface and local effects retain one body upload, with explicit reference ownership. */
export class WaterBodyBuffers {
  private entries = new Map<
    string,
    {
      buffer: GPUBuffer;
      refs: number;
      state?: RenderSurface["waterState"];
      signature?: string;
      staticKey?: string;
    }
  >();
  constructor(readonly device: GPUDevice) {}
  get bytes() {
    return [...this.entries.values()].reduce((n, e) => n + e.buffer.size, 0);
  }
  acquire(surface: RenderSurface) {
    const water = surface.water ?? surface.waterContact?.water;
    const size = waterBodyFloats(surface) * 4,
      key = `${water?.id ?? "empty"}:${size}`;
    let entry = this.entries.get(key);
    if (!entry) {
      entry = {
        buffer: this.device.createBuffer({
          label: `Water ${water?.id ?? "empty"}`,
          size,
          usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST | GPUBufferUsage.COPY_SRC,
        }),
        refs: 0,
      };
      this.entries.set(key, entry);
    }
    entry.refs++;
    return entry.buffer;
  }
  release(buffer: GPUBuffer) {
    for (const [key, e] of this.entries)
      if (e.buffer === buffer) {
        if (--e.refs === 0) {
          e.buffer.destroy();
          this.entries.delete(key);
        }
        return;
      }
  }
  upload(buffer: GPUBuffer, surface: RenderSurface, origin: Vec3 | undefined, opaque: boolean) {
    const entry = [...this.entries.values()].find((e) => e.buffer === buffer);
    if (!entry) throw Error("Unowned water buffer");
    const water = surface.water ?? surface.waterContact?.water,
      state = surface.waterState ?? surface.waterContact?.state;
    if (!water || !state) return 0;
    const staticKey = JSON.stringify([origin, state.domain?.key, water.effects]);
    const signature = JSON.stringify([
      staticKey,
      water.level,
      water.optics,
      water.flow?.velocity,
      opaque,
      surface.waterReflections?.join(","),
    ]);
    if (entry.state === state && entry.signature === signature) return 0;
    const data = packWaterBody(surface, origin, opaque, entry.staticKey === staticKey);
    this.device.queue.writeBuffer(buffer, 0, data as Float32Array<ArrayBuffer>);
    entry.state = state;
    entry.signature = signature;
    entry.staticKey = staticKey;
    return data.byteLength;
  }
  destroy() {
    for (const entry of this.entries.values()) entry.buffer.destroy();
    this.entries.clear();
  }
}

/** Synthesis consumes only source coefficients, never simulation cells or scene proxies. */
export function packWaterSpectrumSource(surface: RenderSurface, origin?: Vec3) {
  const state = surface.waterState;
  if (!state) return new Float32Array(4);
  return packWaterBody(
    {
      ...surface,
      waterState: { ...state, domain: undefined, cells: undefined, wetness: undefined },
      waterReflections: undefined,
      water: { ...surface.water!, effects: undefined },
    },
    origin,
    false,
  );
}
