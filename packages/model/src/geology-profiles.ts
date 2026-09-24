import { z } from "zod";

const variation = z
  .object({
    seed: z.number().int().min(0).max(65_535),
    amplitude: z.number().finite().min(0).max(0.25),
    wavelength: z.number().finite().min(2).max(500),
  })
  .optional();
/** Cross sections are fractions of the landform's bounded half-width. Variation
 * only narrows that support, preserving spatial pruning and patch continuity. */
export const geologicalProfileSchema = z.discriminatedUnion("kind", [
  z
    .object({
      kind: z.literal("river"),
      bedFraction: z.number().finite().min(0.05).max(0.6),
      bankFraction: z.number().finite().min(0.1).max(0.7),
      shoulderDepth: z.number().finite().min(0).max(0.6),
      asymmetry: z.number().finite().min(-0.6).max(0.6),
      variation,
    })
    .refine((profile) => profile.bedFraction + profile.bankFraction <= 0.95, {
      message: "Bed and bank must leave room for a floodplain shoulder",
    }),
  z.object({
    kind: z.literal("cliff"),
    faceFraction: z.number().finite().min(0.03).max(0.4),
    toeHeight: z.number().finite().min(0).max(0.6),
    crestFraction: z.number().finite().min(0.45).max(0.9),
    variation,
  }),
]);
export type GeologicalProfile = z.infer<typeof geologicalProfileSchema>;
