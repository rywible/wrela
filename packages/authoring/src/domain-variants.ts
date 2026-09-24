import { contentKey, idSchema } from "@wrela/model";

import { z } from "zod";
import type { AuthoringCandidate } from "./creature-candidates";
import {
  type SourceConstraintCheck,
  type SourceValueChange,
  sourcePreservationSchema,
} from "./domain-constraints";
import { type DomainRecipePlan, domainRecipeSchema } from "./domain-recipes";

export const domainVariantRequestSchema = z
  .strictObject({
    id: idSchema,
    expectedRevision: z.number().int().nonnegative(),
    actor: z.string().min(1).max(120).optional(),
    intent: z.string().min(1).max(1000).optional(),
    variants: z
      .array(
        z.strictObject({
          id: idSchema,
          recipe: domainRecipeSchema,
          preserve: z.array(sourcePreservationSchema).max(64).default([]),
        }),
      )
      .min(1)
      .max(8),
  })
  .superRefine((request, context) => {
    if (new Set(request.variants.map((variant) => variant.id)).size !== request.variants.length)
      context.addIssue({ code: "custom", path: ["variants"], message: "Variant IDs must be unique" });
  });
export type DomainVariantRequest = z.input<typeof domainVariantRequestSchema>;

/** Readable, bounded identity; hashing the pair avoids delimiter and truncation collisions. */
export function domainVariantCandidateId(requestId: string, variantId: string): string {
  const request = idSchema.parse(requestId),
    variant = idSchema.parse(variantId);
  return `variant-${request.slice(0, 32)}-${variant.slice(0, 32)}-${contentKey([request, variant])}`;
}
export type DomainVariantReview = {
  id: string;
  candidate: AuthoringCandidate;
  domain: DomainRecipePlan["domain"];
  constraints: SourceConstraintCheck[];
  sourceDiff: { changes: SourceValueChange[]; truncated: boolean };
  sourceCost: { operations: number; changedDocuments: number; beforeBytes: number; afterBytes: number };
  review: DomainRecipePlan["review"] & {
    status: "unreviewed";
    baselineKey: string;
    candidateKey: string;
    pendingMeasurements: string[];
  };
};
export type DomainVariantResult = {
  id: string;
  sourceRevision: number;
  sourceKey: string;
  sourcePreparationMs: number;
  variants: DomainVariantReview[];
};
