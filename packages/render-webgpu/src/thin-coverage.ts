import { type ThinCoverage, validThinCoverage } from "@wrela/model";

export type ThinCoverageGpu = { texture: GPUTexture; view: GPUTextureView; bytes: number; uploaded: number };
export function thinCoverageTextureBytes(coverage: ThinCoverage | undefined): number {
  return coverage?.levels.reduce((sum, level) => sum + level.byteLength, 0) ?? 0;
}

/** Upload rows, or part of a row when necessary, without exceeding this frame's byte budget. */
export function uploadThinCoverageGpu(
  device: GPUDevice,
  resource: ThinCoverageGpu,
  coverage: ThinCoverage | undefined,
  maxBytes: number,
): number {
  const levels = coverage?.levels ?? [new Uint8Array([255])];
  let width = coverage?.width ?? 1,
    base = 0;
  const stride = coverage?.format === "coverage-normal" ? 4 : 1;
  let budget = Math.max(0, Math.floor(maxBytes)),
    uploaded = 0;
  for (const [mipLevel, level] of levels.entries()) {
    let offset = Math.max(0, resource.uploaded - base);
    while (offset < level.byteLength && budget >= stride) {
      const height = Math.max(1, (coverage?.height ?? 1) >> mipLevel);
      const layerBytes = width * height * stride;
      const layer = Math.floor(offset / layerBytes);
      const localOffset = offset % layerBytes;
      const x = (localOffset / stride) % width,
        y = Math.floor(localOffset / (stride * width));
      const remaining = Math.floor(Math.min(layerBytes - localOffset, budget) / stride);
      const rows = x === 0 ? Math.floor(remaining / width) : 0;
      const copyWidth = rows > 0 ? width : Math.min(width - x, remaining);
      const copyHeight = Math.max(1, rows),
        count = copyWidth * copyHeight * stride;
      device.queue.writeTexture(
        { texture: resource.texture, mipLevel, origin: { x, y, z: layer } },
        level as Uint8Array<ArrayBuffer>,
        { offset, bytesPerRow: copyWidth * stride, rowsPerImage: copyHeight },
        { width: copyWidth, height: copyHeight },
      );
      offset += count;
      resource.uploaded += count;
      uploaded += count;
      budget -= count;
    }
    base += level.byteLength;
    width = Math.max(1, width >> 1);
    if (budget < stride) break;
  }
  return uploaded;
}

/** Texture lifetime follows object residency. Deferred textures are never drawn until all mip levels arrive. */
export function createThinCoverageGpu(
  device: GPUDevice,
  coverage?: ThinCoverage,
  options: { deferUpload?: boolean } = {},
): ThinCoverageGpu {
  if (coverage && !validThinCoverage(coverage, coverage.uv.length / 2))
    throw new Error("Invalid thin coverage field");
  const width = coverage?.width ?? 1,
    height = coverage?.height ?? 1;
  const texture = device.createTexture({
    label: `Semantic thin coverage ${coverage?.key ?? "opaque fallback"}`,
    size: { width, height, depthOrArrayLayers: coverage?.layers ?? 1 },
    format: coverage?.format === "coverage-normal" ? "rgba8unorm" : "r8unorm",
    mipLevelCount: coverage?.levels.length ?? 1,
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
  });
  const resource: ThinCoverageGpu = {
    texture,
    view: texture.createView({ dimension: "2d-array" }),
    bytes: coverage ? thinCoverageTextureBytes(coverage) : 1,
    uploaded: 0,
  };
  if (!options.deferUpload) uploadThinCoverageGpu(device, resource, coverage, resource.bytes);
  return resource;
}
