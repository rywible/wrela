/** Compiler-derived coverage of semantic thin geometry. Levels are linear R8 area coverage. */
export type ThinCoverage = {
  version: 1;
  /** RGBA stores coverage plus view-frame normal, with a guarded atlas footprint. */
  format?: "coverage-normal";
  /** Texture content identity; UV coordinates belong to the individual mesh. */
  key: string;
  /** Independent mip chains prevent minified modules from leaking into neighbors. */
  layers?: number;
  layer?: Uint16Array;
  width: number;
  height: number;
  levels: Uint8Array[];
  uv: Float32Array;
};

export function thinCoverageBytes(coverage: ThinCoverage | undefined): number {
  return coverage
    ? coverage.uv.byteLength +
        (coverage.layer?.byteLength ?? 0) +
        coverage.levels.reduce((sum, level) => sum + level.byteLength, 0)
    : 0;
}

export function validThinCoverage(coverage: ThinCoverage, vertices: number): boolean {
  const layers = coverage.layers ?? 1;
  if (
    coverage.levels.reduce((sum, level) => sum + level.byteLength, 0) > 16 * 1024 * 1024 ||
    coverage.version !== 1 ||
    (coverage.format !== undefined && coverage.format !== "coverage-normal") ||
    !coverage.key ||
    !Number.isInteger(layers) ||
    layers < 1 ||
    layers > 256 ||
    (layers > 1 && (!coverage.layer || coverage.layer.length !== vertices)) ||
    (coverage.layer !== undefined &&
      (coverage.layer.length !== vertices || coverage.layer.some((value) => value >= layers))) ||
    coverage.width < 1 ||
    coverage.height < 1 ||
    coverage.width > 512 ||
    coverage.height > 512 ||
    !Number.isInteger(Math.log2(coverage.width)) ||
    !Number.isInteger(Math.log2(coverage.height)) ||
    coverage.uv.length !== vertices * 2 ||
    !coverage.uv.every(Number.isFinite)
  )
    return false;
  let width = coverage.width,
    height = coverage.height,
    index = 0;
  while (true) {
    if (
      coverage.levels[index]?.length !==
      width * height * layers * (coverage.format === "coverage-normal" ? 4 : 1)
    )
      return false;
    index++;
    if (width === 1 && height === 1) break;
    width = Math.max(1, width >> 1);
    height = Math.max(1, height >> 1);
  }
  return coverage.levels.length === index;
}
