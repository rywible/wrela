import { type Document, environmentGradeSchema, idSchema, type Project } from "@wrela/model";

import { z } from "zod";
import type { Operation } from "./commands";
import { fitCreatureLandmarkSpan } from "./creature-coherence";
import type { SourcePreservation } from "./domain-constraints";
import { retimePerformanceClip } from "./performance";

const unit = z.number().finite().min(0).max(1);
const target = { target: idSchema };
export const domainRecipeSchema = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("vegetation.canopy"), ...target, density: unit }),
  z.strictObject({
    kind: z.literal("assembly.weather"),
    ...target,
    amount: unit,
    parts: z.array(idSchema).min(1).max(128).optional(),
  }),
  z.strictObject({ kind: z.literal("geology.erosion"), ...target, strength: unit }),
  z.strictObject({
    kind: z.literal("material.history"),
    ...target,
    weathering: unit.optional(),
    wetness: unit.optional(),
    dirt: unit.optional(),
    damage: unit.optional(),
  }),
  z.strictObject({
    kind: z.literal("creature.landmarkSpan"),
    ...target,
    first: idSchema,
    second: idSchema,
    distance: z.number().finite().positive().max(100),
    descendants: z.boolean().default(false),
    protectedRegions: z.array(idSchema).max(128).default([]),
  }),
  z.strictObject({
    kind: z.literal("performance.retime"),
    ...target,
    motion: idSchema,
    duration: z.number().finite().min(0.1).max(120),
  }),
  z.strictObject({ kind: z.literal("world.population"), ...target, rule: idSchema, density: unit }),
  z.strictObject({ kind: z.literal("environment.grade"), ...target, grade: environmentGradeSchema }),
]);
export type DomainRecipe = z.input<typeof domainRecipeSchema>;
export type AuthoringDomain =
  | "creatures"
  | "assemblies"
  | "vegetation"
  | "geology"
  | "materials"
  | "world"
  | "performance"
  | "environment";
export type DomainRecipePlan = {
  domain: AuthoringDomain;
  operations: Operation[];
  preserve: SourcePreservation[];
  review: { subjects: string[]; views: string[]; measurements: string[]; limitations: string[] };
};

export const domainRecipeDescriptions = {
  "vegetation.canopy":
    "Change botanical canopy density while preserving trunk, growth, pruning and branch-edit source.",
  "assembly.weather":
    "Vary existing per-part optical wear while preserving dimensions, sockets, joints and clearance source.",
  "geology.erosion":
    "Change talus erosion strength while retaining authored corridor elevation profiles and local interventions.",
  "material.history":
    "Vary material weathering, wetness, dirt and damage while preserving physical relief and optical substance controls.",
  "creature.landmarkSpan":
    "Fit two existing landmarks by uniform regional scaling through the established coherent proportion operation.",
  "performance.retime":
    "Retime a clip with facial keys, events, contacts and alignment timing through the existing semantic retime operation.",
  "world.population":
    "Vary one population's density while preserving its seed, spatial admission rules and authored layout/placement exceptions.",
  "environment.grade":
    "Change base and sequence exposure/tint while preserving all weather values, timestamps and interpolation.",
} as const;

