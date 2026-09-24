import { idSchema } from "@wrela/model";

import { z } from "zod";
import { batchSchema, type EditBatch } from "./commands";
import type { ChangeSummary, DocumentChange, TransactionResult } from "./session";

export const candidateRequestSchema = z
  .object({
    id: idSchema,
    batch: batchSchema,
    budget: z
      .object({
        maxOperations: z.number().int().min(1).max(256).default(256),
        maxSourceBytes: z
          .number()
          .int()
          .min(1)
          .max(16 * 1024 * 1024)
          .default(4 * 1024 * 1024),
      })
      .strict()
      .optional(),
  })
  .strict();
export type CandidateRequest = z.input<typeof candidateRequestSchema>;
/** Candidates retain ordinary edits, including their original dependency preconditions. */
export type AuthoringCandidate = {
  id: string;
  status: "proposed" | "adopted" | "cancelled";
  sourceRevision: number;
  sourceKey: string;
  fingerprint: string;
  batch: EditBatch;
  changes: DocumentChange[];
  diff: ChangeSummary[];
  result?: TransactionResult;
};
export const candidateAdoptOptionsSchema = z
  .object({
    transactionId: idSchema.optional(),
    actor: z.string().min(1).max(120).optional(),
    intent: z.string().min(1).max(1000).optional(),
  })
  .strict();
export type CandidateAdoptOptions = z.infer<typeof candidateAdoptOptionsSchema>;
