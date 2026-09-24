import { contentKey, type Project } from "@wrela/model";

import { z } from "zod";
import { authoringImpact, sourceProperty } from "./discovery";
import { type AuthoringReview, type ResultConstraint, reviewVerdict } from "./review-contract";
import { AuthoringSession } from "./session";

export const experimentSchema = z.strictObject({
  controls: z
    .array(
      z.strictObject({
        target: z.string(),
        path: z
          .array(z.union([z.string(), z.number().int().nonnegative()]))
          .min(1)
          .max(8),
        delta: z
          .number()
          .finite()
          .refine((v) => v !== 0, "Perturbation must be nonzero"),
        units: z.string().min(1).max(40),
      }),
    )
    .min(1)
    .max(8),
  objective: z.strictObject({
    constraint: z.string(),
    metric: z.string(),
    direction: z.enum(["minimize", "maximize"]),
  }),
});
export type AuthoringExperiment = z.input<typeof experimentSchema>;

/** Measured one-control interventions; baseline, invalid trials and failed constraints remain evidence. */
export async function experimentAuthoring(
  project: Project,
  constraints: ResultConstraint[],
  input: AuthoringExperiment,
  evaluate: (
    baseline: Project,
    candidate: Project,
    constraints: ResultConstraint[],
  ) => Promise<AuthoringReview>,
) {
  const request = experimentSchema.parse(input);
  const baseline = await evaluate(project, project, constraints);
  reviewVerdict(baseline, constraints, contentKey(project), contentKey(project));
  const metric = (report: AuthoringReview) => {
    const result = report.results.find((r) => r.id === request.objective.constraint);
    const value = result?.measured[request.objective.metric];
    return result?.status !== "unmeasured" && typeof value === "number" && Number.isFinite(value)
      ? value
      : null;
  };
  const before = metric(baseline);
  const trials = [];
  for (let i = 0; i < request.controls.length; i++) {
    const control = request.controls[i];
    try {
      const doc = project.documents.find((d) => d.id === control.target);
      const original = sourceProperty(doc, control.path);
      if (typeof original !== "number") throw Error("Sensitivity experiments require a numeric control");
      const operation = {
        kind: "document.set" as const,
        target: control.target,
        path: control.path,
        value: original + control.delta,
      };
      const session = new AuthoringSession(project);
      session.apply({ expectedRevision: 0, operations: [operation] });
      const candidate = session.getSnapshot().project,
        report = await evaluate(project, candidate, constraints);
      const verdict = reviewVerdict(report, constraints, contentKey(project), contentKey(candidate));
      const after = metric(report);
      trials.push({
        id: `trial-${i + 1}`,
        control,
        operation,
        before,
        after,
        sensitivity: before !== null && after !== null ? (after - before) / control.delta : null,
        qualified: verdict.passed,
        impact: authoringImpact(project, [operation]),
        report,
        error: null,
      });
    } catch (error) {
      trials.push({ id: `trial-${i + 1}`, control, qualified: false, error: String(error), after: null });
    }
  }
  const improving = trials.filter(
    (t) =>
      t.qualified &&
      t.after !== null &&
      before !== null &&
      (request.objective.direction === "minimize" ? t.after < before : t.after > before),
  );
  improving.sort(
    (a, b) => ((a.after ?? 0) - (b.after ?? 0)) * (request.objective.direction === "minimize" ? 1 : -1),
  );
  return {
    baseline,
    trials,
    suggested: improving[0]?.id ?? null,
    scope:
      "Measured local finite differences over explicit source controls. No derivative guarantee, artistic approval, automatic adoption, or unsampled-view guarantee.",
  };
}

export function explainAuthoringSource(project: Project, target: string, node?: string) {
  const doc = project.documents.find((d) => d.id === target);
  if (!doc) throw Error("Unknown source definition");
  const controls: { path: (string | number)[]; role: string; units: string }[] = [];
  if ("field" in doc) {
    const index = node ? doc.field.nodes.findIndex((n) => n.id === node) : -1;
    if (index >= 0)
      for (const name of ["position", "size", "radius", "blend"])
        controls.push({ path: ["field", "nodes", index, name], role: "local shape", units: "metres" });
    controls.push({
      path: ["field", "resolution"],
      role: "surface realization; inspect feature diagnostics before increasing",
      units: "samples per axis",
    });
  }
  if (doc.kind === "character") {
    controls.push(
      { path: ["joints"], role: "articulation placement and hierarchy", units: "metres/radians" },
      { path: ["motions"], role: "pose and timing", units: "seconds/metres/radians" },
    );
    if (doc.creature)
      controls.push({
        path: ["creature"],
        role: "regional bindings, contacts, correctives and attachments; use creature.fields for focused controls",
        units: "per field",
      });
  }
  if (doc.kind === "vegetation")
    controls.push({ path: ["botanical"], role: "architecture, coverage and motion", units: "per field" });
  if (doc.kind === "world")
    controls.push(
      { path: ["instances"], role: "authored placement", units: "metres/radians" },
      { path: ["composition"], role: "graded paths, clearings and landmarks", units: "metres" },
    );
  if (doc.kind === "material")
    controls.push({ path: ["roughness"], role: "highlight response", units: "unit interval" });
  return {
    target,
    sourceKey: contentKey(doc),
    node: node ?? null,
    controls,
    impact: authoringImpact(project, [{ kind: "document.rename", target, name: doc.name }]),
    diagnoses: [
      "Check rest-shape silhouette before lighting",
      "Compare clay and beauty before changing material",
      "Compare rest and posed views to separate geometry from binding",
      "Use experiments to measure which explicit control changes the failed metric",
    ],
    evidence: "Source attribution and diagnostic hypotheses; experiments supply measured influence.",
  };
}
