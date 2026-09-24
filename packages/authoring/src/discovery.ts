import { contentKey, type Project, references } from "@wrela/model";
import { z } from "zod";
import { authoringCapabilities, capabilityApplies } from "./capabilities";
import type { Operation } from "./commands";
import { availableDomainRecipes, domainRecipeDescriptions, domainRecipeSchema } from "./domain-recipes";
import { editRecipeSchema } from "./edit-recipe";
import { experimentSchema } from "./experiments";
import { authoringIntentSchema } from "./intent";
import { resultConstraintSchema } from "./review-contract";

const pathSchema = z.array(z.union([z.string().min(1).max(120), z.number().int().nonnegative()])).max(16);
export const discoveryQuerySchema = z
  .strictObject({
    target: z.string().optional(),
    search: z.string().max(200).optional(),
    operation: z.string().optional(),
    recipe: z.string().optional(),
    contract: z.enum(["intent", "constraint", "experiment", "editRecipe"]).optional(),
    offset: z.number().int().min(0).default(0),
    limit: z.number().int().min(1).max(32).default(12),
  })
  .refine(
    (q) => [q.operation, q.recipe, q.contract].filter(Boolean).length <= 1,
    "Request one schema at a time",
  );
export type DiscoveryQuery = z.input<typeof discoveryQuerySchema>;
export const inspectionQuerySchema = z.strictObject({
  target: z.string(),
  path: pathSchema.default([]),
  offset: z.number().int().min(0).default(0),
  limit: z.number().int().min(1).max(32).default(12),
  depth: z.number().int().min(0).max(3).default(1),
});
export type InspectionQuery = z.input<typeof inspectionQuerySchema>;

export function sourceProperty(value: unknown, path: (string | number)[]): unknown {
  for (const key of path) {
    if (["__proto__", "prototype", "constructor"].includes(String(key))) throw Error("Unsafe source path");
    if (!value || typeof value !== "object" || !Object.hasOwn(value, key))
      throw Error(`Missing source property: ${path.join("/")}`);
    value = (value as Record<string | number, unknown>)[key];
  }
  return value;
}

function summarize(value: unknown, depth: number): unknown {
  if (!value || typeof value !== "object")
    return typeof value === "string" && value.length > 240
      ? { text: value.slice(0, 240), truncated: true }
      : value;
  const entries = Object.entries(value);
  if (depth === 0) return { type: Array.isArray(value) ? "array" : "object", count: entries.length };
  return {
    entries: entries.slice(0, 12).map(([key, child]) => ({ key, value: summarize(child, depth - 1) })),
    count: entries.length,
    truncated: entries.length > 12,
  };
}

/** Page at the requested path. Large values never silently expand into an agent's context. */
export function inspectAuthoring(project: Project, input: InspectionQuery) {
  const query = inspectionQuerySchema.parse(input);
  const doc = project.documents.find((d) => d.id === query.target);
  if (!doc) throw Error(`Unknown definition ${query.target}`);
  const value = sourceProperty(doc, query.path);
  const entries = value && typeof value === "object" ? Object.entries(value) : null;
  return {
    target: doc.id,
    kind: doc.kind,
    sourceKey: contentKey(doc),
    path: query.path,
    value: entries ? undefined : summarize(value, query.depth),
    entries: entries?.slice(query.offset, query.offset + query.limit).map(([key, child]) => ({
      path: [...query.path, Array.isArray(value) ? Number(key) : key],
      value: summarize(child, query.depth),
    })),
    total: entries?.length ?? 1,
    nextOffset: entries && query.offset + query.limit < entries.length ? query.offset + query.limit : null,
  };
}

