import { canonical, type Document, idSchema, type Project } from "@wrela/model";

import { z } from "zod";

const segment = z.union([z.string().min(1).max(120), z.number().int().min(0).max(100_000)]);
export const sourcePreservationSchema = z.strictObject({
  target: idSchema,
  path: z.array(segment).max(16),
  reason: z.string().min(1).max(400).optional(),
});
export type SourcePreservation = z.infer<typeof sourcePreservationSchema>;
export type SourceConstraintCheck = SourcePreservation & {
  passed: boolean;
  scope: "authored-source";
};

function property(document: Document | undefined, path: (string | number)[]): unknown {
  if (!document) throw Error("A protected document is missing");
  let cursor: unknown = document;
  for (const key of path) {
    if (["__proto__", "prototype", "constructor"].includes(String(key)))
      throw Error("Unsafe protected property path");
    if (!cursor || typeof cursor !== "object" || !Object.hasOwn(cursor, key))
      throw Error(`Protected property ${path.join("/")} does not exist`);
    cursor = (cursor as Record<string | number, unknown>)[key];
  }
  return cursor;
}

/** Equality constrains source, not rendered clearance, silhouette or appearance. */
export function reviewSourcePreservation(
  before: Project,
  after: Project,
  constraints: SourcePreservation[],
): SourceConstraintCheck[] {
  return constraints.map((input) => {
    const constraint = sourcePreservationSchema.parse(input);
    const a = property(
      before.documents.find((document) => document.id === constraint.target),
      constraint.path,
    );
    const b = property(
      after.documents.find((document) => document.id === constraint.target),
      constraint.path,
    );
    return { ...constraint, passed: canonical(a) === canonical(b), scope: "authored-source" };
  });
}

export type SourceValueChange = {
  target: string;
  path: (string | number)[];
  before: unknown;
  after: unknown;
};
/** Bounded leaf-level changes remain useful when a transaction replaces a whole domain object. */
export function sourceValueChanges(before: Document, after: Document, limit = 512) {
  const changes: SourceValueChange[] = [];
  let truncated = false;
  const visit = (a: unknown, b: unknown, path: (string | number)[]) => {
    if (canonical(a) === canonical(b)) return;
    if (changes.length >= limit) {
      truncated = true;
      return;
    }
    if (a && b && typeof a === "object" && typeof b === "object" && Array.isArray(a) === Array.isArray(b)) {
      const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
      for (const key of keys)
        visit((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key], [
          ...path,
          Array.isArray(a) ? Number(key) : key,
        ]);
    } else changes.push({ target: after.id, path, before: a, after: b });
  };
  visit(before, after, []);
  return { changes, truncated };
}
