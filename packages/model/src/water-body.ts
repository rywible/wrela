import { z } from "zod";
import type { MeshData } from "./contracts";

const pair = z.tuple([z.number().finite(), z.number().finite()]);
/** Bounded visual residuals; the local fluid remains authoritative for gameplay. */
export const waterEffectSchema = z
  .object({
    id: z.string().min(1).max(80),
    kind: z.enum(["breaker", "waterfall"]),
    start: pair,
    end: pair,
    height: z.number().min(0.1).max(8),
    width: z.number().min(0.2).max(20),
    period: z.number().min(1).max(20),
    phase: z.number().min(0).max(1),
  })
  .refine(
    (v) => Math.hypot(v.end[0] - v.start[0], v.end[1] - v.start[1]) > 0.2,
    "Water effect front must have length",
  );
export type WaterEffect = z.infer<typeof waterEffectSchema>;
/** Metres, seconds and cubic metres/second. Derived grids are never source assets. */
export const waterDomainSchema = z.object({
  min: pair,
  size: z.tuple([z.number().min(4).max(512), z.number().min(4).max(512)]),
  resolution: z.union([z.literal(32), z.literal(64), z.literal(128)]).default(64),
  /** Visual bed resolution is independent of the conserved fluid grid. */
  renderResolution: z.union([z.literal(64), z.literal(128), z.literal(256)]).optional(),
  bedDetail: z.number().min(0).max(0.25).optional(),
  dryingSeconds: z.number().min(1).max(120).optional(),
  basins: z
    .array(
      z.object({
        center: pair,
        radii: z.tuple([z.number().min(1).max(200), z.number().min(1).max(200)]),
        depth: z.number().min(0.1).max(20),
        level: z.number().min(-100).max(100).optional(),
      }),
    )
    .max(8)
    .default([]),
  obstacles: z
    .array(
      z.object({
        center: pair,
        radius: z.number().min(0.2).max(20),
        height: z.number().min(0.1).max(20),
        aspect: z.number().min(0.35).max(3).optional(),
        yaw: z.number().min(-6.29).max(6.29).optional(),
      }),
    )
    .max(64)
    .default([]),
  sources: z
    .array(
      z.object({
        position: pair,
        radius: z.number().min(0.2).max(20),
        rate: z.number().min(-20).max(20),
        velocity: z.tuple([z.number().min(-20).max(20), z.number().min(-20).max(20)]).default([0, 0]),
      }),
    )
    .max(16)
    .default([]),
  bankHeight: z.number().min(0.1).max(10).default(1.2),
  bankWidth: z.number().min(0.2).max(20).default(2),
  simulate: z.boolean().default(true),
  friction: z.number().min(0).max(5).default(0.12),
});
export const waterSpectrumSchema = z.object({
  mode: z.enum(["artistic", "sea-state"]).optional(),
  /** Sea-state overrides; omitted values use fully developed wind-sea estimates. */
  significantHeight: z.number().min(0).max(12).optional(),
  peakPeriod: z.number().min(0.5).max(16).optional(),
  depth: z.number().min(0.2).max(1000).optional(),
  swell: z
    .object({
      height: z.number().min(0).max(8),
      period: z.number().min(1).max(16),
      direction: z.number().min(-6.29).max(6.29),
      spread: z.number().min(0.01).max(1).default(0.15),
    })
    .optional(),
  seed: z.number().int().default(17),
  windSpeed: z.number().min(0.1).max(30).default(6),
  direction: z.number().min(-6.29).max(6.29).default(0.6),
  amplitude: z.number().min(0).max(3).default(0.3),
  wavelength: z.number().min(0.2).max(120).default(12),
  spread: z.number().min(0.05).max(1.5).default(0.7),
  choppiness: z.number().min(0).max(0.8).default(0.35),
});
export const waterOpticsSchema = z.object({
  scattering: z
    .tuple([z.number().min(0).max(2), z.number().min(0).max(2), z.number().min(0).max(2)])
    .optional(),
  anisotropy: z.number().min(-0.8).max(0.9).optional(),
  foamLifetime: z.number().min(0.2).max(30).optional(),
  absorption: z
    .tuple([z.number().min(0.001).max(10), z.number().min(0.001).max(10), z.number().min(0.001).max(10)])
    .default([0.18, 0.055, 0.025]),
  caustics: z.number().min(0).max(1).default(0.35),
  foam: z.number().min(0).max(2).default(0.6),
});
export type WaterDomain = z.infer<typeof waterDomainSchema>;
export type WaterSpectrum = z.infer<typeof waterSpectrumSchema>;
export const persistentWaterStateSchema = z
  .object({
    id: z.string().min(1).max(100),
    key: z.string().min(1).max(128),
    tick: z.number().int().min(0).max(1e9),
    state: z.array(z.number().finite()).max(128 * 128 * 4),
    exchangedVolume: z.number().finite(),
    elapsed: z.number().finite().min(0).optional(),
    wetness: z
      .array(z.number().min(0).max(1))
      .max(128 * 128)
      .optional(),
  })
  .strict();
export type PersistentWaterState = z.infer<typeof persistentWaterStateSchema>;
export interface CompiledWaterDomain {
  key: string;
  min: [number, number];
  size: [number, number];
  resolution: number;
  spacing: [number, number];
  /** Interleaved bed elevation, initial surface level, initial velocity X/Z. */
  cells: Float32Array;
  /** Static fine bed: elevation, initial level, shore distance, energy-source mask. */
  contact: Float32Array;
  renderResolution: number;
  /** Conservative cell extrema for eight-by-eight fluid tiles. */
  tiles: Float32Array;
  surface: MeshData;
  bed: MeshData;
}
export interface CompiledWaterSpectrum {
  key: string;
  /** Three periodic shading cascades, eighteen generated carriers each, following any authored carriers. */
  tiles?: [number, number, number];
  /** Two vec4 per carrier: kx,kz,omega,phase; amplitude,wavelength,dirX,dirZ. */
  carriers: Float32Array;
  amplitudeBound: number;
  /** Horizontal displacement is bounded to keep the parametric surface invertible. */
  choppiness: number;
  slopeVariance: number;
  slopeCovariance: [number, number, number];
  /** Ensemble slope variance of each generated shading cascade; excludes authored waves/swell. */
  bandSlopeVariance: [number, number, number];
  /** Source-derived realization facts, independently inspectable from the renderer. */
  realization: {
    method: "carriers";
    mapSize: 128 | 256;
    layers: 3 | 6;
    maxFrequency: number;
    reason: string;
  };
}
export interface WaterRenderState {
  domain?: CompiledWaterDomain;
  spectrum: CompiledWaterSpectrum;
  /** Surface elevation, velocity X/Z, foam concentration at each grid node. */
  cells?: Float32Array;
  previous?: Float32Array;
  wetness?: Float32Array;
  revision: number;
  minLevel: number;
  maxLevel: number;
}