export function discoverAuthoring(project: Project, revision: number, input: DiscoveryQuery = {}) {
  const query = discoveryQuerySchema.parse(input);
  const target = query.target ? project.documents.find((d) => d.id === query.target) : undefined;
  if (query.target && !target) throw Error(`Unknown definition ${query.target}`);
  const terms = (query.search ?? "").toLowerCase().split(/\s+/).filter(Boolean);
  const operations = authoringCapabilities
    .filter(
      ({ kind, description }) =>
        (!target || capabilityApplies(kind, target)) &&
        terms.every((term) => `${kind} ${description}`.toLowerCase().includes(term)),
    )
    .map(({ kind, description, targetKinds, effects }) => ({ kind, description, targetKinds, effects }));
  const operation = query.operation && authoringCapabilities.find((c) => c.kind === query.operation)?.schema;
  const recipe = query.recipe && domainRecipeSchema.options.find((s) => s.shape.kind.value === query.recipe);
  if (query.operation && !operation) throw Error("Unknown operation; discover the catalog first");
  if (query.recipe && !recipe) throw Error("Unknown recipe; discover the catalog first");
  const contracts = {
    intent: authoringIntentSchema,
    constraint: resultConstraintSchema,
    experiment: experimentSchema,
    editRecipe: editRecipeSchema,
  };
  const targets = project.documents
    .filter((d) => !target || d.id === target.id)
    .flatMap((d) => {
      const recipes = availableDomainRecipes(d);
      return recipes.length ? [{ id: d.id, name: d.name, recipes }] : [];
    });
  return {
    version: 1,
    revision,
    operations: operations.slice(query.offset, query.offset + query.limit),
    total: operations.length,
    nextOffset: query.offset + query.limit < operations.length ? query.offset + query.limit : null,
    schema: operation
      ? z.toJSONSchema(operation)
      : recipe
        ? z.toJSONSchema(recipe)
        : query.contract
          ? z.toJSONSchema(contracts[query.contract])
          : undefined,
    domainVariants: {
      recipes: Object.entries(domainRecipeDescriptions).map(([kind, description]) => ({ kind, description })),
      targets: targets.slice(query.offset, query.offset + query.limit),
      total: targets.length,
      nextOffset: query.offset + query.limit < targets.length ? query.offset + query.limit : null,
    },
    conventions: { distance: "metres", time: "seconds", angles: "radians", up: "+Y", colors: "linear RGB" },
    workflow: [
      "discover({target,search})",
      "discover({operation})",
      "inspectFields({target,path})",
      "impact(batch)",
      "work.propose",
      "work.review",
      "work.adopt",
    ],
    contracts: Object.keys(contracts),
    work: {
      entry: "work",
      methods: [
        "context (browser: work.fast.context)",
        "evaluate (browser: work.fast.evaluate)",
        "study (browser: work.fast.study; bounded domain search with fixed review rigs)",
        "remember (promote a visually accepted study to a transferable semantic recipe)",
        "finish (browser: work.fast.finish)",
        "list",
        "create/init",
        "open/inspect",
        "propose",
        "review",
        "capture",
        "experiment",
        "feedback",
        "decide",
        "adopt",
        "backup",
        "restore",
        "recipe",
        "recipes",
        "instantiate",
      ],
      documentation: "docs/architecture/agent-authoring.md",
      publication: "Requires current work key and passing declared result constraints",
    },
    limits: { page: 32, operationsPerBatch: 256 },
    fullSchema: "discoverFull() is an explicit compatibility/debugging endpoint; prefer operation schemas.",
  };
}

/** Source dependencies are causal candidates, not measured visual sensitivities. */
export function authoringImpact(project: Project, operations: Operation[]) {
  const changed = new Set(operations.map((o) => (o.kind === "document.create" ? o.document.id : o.target)));
  const affected = new Set(changed);
  const edges: { from: string; to: string }[] = [];
  for (const doc of project.documents)
    for (const ref of references(doc)) edges.push({ from: ref, to: doc.id });
  let progress = true;
  while (progress) {
    progress = false;
    for (const edge of edges)
      if (affected.has(edge.from) && !affected.has(edge.to)) {
        affected.add(edge.to);
        progress = true;
      }
  }
  return {
    changed: [...changed],
    affected: [...affected],
    edges: edges.filter((e) => affected.has(e.from) && affected.has(e.to)),
    scope:
      "Declared source dependency closure; use matched review experiments to measure visible consequences.",
  };
}
