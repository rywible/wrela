/** Explicit cost/quality choices. Authoring quality and render quality remain independent. */
export const QUALITY_PROFILES = {
  low: {
    resolutionScale: 0.75,
    maxPixelRatio: 1,
    samples: 1,
    shadowSize: 1024,
    shadowDistance: 80,
    maxPointLights: 4,
    detailPixelScale: 1.5,
    maxGpuBytes: 128 * 1024 * 1024,
    maxUploadBytesPerFrame: 4 * 1024 * 1024,
  },
  balanced: {
    resolutionScale: 1,
    maxPixelRatio: 1.5,
    samples: 4,
    shadowSize: 2048,
    shadowDistance: 120,
    maxPointLights: 8,
    detailPixelScale: 1,
    maxGpuBytes: 256 * 1024 * 1024,
    maxUploadBytesPerFrame: 8 * 1024 * 1024,
  },
  high: {
    resolutionScale: 1,
    maxPixelRatio: 2,
    samples: 4,
    shadowSize: 4096,
    shadowDistance: 180,
    maxPointLights: 8,
    detailPixelScale: 0.75,
    maxGpuBytes: 384 * 1024 * 1024,
    maxUploadBytesPerFrame: 12 * 1024 * 1024,
  },
} as const;
export type RenderQuality = keyof typeof QUALITY_PROFILES;
