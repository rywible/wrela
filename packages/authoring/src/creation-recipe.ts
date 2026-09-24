import {
  contentKey,
  type Document,
  documentSchema,
  idSchema,
  type Project,
  parseProject,
  references,
} from "@wrela/model";
import { z } from "zod";
import type { Operation } from "./commands";
import { reviewVerdict } from "./review-contract";
import { materializeWorkProposal, parseWorkSession, type WorkSession } from "./work-session";

export const capabilityIdSchema = z
  .string()
  .min(1)
  .max(120)
  .regex(/^[a-zA-Z0-9_.-]+$/);
export const creationRecipeSchema = z.strictObject({
  version: z.literal(1),
  id: idSchema,
  revision: z.string().min(1).max(64),
  description: z.string().min(1).max(2000),
  roots: z.array(idSchema).min(1).max(32),
  documents: z.array(documentSchema).min(1).max(256),
  capabilities: z
    .array(z.strictObject({ id: capabilityIdSchema, version: z.string().min(1).max(64) }))
    .max(32),
  provenance: z.strictObject({
    work: idSchema,
    proposal: idSchema,
    sourceKey: z.string(),
    reviewer: z.string(),
    reason: z.string(),
    reviewKeys: z.array(z.string()),
  }),
});
export type CreationRecipe = z.infer<typeof creationRecipeSchema>;
export function promoteCreationRecipe(
  input: WorkSession,
  proposal: string,
  options: Pick<CreationRecipe, "id" | "revision" | "description" | "roots" | "capabilities">,
): CreationRecipe {
  const work = parseWorkSession(input),
    p = work.proposals.find((p) => p.id === proposal);
  if (!p || p.decision?.verdict !== "accepted")
    throw Error("Creation recipe needs a recorded visual acceptance");
  const project = materializeWorkProposal(work, proposal),
    review = p.reviews.at(-1);
  if (!review || !reviewVerdict(review, work.constraints, work.baselineKey, p.candidateKey).passed)
    throw Error("Creation recipe needs a current passing review");
  const retained = new Set<string>();
  function visit(id: string) {
    if (retained.has(id)) return;
    const doc = project.documents.find((d) => d.id === id);
    if (!doc) throw Error(`Missing creation dependency ${id}`);
    retained.add(id);
    for (const ref of references(doc)) visit(ref);
  }
  for (const root of options.roots) visit(root);
  return creationRecipeSchema.parse({
    version: 1,
    ...options,
    documents: project.documents.filter((d) => retained.has(d.id)),
    provenance: {
      work: work.id,
      proposal,
      sourceKey: p.candidateKey,
      reviewer: p.decision.reviewer,
      reason: p.decision.reason,
      reviewKeys: p.reviews.map(contentKey),
    },
  });
}
/** Rebind document references only; names, node/part/branch identities and arbitrary text remain untouched. */
function remapDocument(source: Document, ids: Map<string, string>): Document {
  if (!["material", "object", "vegetation"].includes(source.kind))
    throw Error(
      "Creation recipes currently support rigid objects, vegetation and their material dependencies",
    );
  const doc = structuredClone(source),
    ref = (id: string) => {
      const value = ids.get(id);
      if (!value) throw Error(`Recipe dependency missing: ${id}`);
      return value;
    };
  doc.id = ref(doc.id);
  doc.dependencies = doc.dependencies.map(ref);
  if (doc.generated) doc.generated.policy = "detached";
  if ("material" in doc) doc.material = ref(doc.material);
  if (doc.kind === "object" || doc.kind === "character") {
    for (const node of doc.field.nodes) if (node.material) node.material = ref(node.material);
    if (doc.kind === "object" && doc.assembly)
      for (const part of doc.assembly.parts) if (part.material) part.material = ref(part.material);
    // Creature attachments have additional material references; do not silently produce partial clones.
    if (doc.kind === "character" && doc.creature)
      throw Error(
        "Creature creation uses its dedicated retarget pipeline; this recipe supports rigid assets",
      );
  }
  if (doc.kind === "vegetation") doc.trunkMaterial = ref(doc.trunkMaterial);
  if (doc.kind === "stage") {
    doc.environment = ref(doc.environment);
    doc.lighting = ref(doc.lighting);
    doc.subjects = doc.subjects.map(ref);
  }
  if (doc.kind === "world") {
    doc.terrain = ref(doc.terrain);
    doc.environment = ref(doc.environment);
    doc.lighting = ref(doc.lighting);
    if (doc.water) doc.water = ref(doc.water);
    if (doc.waters) doc.waters = doc.waters.map(ref);
    for (const instance of doc.instances) instance.definition = ref(instance.definition);
    for (const rule of doc.populations) rule.definition = ref(rule.definition);
  }
  return doc;
}
export function instantiateCreationRecipe(project: Project, raw: CreationRecipe, namespace: string) {
  const recipe = creationRecipeSchema.parse(raw);
  idSchema.parse(namespace);
  const ids = new Map(
    recipe.documents.map((d, i) => [d.id, `${namespace}-${i}-${contentKey(d.id).slice(0, 8)}`]),
  );
  if (ids.size !== recipe.documents.length) throw Error("Duplicate creation document");
  for (const id of ids.values()) {
    idSchema.parse(id);
    if (project.documents.some((d) => d.id === id)) throw Error("Creation recipe namespace already exists");
  }
  const documents = recipe.documents.map((d) => remapDocument(d, ids));
  parseProject({ ...project, documents: [...project.documents, ...documents] });
  const roots = recipe.roots.map((id) => {
    const next = ids.get(id);
    if (!next) throw Error("Creation root missing");
    return next;
  });
  return {
    operations: documents.map((document): Operation => ({ kind: "document.create", document })),
    roots,
    identities: Object.fromEntries(ids),
    recipeKey: contentKey(recipe),
    revision: recipe.revision,
    capabilities: recipe.capabilities,
    acceptance: "unreviewed" as const,
  };
}
