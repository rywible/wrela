import {
  type AssemblyPart,
  assemblyPartSchema,
  contentKey,
  createSurfaceAppearance,
  createSurfaceLayer,
  type Document,
  idSchema,
  type MaterialDefinition,
  type ObjectDefinition,
  type Project,
  parseProject,
  revolvedShellSchema,
  type Vec3,
  vec3Schema,
} from "@wrela/model";
import { z } from "zod";
import type { Operation } from "./commands";

const dimension = z.number().finite().min(0.001).max(20);
const componentBase = {
  id: idSchema,
  name: z.string().min(1).max(120),
  material: z.enum(["iron", "brass", "glass", "light", "wood"]),
  position: vec3Schema.default([0, 0, 0]),
  rotation: vec3Schema.default([0, 0, 0]),
  parent: idSchema.optional(),
  joint: assemblyPartSchema.shape.joint,
};
export const heroComponentSchema = z.discriminatedUnion("kind", [
  z.strictObject({ ...componentBase, kind: z.literal("shell"), shell: revolvedShellSchema }),
  z.strictObject({
    ...componentBase,
    kind: z.literal("rod"),
    path: z.array(vec3Schema).min(2).max(64),
    radius: dimension,
  }),
  z.strictObject({
    ...componentBase,
    kind: z.literal("beam"),
    path: z.array(vec3Schema).min(2).max(64),
    width: dimension,
    depth: dimension,
    bevel: dimension.optional(),
  }),
  z.strictObject({
    ...componentBase,
    kind: z.literal("ring"),
    radius: dimension,
    tube: dimension,
    segments: z.number().int().min(12).max(64).default(40),
  }),
  z.strictObject({
    ...componentBase,
    kind: z.literal("plate"),
    width: dimension,
    height: dimension,
    thickness: dimension,
    bevel: dimension.optional(),
  }),
]);
export const heroDesignSchema = z
  .strictObject({
    id: idSchema,
    name: z.string().min(1).max(120),
    family: z.string().min(1).max(100),
    intent: z.string().min(1).max(2000),
    seed: z.number().int().min(0).max(0x7fffffff).default(73),
    stage: z.enum(["blockout", "construction", "surface", "production"]).default("surface"),
    age: z.number().min(0).max(1).default(0.5),
    components: z.array(heroComponentSchema).min(1).max(96),
  })
  .superRefine((v, c) => {
    if (new Set(v.components.map((p) => p.id)).size !== v.components.length)
      c.addIssue({ code: "custom", message: "Component identities must be unique" });
    for (const part of v.components)
      if (part.kind === "ring" && part.tube >= part.radius)
        c.addIssue({ code: "custom", message: "Ring tube must be smaller than its radius" });
  });
export type HeroDesign = z.infer<typeof heroDesignSchema>;
export type HeroDesignInput = z.input<typeof heroDesignSchema>;
export const HERO_CONSTRUCTION_VERSION = "hero-construction-1";
export const HERO_CAPABILITIES = [
  { id: "assembly.sweep", version: "1" },
  { id: "assembly.revolved-shell", version: "1" },
  { id: "material.hero-palette", version: "1" },
] as const;

