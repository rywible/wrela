import { contentKey, idSchema, type Project } from "@wrela/model";

import { z } from "zod";
import { domainBriefSchema, planDomainAuthoring } from "./authoring-domain";
import { type Operation, operationSchema } from "./commands";
import { sourceProperty } from "./discovery";
import { resultConstraintSchema, reviewVerdict } from "./review-contract";
import { AuthoringSession } from "./session";
import { materializeWorkProposal, parseWorkSession, type WorkSession } from "./work-session";

const binding = z.strictObject({
  operation: z.number().int().min(0).max(255),
  path: z
    .array(z.union([z.string(), z.number().int().nonnegative()]))
    .min(1)
    .max(16),
});
export const editRecipeSchema = z.strictObject({
  version: z.literal(1),
  id: idSchema,
  description: z.string().min(1).max(2000),
  semantic: domainBriefSchema.optional(),
  operations: z.array(operationSchema).min(1).max(256),
  constraints: z.array(resultConstraintSchema).max(64),
  parameters: z
    .array(
      z.strictObject({
        name: idSchema,
        description: z.string().min(1).max(400),
        units: z.string().min(1).max(40),
        min: z.number().finite(),
        max: z.number().finite(),
        default: z.number().finite(),
        bindings: z.array(binding).min(1).max(32),
      }),
    )
    .max(32),
  provenance: z.strictObject({
    work: idSchema,
    proposal: idSchema,
    sourceKey: z.string(),
    reviewKeys: z.array(z.string()),
    reviewer: z.string(),
    acceptedReason: z.string(),
    supportedSource: z.string(),
  }),
});
export type EditRecipe = z.infer<typeof editRecipeSchema>;
export function parseEditRecipe(input: unknown): EditRecipe {
  const recipe = editRecipeSchema.parse(input),
    names = new Set<string>(),
    bindings = new Set<string>();
  for (const parameter of recipe.parameters) {
    if (names.has(parameter.name)) throw Error("Duplicate recipe parameter");
    names.add(parameter.name);
    if (parameter.min > parameter.default || parameter.max < parameter.default)
      throw Error("Recipe default lies outside its range");
    for (const b of parameter.bindings) {
      const key = JSON.stringify(b);
      if (bindings.has(key)) throw Error("Recipe parameters cannot share a binding");
      bindings.add(key);
      if (sourceProperty(recipe.operations[b.operation], b.path) !== parameter.default)
        throw Error("Recipe binding does not match its accepted default");
    }
  }
  return recipe;
}
export function promoteEditRecipe(
  work: WorkSession,
  proposal: string,
  input: Pick<EditRecipe, "id" | "description" | "parameters">,
): EditRecipe {
  const parsed = parseWorkSession(work),
    p = parsed.proposals.find((p) => p.id === proposal);
  if (!p || p.decision?.verdict !== "accepted")
    throw Error("Reusable recipes require recorded artistic acceptance");
  materializeWorkProposal(parsed, proposal);
  const review = p.reviews.at(-1);
  if (
    parsed.constraints.length &&
    (!review || !reviewVerdict(review, parsed.constraints, parsed.baselineKey, p.candidateKey).passed)
  )
    throw Error("Reusable recipes require passing result constraints");
  const result = parseEditRecipe({
    version: 1,
    ...input,
    operations: p.batch.operations,
    constraints: parsed.constraints,
    provenance: {
      work: parsed.id,
      proposal: p.id,
      sourceKey: p.candidateKey,
      reviewKeys: p.reviews.map(contentKey),
      reviewer: p.decision.reviewer,
      acceptedReason: p.decision.reason,
      supportedSource: parsed.baselineKey,
    },
  });
  return result;
}
export function instantiateEditRecipe(
  project: Project,
  input: EditRecipe,
  parameters: Record<string, number> = {},
  targets: Record<string, string> = {},
) {
  const recipe = parseEditRecipe(input),
    operations = structuredClone(recipe.operations);
  if (recipe.semantic) {
    if (Object.keys(parameters).length)
      throw Error(
        "Semantic recipes use their approved conditions; author a new study to explore other conditions",
      );
    const plan = planDomainAuthoring(
      project,
      { ...recipe.semantic, target: targets[recipe.semantic.target] ?? recipe.semantic.target },
      `recipe-${recipe.id}-${contentKey(project)}`,
    );
    new AuthoringSession(project).preview({ expectedRevision: 0, operations: plan.operations });
    return {
      operations: plan.operations,
      constraints: plan.constraints,
      provenance: recipe.provenance,
      acceptance: "unreviewed",
      supportedBaseline: recipe.provenance.supportedSource === contentKey(project),
      limitation:
        "Conditions transfer through domain rules; quality and applicability need fresh review on the new asset.",
    };
  }
  for (const name of Object.keys(parameters))
    if (!recipe.parameters.some((p) => p.name === name)) throw Error(`Unknown recipe parameter ${name}`);
  for (const p of recipe.parameters) {
    const value = parameters[p.name] ?? p.default;
    if (!Number.isFinite(value) || value < p.min || value > p.max)
      throw Error(`Parameter outside supported range: ${p.name}`);
    for (const b of p.bindings) {
      sourceProperty(operations[b.operation], b.path);
      const parent = sourceProperty(operations[b.operation], b.path.slice(0, -1)) as Record<
        string | number,
        unknown
      >;
      parent[b.path.at(-1) as string | number] = value;
    }
  }
  for (const operation of operations) {
    if (operation.kind === "document.create") {
      if (Object.keys(targets).length)
        throw Error(
          "Creation recipes require explicit dependency-aware source edits; target rebinding is for edits",
        );
    } else operation.target = targets[operation.target] ?? operation.target;
  }
  const constraints = recipe.constraints.map((c) => ({ ...c, target: targets[c.target] ?? c.target }));
  const session = new AuthoringSession(project);
  session.preview({ expectedRevision: 0, operations });
  return {
    operations: operations as Operation[],
    constraints,
    provenance: recipe.provenance,
    acceptance: "unreviewed",
    supportedBaseline: recipe.provenance.supportedSource === contentKey(project),
    limitation:
      "Parameter ranges are declared applicability, not certified artistic quality; every instantiation needs fresh constraint and visual review.",
  };
}
