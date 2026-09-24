import { z } from "zod";

const finite = z.number().finite();
const vector = z.tuple([finite, finite, finite]);
const tint = z.tuple([z.number().min(0).max(4), z.number().min(0).max(4), z.number().min(0).max(4)]);
const horizontal = z.tuple([finite, finite]);
/** A local solar clock runs independently of authored weather transitions. */
export const dayCycleSchema = z.object({
  duration: z.number().min(30).max(86400),
  startHour: z.number().min(0).max(24),
  latitude: z.number().min(-75).max(75),
  declination: z.number().min(-23.44).max(23.44).default(0),
  northOffset: z.number().min(-6.29).max(6.29).default(0),
  moonIntensity: z.number().min(0).max(1).default(0.055),
  nightExposure: z.number().min(0).max(6).default(2.5),
});
/** Bounded formations in atmosphere-local metres, advected with the weather. */
export const cloudFormationSchema = z
  .object({
    id: z.string().min(1).max(100),
    kind: z.enum(["tower", "bank", "wisp"]),
    center: horizontal,
    base: z.number().min(850).max(5000),
    size: z.tuple([
      z.number().min(200).max(12000),
      z.number().min(150).max(5500),
      z.number().min(200).max(12000),
    ]),
    yaw: z.number().min(-6.29).max(6.29),
    density: z.number().min(0).max(1),
    erosion: z.number().min(0).max(1),
    // Growing tips (0) through mature, spreading and evaporating growth (1).
    maturity: z.number().min(0).max(1).optional(),
    shear: z.number().min(-1).max(1).optional(),
    seed: z.number().int().min(0).max(65535),
  })
  .refine((f) => f.base + f.size[1] <= 6500, "Cloud formation must fit below 6500 metres");
/** Semantic weather controls; a front is an oriented, moving cover transition. */
export const cloudscapeSchema = z.object({
  development: z.number().min(0).max(1).default(0.45),
  storminess: z.number().min(0).max(1).default(0),
  highCloudCover: z.number().min(0).max(1).default(0.15),
  highCloudYaw: z.number().min(-6.29).max(6.29).optional(),
  highCloudHeight: z.number().min(8500).max(14000).optional(),
  midCloudCover: z.number().min(0).max(1).optional(),
  // Keep the thin middle deck above the bounded convective volume (6500 m).
  midCloudHeight: z.number().min(6600).max(8000).optional(),
  midCloudYaw: z.number().min(-6.29).max(6.29).optional(),
  background: z.number().min(0).max(1).optional(),
  formations: z
    .array(cloudFormationSchema)
    .max(4)
    .refine(
      (forms) => new Set(forms.map((f) => f.id)).size === forms.length,
      "Cloud formation identities must be unique",
    )
    .optional(),
  front: z
    .object({
      origin: horizontal,
      direction: horizontal.refine((v) => Math.hypot(...v) > 0.0001, "Front direction must be nonzero"),
      width: z.number().min(100).max(100000),
      strength: z.number().min(-1).max(1),
    })
    .optional(),
});
/** Scene-linear light tint and photographic exposure stops. Identity is explicit. */
export const environmentGradeSchema = z.object({
  exposureCompensation: z.number().min(-8).max(8).default(0),
  tint: tint.default([1, 1, 1]),
});
export const environmentStateSchema = z.object({
  sunElevation: z.number().min(-1.57).max(1.57),
  sunAzimuth: z.number().min(-6.29).max(6.29),
  turbidity: z.number().min(1).max(10),
  fogDensity: z.number().min(0).max(0.1),
  cloudCover: z.number().min(0).max(1).optional(),
  cloudscape: cloudscapeSchema.optional(),
  wind: vector,
  ambient: z.number().min(0).max(3),
  sunIntensity: z.number().min(0).max(30),
  wetness: z.number().min(0).max(1).default(0),
  grade: environmentGradeSchema.default({ exposureCompensation: 0, tint: [1, 1, 1] }),
});
export const environmentSequenceSchema = z
  .object({
    duration: z.number().min(0.1).max(86400),
    loop: z.boolean().default(true),
    interpolation: z.enum(["linear", "smooth"]).default("smooth"),
    keyframes: z
      .array(z.object({ time: z.number().min(0).max(86400), state: environmentStateSchema }))
      .min(2)
      .max(32),
  })
  .superRefine((sequence, context) => {
    sequence.keyframes.forEach((key, index) => {
      if (
        (sequence.loop ? key.time >= sequence.duration : key.time > sequence.duration) ||
        (index > 0 && key.time <= sequence.keyframes[index - 1].time)
      )
        context.addIssue({
          code: "custom",
          path: ["keyframes", index, "time"],
          message: "Keyframes must increase strictly; looping states must precede the sequence duration",
        });
    });
  });
export const lightingZoneSchema = z.object({
  id: z.string().min(1).max(100),
  name: z.string().min(1).max(120),
  center: vector,
  radius: z.number().min(0.1).max(10000),
  /** Optional oriented room volume; omitted zones retain their spherical support. */
  box: z
    .object({
      halfExtents: z.tuple([
        z.number().min(0.1).max(10000),
        z.number().min(0.1).max(10000),
        z.number().min(0.1).max(10000),
      ]),
      yaw: z.number().min(-6.29).max(6.29).default(0),
    })
    .optional(),
  blendDistance: z.number().min(0).max(10000),
  priority: z.number().int().min(-100).max(100).default(0),
  ambient: z.number().min(0).max(3),
  sunIntensity: z.number().min(0).max(30),
  fogMultiplier: z.number().min(0).max(10).default(1),
  grade: environmentGradeSchema.default({ exposureCompensation: 0, tint: [1, 1, 1] }),
});
const riverPointSchema = z.object({
  position: z.tuple([finite, finite]),
  level: z.number().min(-100).max(100).optional(),
  width: z.number().min(0.5).max(1000),
  depth: z.number().min(0.05).max(100),
});
export const waterWeatherResponseSchema = z.object({
  /** Authored waves/roughness are the baseline at this horizontal wind speed. */
  referenceWindSpeed: z.number().min(0).max(100).default(0),
  waveGain: z.number().min(0).max(2).default(0.12),
  roughnessGain: z.number().min(0).max(0.1).default(0.012),
});
export const waterAuthoringSchema = z.object({
  weatherResponse: waterWeatherResponseSchema.optional(),
  velocity: z.tuple([z.number().min(-20).max(20), z.number().min(-20).max(20)]).default([0, 0]),
  river: z
    .object({
      points: z.array(riverPointSchema).min(2).max(32),
      shoreWidth: z.number().min(0.01).max(50).default(1),
      foam: z.number().min(0).max(1).default(0.25),
    })
    .superRefine((river, context) => {
      river.points.forEach((point, index) => {
        if (
          index &&
          Math.hypot(
            point.position[0] - river.points[index - 1].position[0],
            point.position[1] - river.points[index - 1].position[1],
          ) < 0.01
        )
          context.addIssue({
            code: "custom",
            path: ["points", index, "position"],
            message: "Consecutive river points must be distinct",
          });
      });
    })
    .optional(),
});
export type EnvironmentGrade = z.infer<typeof environmentGradeSchema>;
export type DayCycle = z.infer<typeof dayCycleSchema>;
export type Cloudscape = z.infer<typeof cloudscapeSchema>;
export type EnvironmentState = z.infer<typeof environmentStateSchema>;
export type EnvironmentSequence = z.infer<typeof environmentSequenceSchema>;
export type LightingZone = z.infer<typeof lightingZoneSchema>;
export type WaterAuthoring = z.infer<typeof waterAuthoringSchema>;