function material(
  id: string,
  kind: HeroDesign["components"][number]["material"],
  age: number,
  blockout: boolean,
): MaterialDefinition {
  const colors = {
    iron: [0.075, 0.09, 0.095],
    brass: [0.46, 0.26, 0.085],
    glass: [0.52, 0.69, 0.72],
    light: [1, 0.56, 0.13],
    wood: [0.22, 0.12, 0.055],
  } as const;
  const appearance = createSurfaceAppearance(
    kind === "glass" ? "glass" : kind === "iron" || kind === "brass" ? "metal" : "generic",
  );
  appearance.weathering = age * 0.24;
  appearance.dirt = age * 0.08;
  appearance.historyScale = 11;
  if (kind === "wood") appearance.detail = { kind: "wood", scale: 1.3, strength: 0.6 };
  if (kind === "glass") {
    appearance.transmission = 0.94;
    appearance.indexOfRefraction = 1.5;
    appearance.response = { thickness: 0.003, clearcoat: 0.9, clearcoatRoughness: 0.08 };
  }
  if (kind === "brass" || kind === "iron") {
    const oxide = createSurfaceLayer("oxide");
    oxide.color = kind === "brass" ? [0.065, 0.16, 0.12] : [0.16, 0.065, 0.025];
    oxide.coverage = age * 0.4;
    oxide.roughness = 0.86;
    oxide.mask.scale = 17;
    oxide.mask.threshold = 0.58;
    oxide.mask.softness = 0.2;
    appearance.layers = [oxide];
    appearance.response = { anisotropy: 0.22, clearcoat: 0.08 };
  }
  return {
    id,
    name: kind,
    schemaVersion: 1,
    dependencies: [],
    kind: "material",
    color: blockout ? [0.4, 0.4, 0.4] : [...colors[kind]],
    secondary: [0.08, 0.055, 0.035],
    roughness: blockout ? 0.8 : kind === "glass" ? 0.1 : kind === "brass" ? 0.3 : 0.52,
    metallic: !blockout && (kind === "iron" || kind === "brass") ? 1 : 0,
    pattern: "solid",
    scale: 1,
    normalStrength: 0,
    appearance: blockout ? createSurfaceAppearance("generic") : appearance,
    emission: { color: [1, 0.38, 0.07] as Vec3, intensity: !blockout && kind === "light" ? 3 : 0 },
  };
}
/** An empty asset namespace: one neutral material satisfies the project envelope, no geometry/templates. */
export function createHeroWorkspace(): Project {
  return parseProject({
    schemaVersion: 1,
    id: "hero-workspace",
    name: "Hero workshop",
    entry: "workshop-neutral",
    documents: [material("workshop-neutral", "iron", 0, true)],
  });
}
/** Lower a bounded semantic component graph to ordinary editable source. No target-specific fixture cloning. */
export function buildHeroDesign(raw: HeroDesignInput): {
  design: HeroDesign;
  documents: Document[];
  target: string;
  capabilities: typeof HERO_CAPABILITIES;
} {
  const design = heroDesignSchema.parse(raw),
    used = [...new Set(design.components.map((p) => p.material))];
  const materials = used.map((k) =>
    material(`${design.id}-${k}`, k, design.age, design.stage === "blockout"),
  );
  const parts: AssemblyPart[] = design.components.map((p, index) => {
    const base: AssemblyPart = {
      id: p.id,
      name: p.name,
      material: `${design.id}-${p.material}`,
      position: p.position,
      rotation: p.rotation,
      parent: p.parent,
      joint: p.joint,
      profile: { kind: "circle", radius: 0.01, segments: 16 },
      path: [
        [0, 0, 0],
        [0, 1, 0],
      ],
      bevel: 0,
      repeat: { count: 1, offset: [0, 0, 0] },
      sockets: [],
      wear: {
        amount: design.stage === "blockout" ? 0 : design.age * 0.08,
        scale: 3,
        seed: design.seed + index * 97,
      },
    };
    if (p.kind === "shell") {
      base.shell = p.shell;
      base.profile = {
        kind: "circle",
        radius: Math.max(...p.shell.profile.map((x) => x[0])),
        segments: p.shell.segments,
      };
      base.path = [
        [0, Math.min(...p.shell.profile.map((x) => x[1])), 0],
        [0, Math.max(...p.shell.profile.map((x) => x[1])), 0],
      ];
    }
    if (p.kind === "rod") {
      base.profile = { kind: "circle", radius: p.radius, segments: 16 };
      base.path = p.path;
    }
    if (p.kind === "beam") {
      base.profile = { kind: "rectangle", width: p.width, height: p.depth };
      base.path = p.path;
      base.bevel = p.bevel ?? Math.min(p.width, p.depth) * 0.08;
      base.endBevel = base.bevel;
    }
    if (p.kind === "plate") {
      base.profile = { kind: "rectangle", width: p.width, height: p.height };
      base.path = [
        [0, 0, -p.thickness / 2],
        [0, 0, p.thickness / 2],
      ];
      base.bevel = p.bevel ?? Math.min(p.width, p.height) * 0.025;
      base.endBevel = Math.min(base.bevel, p.thickness * 0.2);
    }
    if (p.kind === "ring") {
      // A circular closed meridian preserves a hole without coincident sweep end caps.
      base.shell = {
        segments: p.segments,
        profile: Array.from({ length: 12 }, (_, i) => {
          const a = (i * Math.PI * 2) / 12;
          return [p.radius + p.tube * Math.cos(a), p.tube * Math.sin(a)];
        }),
      };
      base.profile = { kind: "circle", radius: p.radius + p.tube, segments: p.segments };
      base.path = [
        [0, -p.tube, 0],
        [0, p.tube, 0],
      ];
    }
    base.sockets = [
      { id: "origin", position: [0, 0, 0] },
      { id: "tip", position: [...base.path.at(-1)!] },
    ];
    return base;
  });
  const object: ObjectDefinition = {
    id: design.id,
    name: design.name,
    schemaVersion: 1,
    dependencies: materials.map((m) => m.id),
    kind: "object",
    material: materials[0].id,
    collision: "mesh",
    assembly: { parts, grid: 0.001, clearances: [] },
    field: {
      root: "placeholder",
      nodes: [
        {
          id: "placeholder",
          name: "Assembly envelope",
          kind: "box",
          position: [0, 0.5, 0],
          rotation: [0, 0, 0],
          size: [1, 1, 1],
          radius: 1,
          blend: 0,
          children: [],
        },
      ],
      bounds: { min: [-1, 0, -1], max: [1, 2, 1] },
      resolution: 16,
    },
    generated: {
      generator: HERO_CONSTRUCTION_VERSION,
      policy: "detached",
      recipe: {
        id: design.id,
        version: HERO_CONSTRUCTION_VERSION,
        key: contentKey(design),
        parameters: { age: design.age, seed: design.seed },
        overrides: [],
      },
    },
  };
  const documents: Document[] = [...materials, object];
  parseProject({ schemaVersion: 1, id: "hero-validation", name: design.name, documents, entry: design.id });
  return { design, documents, target: design.id, capabilities: HERO_CAPABILITIES };
}
export function heroCreationOperations(project: Project, design: HeroDesignInput): Operation[] {
  const result = buildHeroDesign(design);
  if (result.documents.some((d) => project.documents.some((existing) => existing.id === d.id)))
    throw Error("Creation namespace exists; use a new asset ID or branch the existing proposal");
  return result.documents.map((document) => ({ kind: "document.create", document }));
}
/** Advance an owned design without replacing unrelated documents. Pins are checked by the iteration layer. */
export function heroRevisionOperations(project: Project, raw: HeroDesignInput): Operation[] {
  const result = buildHeroDesign(raw),
    root = project.documents.find((d) => d.id === result.target);
  if (!root) return heroCreationOperations(project, raw);
  if (root.kind !== "object" || root.generated?.generator !== HERO_CONSTRUCTION_VERSION)
    throw Error("This asset is not owned by the hero construction recipe; use targeted edits");
  const owned = new Set([root.id, ...root.dependencies]),
    operations: Operation[] = [];
  for (const document of result.documents) {
    const old = project.documents.find((d) => d.id === document.id);
    if (!old) {
      operations.push({ kind: "document.create", document });
      continue;
    }
    if (!owned.has(old.id) || old.kind !== document.kind)
      throw Error("Design revision collides with unrelated source");
    for (const [key, value] of Object.entries(document))
      if (
        !["id", "kind", "schemaVersion", "generated", "dependencies"].includes(key) &&
        contentKey(value) !== contentKey((old as unknown as Record<string, unknown>)[key] ?? null)
      )
        operations.push({ kind: "document.set", target: document.id, path: [key], value });
  }
  // Old materials may be shared by another object; leave unused definitions available rather than deleting them.
  return operations;
}