/** Recipes expand to existing source operations. No hidden generator or alternate transaction authority. */
export function planDomainRecipe(project: Project, input: DomainRecipe): DomainRecipePlan {
  const recipe = domainRecipeSchema.parse(input);
  const document = project.documents.find((value) => value.id === recipe.target);
  if (!document) throw Error(`Definition ${recipe.target} does not exist`);
  const operations: Operation[] = [],
    preserve: SourcePreservation[] = [];
  const set = (path: (string | number)[], value: unknown) =>
    operations.push({ kind: "document.set", target: document.id, path, value });
  const protect = (path: (string | number)[], reason: string) =>
    preserve.push({ target: document.id, path, reason });
  const protectOther = (owner: object, prefix: (string | number)[], mutable: string[], reason: string) => {
    for (const key of Object.keys(owner)) if (!mutable.includes(key)) protect([...prefix, key], reason);
  };
  let domain: AuthoringDomain;
  const limitations: string[] = [];
  switch (recipe.kind) {
    case "vegetation.canopy": {
      if (document.kind !== "vegetation" || !document.botanical)
        throw Error("Canopy edits require botanical vegetation");
      domain = "vegetation";
      set(["botanical", "canopy", "density"], recipe.density);
      protectOther(document, [], ["botanical"], "Preserve whole-plant dimensions, seed and bindings");
      protectOther(
        document.botanical,
        ["botanical"],
        ["canopy"],
        "Preserve woody growth and local branch edits",
      );
      protectOther(
        document.botanical.canopy,
        ["botanical", "canopy"],
        ["density"],
        "Preserve leaf dimensions and distribution controls",
      );
      limitations.push(
        "Density is source intent; projected canopy coverage, shadow energy and temporal stability require rendered review.",
      );
      break;
    }
    case "assembly.weather": {
      if (document.kind !== "object" || !document.assembly) throw Error("Weather edits require an assembly");
      domain = "assemblies";
      const selected = new Set(recipe.parts ?? document.assembly.parts.map((part) => part.id));
      if ([...selected].some((id) => !document.assembly?.parts.some((part) => part.id === id)))
        throw Error("Unknown assembly part");
      document.assembly.parts.forEach((part, index) => {
        if (selected.has(part.id)) {
          set(["assembly", "parts", index, "wear", "amount"], recipe.amount);
          protectOther(
            part,
            ["assembly", "parts", index],
            ["wear"],
            "Preserve part geometry, assembly connections and articulation",
          );
          protect(["assembly", "parts", index, "wear", "scale"], "Preserve wear scale");
          protect(["assembly", "parts", index, "wear", "seed"], "Preserve wear identity");
        } else protect(["assembly", "parts", index], "Preserve unselected part");
      });
      protectOther(
        document.assembly,
        ["assembly"],
        ["parts"],
        "Preserve exact authored clearance and assembly settings",
      );
      protectOther(document, [], ["assembly"], "Preserve source outside assembly");
      limitations.push(
        "Existing assembly wear is optical coloration; this operation does not create chipped geometry or certify swept collision.",
      );
      break;
    }
    case "geology.erosion": {
      if (document.kind !== "terrain" || !document.geology)
        throw Error("Erosion edits require authored geology");
      domain = "geology";
      if (!document.geology.corridors?.length)
        throw Error("Author a protected corridor before proposing route-preserving erosion");
      set(["geology", "erosion", "strength"], recipe.strength);
      protectOther(document, [], ["geology"], "Preserve base terrain and local interventions");
      protectOther(
        document.geology,
        ["geology"],
        ["erosion"],
        "Preserve corridor profiles, formations and review route",
      );
      protectOther(
        document.geology.erosion,
        ["geology", "erosion"],
        ["strength"],
        "Preserve erosion support and talus angle",
      );
      limitations.push(
        "Corridor source is unchanged and remains applied after erosion; slope, body clearance and formation collisions still require compiled route review.",
      );
      break;
    }
    case "material.history": {
      if (document.kind !== "material" || !document.appearance)
        throw Error("History edits require authored surface appearance");
      domain = "materials";
      const controls = ["weathering", "wetness", "dirt", "damage"] as const;
      const changed = controls.filter((key) => recipe[key] !== undefined);
      if (!changed.length) throw Error("Specify at least one surface history control");
      for (const key of changed) set(["appearance", key], recipe[key]);
      protectOther(document, [], ["appearance"], "Preserve base material and source domain");
      protectOther(
        document.appearance,
        ["appearance"],
        changed,
        "Preserve substance, relief, detail, layers and other history",
      );
      limitations.push(
        "Material edits affect every user of this material; make a local material first for isolated treatment.",
      );
      break;
    }
    case "creature.landmarkSpan": {
      if (document.kind !== "character") throw Error("Landmark fitting requires a character");
      domain = "creatures";
      operations.push(
        ...fitCreatureLandmarkSpan(document, {
          first: recipe.first,
          second: recipe.second,
          distance: recipe.distance,
          descendants: recipe.descendants,
          preserve: recipe.protectedRegions,
        }),
      );
      protect(["material"], "Preserve body material ownership");
      limitations.push(
        "One span specifies uniform regional scale; it does not infer a finished anatomical shape. Coherent fitting is enforced by creature proportion operations.",
      );
      break;
    }
    case "performance.retime": {
      if (document.kind !== "character") throw Error("Retiming requires a character");
      domain = "performance";
      const result = retimePerformanceClip(document, recipe.motion, recipe.duration);
      set(["motions"], result.motions);
      set(["performance"], result.performance);
      if (result.creature) set(["creature"], result.creature);
      protectOther(
        document,
        [],
        ["motions", "performance", "creature"],
        "Preserve rest anatomy, rig, source identity and physics",
      );
      document.motions.forEach((motion, index) => {
        if (motion.id !== recipe.motion) protect(["motions", index], "Preserve other clips");
      });
      limitations.push(
        "Key/event/contact alignment is retained; contacts and weight transfer require runtime review at the new speed.",
      );
      break;
    }
    case "world.population": {
      if (document.kind !== "world") throw Error("Population edits require a world");
      domain = "world";
      const index = document.populations.findIndex((rule) => rule.id === recipe.rule);
      if (index < 0) throw Error("Unknown population rule");
      set(["populations", index, "density"], recipe.density);
      protectOther(
        document,
        [],
        ["populations"],
        "Preserve authored layout, routes, spaces and explicit exceptions",
      );
      document.populations.forEach((rule, i) => {
        if (i === index)
          protectOther(
            rule,
            ["populations", i],
            ["density"],
            "Preserve population identities, spacing and spatial admission",
          );
        else protect(["populations", i], "Preserve other populations");
      });
      limitations.push(
        "Authored route/layout source is unchanged; new admitted instances still require runtime traversal and habitat composition review.",
      );
      break;
    }
    case "environment.grade": {
      if (document.kind !== "environment") throw Error("Grading requires an environment");
      domain = "environment";
      set(["grade"], recipe.grade);
      if (document.sequence) {
        const sequence = structuredClone(document.sequence);
        sequence.keyframes.forEach((key) => {
          key.state.grade = structuredClone(recipe.grade);
        });
        set(["sequence"], sequence);
        protectOther(
          document.sequence,
          ["sequence"],
          ["keyframes"],
          "Preserve sequence timing and interpolation",
        );
        document.sequence.keyframes.forEach((key, index) => {
          protect(["sequence", "keyframes", index, "time"], "Preserve weather key time");
          protectOther(
            key.state,
            ["sequence", "keyframes", index, "state"],
            ["grade"],
            "Preserve every weather state",
          );
        });
      }
      protectOther(document, [], ["grade", "sequence"], "Preserve sky, atmosphere and wind source");
      limitations.push(
        "This sets a consistent grade across existing weather states; it does not change sun transport or certify exposure across all views.",
      );
      break;
    }
  }
  return {
    domain,
    operations,
    preserve,
    review: {
      subjects: [document.id],
      views: ["neutral", "silhouette", "detail", "gameplay", "motion"],
      measurements: ["editToCompleteFrameMs", "compileMs", "gpuP95Ms", "residentBytes", "temporalError"],
      limitations,
    },
  };
}

export function availableDomainRecipes(document: Document): (keyof typeof domainRecipeDescriptions)[] {
  switch (document.kind) {
    case "vegetation":
      return document.botanical ? ["vegetation.canopy"] : [];
    case "object":
      return document.assembly ? ["assembly.weather"] : [];
    case "terrain":
      return document.geology?.corridors?.length ? ["geology.erosion"] : [];
    case "material":
      return document.appearance ? ["material.history"] : [];
    case "character": {
      const landmarks = document.creature?.landmarks ?? [];
      const hasFittableSpan = landmarks.some((first, index) =>
        landmarks
          .slice(index + 1)
          .some(
            (second) =>
              first.id !== second.id &&
              first.region === second.region &&
              Math.hypot(...first.position.map((value, axis) => value - second.position[axis])) >= 1e-8,
          ),
      );
      return [
        ...(hasFittableSpan ? ["creature.landmarkSpan" as const] : []),
        ...(document.motions.length ? ["performance.retime" as const] : []),
      ];
    }
    case "world":
      return document.populations.length ? ["world.population"] : [];
    case "environment":
      return ["environment.grade"];
    default:
      return [];
  }
}
