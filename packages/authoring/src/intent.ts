import { idSchema, type Project, vec3Schema } from "@wrela/model";

import { z } from "zod";
import { openingIntentSchema, planAssemblyIntent, timberIntentSchema } from "./assembly-intent";
import type { Operation } from "./commands";
import { domainRecipeSchema, planDomainRecipe } from "./domain-recipes";
import type { ResultConstraint } from "./review-contract";
import { AuthoringSession } from "./session";

export const authoringIntentSchema = z.discriminatedUnion("kind", [
  openingIntentSchema,
  timberIntentSchema,
  z.strictObject({
    kind: z.literal("forest.shelter"),
    world: idSchema,
    trees: z.array(idSchema).min(1).max(8),
    populations: z.array(idSchema).min(1).max(8),
    maturity: z.number().min(0).max(1),
    canopyDensity: z.number().min(0).max(1),
    populationDensity: z.number().min(0).max(1),
    stiffness: z.number().min(0).max(1),
    clearings: z.array(idSchema).max(16).default([]),
    routes: z.array(idSchema).min(1).max(16),
    sightline: z.strictObject({ from: vec3Schema, to: vec3Schema }).optional(),
  }),
  z.strictObject({
    kind: z.literal("world.route"),
    world: idSchema,
    route: idSchema,
    points: z.array(vec3Schema).min(2).max(64),
    width: z.number().min(1).max(30),
    shoulder: z.number().min(0).max(20),
    cornerRadius: z.number().min(0).max(50),
  }),
  z.strictObject({ kind: z.literal("bundle"), recipes: z.array(domainRecipeSchema).min(1).max(16) }),
]);
export type AuthoringIntent = z.input<typeof authoringIntentSchema>;

/** Explicit intent plans coordinate existing semantic source. They never infer artistic acceptance. */
export function planAuthoringIntent(project: Project, input: AuthoringIntent) {
  const intent = authoringIntentSchema.parse(input),
    operations: Operation[] = [],
    constraints: ResultConstraint[] = [];
  const protect = (target: string, path: (string | number)[]) =>
    constraints.push({
      id: `preserve-${constraints.length}`,
      kind: "source",
      target,
      path,
    });
  if (intent.kind === "assembly.opening" || intent.kind === "assembly.timber") {
    const plan = planAssemblyIntent(project, intent);
    const session = new AuthoringSession(project);
    session.preview({ expectedRevision: 0, operations: plan.operations });
    return { ...plan, impact: session.impact({ expectedRevision: 0, operations: plan.operations }) };
  } else if (intent.kind === "bundle") {
    const session = new AuthoringSession(project);
    for (const recipe of intent.recipes) {
      const plan = planDomainRecipe(session.getSnapshot().project, recipe);
      session.apply({ expectedRevision: session.getSnapshot().revision, operations: plan.operations });
      operations.push(...plan.operations);
      for (const c of plan.preserve) protect(c.target, c.path);
    }
  } else {
    const world = project.documents.find((d) => d.id === intent.world);
    if (world?.kind !== "world" || !world.composition)
      throw Error("Intent requires an authored world composition");
    const composition = structuredClone(world.composition);
    if (intent.kind === "world.route") {
      const path = composition.paths.find((p) => p.id === intent.route);
      if (!path) throw Error("Unknown route");
      Object.assign(path, {
        points: intent.points,
        width: intent.width,
        shoulder: intent.shoulder,
        cornerRadius: intent.cornerRadius,
        flatten: true,
      });
      constraints.push({ id: "route-access", kind: "route", target: world.id, route: path.id });
      protect(world.id, ["instances"]);
    } else {
      for (const treeId of new Set(intent.trees)) {
        const tree = project.documents.find((d) => d.id === treeId);
        if (tree?.kind !== "vegetation" || !tree.botanical) throw Error(`Botanical tree required: ${treeId}`);
        for (const [path, value] of [
          [["botanical", "age"], intent.maturity],
          [["botanical", "canopy", "density"], intent.canopyDensity],
          [["botanical", "motion", "stiffness"], intent.stiffness],
        ] as const)
          operations.push({ kind: "document.set", target: treeId, path: [...path], value });
        protect(treeId, ["height"]);
        protect(treeId, ["seed"]);
      }
      const populations = structuredClone(world.populations);
      for (const id of new Set(intent.populations)) {
        const population = populations.find((p) => p.id === id);
        if (!population || !intent.trees.includes(population.definition))
          throw Error("Selected populations must reference the selected trees");
        population.density = intent.populationDensity;
      }
      for (const id of intent.clearings) {
        const space = composition.spaces.find((s) => s.id === id);
        if (!space) throw Error(`Unknown clearing ${id}`);
        space.clearPopulation = true;
      }
      for (const id of intent.routes) {
        if (!composition.paths.some((p) => p.id === id)) throw Error(`Unknown route ${id}`);
        constraints.push({ id: `route-${constraints.length}`, kind: "route", target: world.id, route: id });
      }
      if (intent.sightline)
        constraints.push({ id: "landmark-view", kind: "sightline", target: world.id, ...intent.sightline });
      operations.push({ kind: "document.set", target: world.id, path: ["populations"], value: populations });
      protect(world.id, ["composition", "paths"]);
    }
    operations.push({ kind: "document.set", target: world.id, path: ["composition"], value: composition });
  }
  const session = new AuthoringSession(project);
  session.preview({ expectedRevision: 0, operations });
  return {
    operations,
    constraints,
    impact: session.impact({ expectedRevision: 0, operations }),
    limitations: [
      "This typed construction plan expresses intent through explicit controls. Artistic quality requires review.",
      "Route/sightline review is sampled terrain and conservative authored-instance bounds; scattered canopy appearance requires matched captures.",
    ],
  };
}
