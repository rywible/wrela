import {
  type AssemblyPart,
  assemblyPartPoint,
  assemblyPathFrames,
  createSurfaceAppearance,
  idSchema,
  type Project,
} from "@wrela/model";
import { z } from "zod";
import type { Operation } from "./commands";
import type { ResultConstraint } from "./review-contract";

export const openingIntentSchema = z.strictObject({
  kind: z.literal("assembly.opening"),
  target: idSchema,
  width: z.number().finite().min(0.2).max(100),
  members: z.strictObject({ left: idSchema, right: idSchema, span: idSchema }).optional(),
});
export const timberIntentSchema = z.strictObject({
  kind: z.literal("assembly.timber"),
  target: idSchema,
  material: idSchema,
  parts: z.array(idSchema).min(1).max(12).optional(),
  age: z.number().min(0).max(1).default(0.35),
  grainScale: z.number().min(0.2).max(8).default(1.8),
  bevel: z.number().min(0).max(0.02).optional(),
  edgeWear: z.number().min(0).max(1).optional(),
});
function simple(part: AssemblyPart, axis: number) {
  return (
    !part.parent &&
    !part.mate &&
    !part.joint &&
    part.repeat.count === 1 &&
    part.rotation.every((v) => Math.abs(v) < 1e-8) &&
    part.path.length === 2 &&
    part.profile.kind === "rectangle" &&
    part.path[0].every((v, i) => i === axis || Math.abs(v - part.path[1][i]) < 1e-8) &&
    part.path[1][axis] > part.path[0][axis]
  );
}
export function openingMembers(parts: AssemblyPart[]) {
  const posts = parts
    .filter((p) => simple(p, 1))
    .sort((a, b) => a.position[0] + a.path[0][0] - b.position[0] - b.path[0][0]);
  const spans = parts.filter((p) => simple(p, 0));
  return posts.length === 2 && spans.length === 1
    ? { left: posts[0].id, right: posts[1].id, span: spans[0].id }
    : undefined;
}
export function planAssemblyIntent(
  project: Project,
  raw: z.input<typeof openingIntentSchema> | z.input<typeof timberIntentSchema>,
) {
  const intent =
    raw.kind === "assembly.opening" ? openingIntentSchema.parse(raw) : timberIntentSchema.parse(raw);
  const doc = project.documents.find((d) => d.id === intent.target);
  if (doc?.kind !== "object" || !doc.assembly) throw Error("Select an object with authored assembly parts");
  const operations: Operation[] = [],
    constraints: ResultConstraint[] = [];
  const preserve = (path: (string | number)[]) =>
    constraints.push({ id: `assembly-preserve-${constraints.length}`, kind: "source", target: doc.id, path });
  const parts = structuredClone(doc.assembly.parts);
  if (intent.kind === "assembly.opening") {
    const members = intent.members ?? openingMembers(parts);
    if (!members) throw Error("Opening members are ambiguous; specify left, right and span part IDs");
    if (new Set(Object.values(members)).size !== 3) throw Error("Opening requires three distinct members");
    const left = parts.find((p) => p.id === members.left),
      right = parts.find((p) => p.id === members.right),
      span = parts.find((p) => p.id === members.span);
    if (!left || !right || !span || !simple(left, 1) || !simple(right, 1) || !simple(span, 0))
      throw Error(
        "Opening edit supports straight +Y posts and a +X span in the object frame; articulated or attached structural members require an explicit construction plan",
      );
    const extent = (p: AssemblyPart, axis: number) => {
      if (p.profile.kind !== "rectangle") throw Error("Rectangular member required");
      const f = assemblyPathFrames(p)[0];
      return (
        (Math.abs(f.axes[0][axis]) * p.profile.width) / 2 + (Math.abs(f.axes[1][axis]) * p.profile.height) / 2
      );
    };
    const lx = left.position[0] + left.path[0][0],
      rx = right.position[0] + right.path[0][0],
      le = extent(left, 0),
      re = extent(right, 0);
    const oldWidth = rx - re - lx - le;
    if (oldWidth <= 0) throw Error("Selected posts have no positive opening");
    const bottom = left.position[1] + left.path[0][1],
      top = left.position[1] + left.path[1][1];
    if (
      Math.abs(bottom - right.position[1] - right.path[0][1]) > 1e-5 ||
      Math.abs(top - right.position[1] - right.path[1][1]) > 1e-5 ||
      Math.abs(top - (span.position[1] + span.path[0][1] - extent(span, 1))) > 1e-5
    )
      throw Error("Posts and span must share a connected headroom plane before resizing");
    const delta = (intent.width - oldWidth) / 2;
    left.position[0] -= delta;
    right.position[0] += delta;
    span.position[0] -= delta;
    span.path[1][0] += 2 * delta;
    if (span.path[1][0] <= span.path[0][0]) throw Error("Requested width reverses the span");
    parts.forEach((part, i) => {
      const old = doc.assembly!.parts[i];
      if (!Object.values(members).includes(part.id)) {
        preserve(["assembly", "parts", i]);
        return;
      }
      for (const property of [
        "profile",
        "rotation",
        "sockets",
        "material",
        "joint",
        "mate",
        "parent",
      ] as const)
        if (old[property] !== undefined) preserve(["assembly", "parts", i, property]);
    });
    preserve(["material"]);
    operations.push({ kind: "document.set", target: doc.id, path: ["assembly", "parts"], value: parts });
    const center = (lx + le + rx - re) / 2;
    const clearances = structuredClone(doc.assembly.clearances);
    for (const volume of clearances)
      if (Math.abs(volume.min[0] - (lx + le)) < 1e-4 && Math.abs(volume.max[0] - (rx - re)) < 1e-4) {
        volume.min[0] = center - intent.width / 2;
        volume.max[0] = center + intent.width / 2;
      }
    operations.push({
      kind: "document.set",
      target: doc.id,
      path: ["assembly", "clearances"],
      value: clearances,
    });
    const depth = Math.min(extent(left, 2), extent(right, 2), extent(span, 2));
    const z = left.position[2] + left.path[0][2];
    constraints.push({
      id: "opening-clearance",
      kind: "clearance",
      target: doc.id,
      minimum: [center - intent.width / 2 + 0.001, bottom + 0.001, z - depth + 0.001],
      maximum: [center + intent.width / 2 - 0.001, top - 0.001, z + depth - 0.001],
    });
  } else {
    if (project.documents.some((d) => d.id === intent.material))
      throw Error("Use a new material ID; timber authoring never overwrites shared materials");
    const selected = intent.parts ?? parts.map((p) => p.id);
    if (
      selected.length > 12 ||
      new Set(selected).size !== selected.length ||
      selected.some((id) => !parts.some((p) => p.id === id))
    )
      throw Error("Select up to twelve unique existing timber members");
    const appearance = createSurfaceAppearance();
    appearance.detail = { kind: "wood", scale: intent.grainScale, strength: 0.78 };
    appearance.weathering = intent.age * 0.35;
    appearance.dirt = intent.age * 0.1;
    appearance.damage = 0;
    appearance.layers = [
      {
        id: "ground-contact",
        name: "Ground contact staining",
        enabled: true,
        color: [0.075, 0.065, 0.043],
        roughness: 0.88,
        metallic: 0,
        coverage: intent.age * 0.6,
        relief: 0,
        mask: {
          kind: "height",
          scale: 1,
          threshold: 0.5,
          softness: 0.12,
          invert: false,
          minimumHeight: -0.3,
          maximumHeight: 0.22,
        },
      },
    ];
    operations.push({
      kind: "document.create",
      document: {
        id: intent.material,
        name: "Member-aligned weathered timber",
        kind: "material",
        schemaVersion: 1,
        dependencies: [],
        color: [0.27 - intent.age * 0.05, 0.16 + intent.age * 0.025, 0.075 + intent.age * 0.035],
        secondary: [0.13, 0.075, 0.036],
        roughness: 0.68 + intent.age * 0.16,
        metallic: 0,
        pattern: "solid",
        scale: 1,
        normalStrength: 0,
        domain: "local",
        appearance,
      },
    });
    parts.forEach((part, i) => {
      if (!selected.includes(part.id)) {
        preserve(["assembly", "parts", i]);
        return;
      }
      const start = assemblyPartPoint(doc.assembly!, part, part.path[0]);
      const end = assemblyPartPoint(doc.assembly!, part, part.path.at(-1)!);
      const length = Math.hypot(...end.map((v, a) => v - start[a]));
      const slope = (end[1] - start[1]) / length;
      // Local material Y follows the sweep, while deposition remains tied to
      // authored ground height. A horizontal lintel must not stain at one end.
      const base = operations[0];
      if (base.kind !== "document.create" || base.document.kind !== "material")
        throw Error("Missing timber material");
      const material = structuredClone(base.document);
      material.id = `${intent.material.slice(0, 80)}-${i}`;
      if (project.documents.some((d) => d.id === material.id))
        throw Error("Timber material ID already exists");
      const layer = material.appearance!.layers[0];
      if (part.path.length !== 2) material.appearance!.layers = [];
      else if (Math.abs(slope) < 1e-5) {
        layer.mask.kind = "uniform";
        layer.coverage *= Math.exp(-Math.max(0, start[1]) / 0.18);
      } else {
        const a = (-0.3 - start[1]) / slope,
          b = (0.22 - start[1]) / slope;
        layer.mask.minimumHeight = Math.min(a, b);
        layer.mask.maximumHeight = Math.max(a, b);
        layer.mask.softness = Math.min(1, 0.12 / Math.abs(slope));
      }
      operations.push({ kind: "document.create", document: material });
      part.material = material.id;
      if (intent.bevel !== undefined) {
        part.bevel = intent.bevel;
        part.endBevel = intent.bevel;
      }
      if (
        intent.edgeWear !== undefined &&
        part.profile.kind === "rectangle" &&
        part.path.length === 2 &&
        part.bevel > 0
      )
        part.edgeWear = intent.edgeWear;
      for (const property of [
        "profile",
        "path",
        "position",
        "rotation",
        "sockets",
        "joint",
        "mate",
        "parent",
      ] as const)
        if (part[property] !== undefined) preserve(["assembly", "parts", i, property]);
    });
    if (constraints.length > 64)
      throw Error("Timber preservation exceeds the review budget; split the selection");
    operations.push({ kind: "document.set", target: doc.id, path: ["assembly", "parts"], value: parts });
  }
  return {
    operations,
    constraints,
    limitations: [
      "Opening resizing supports an unambiguous orthogonal frame; unsupported attachments are rejected.",
      "Timber uses sweep-local grain and geometric end faces. Ground staining assumes object-local +Y with ground at zero; it does not infer physical joint cavities.",
    ],
  };
}
