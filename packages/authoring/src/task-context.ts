import {
  assemblyPartPoint,
  contentKey,
  idSchema,
  type Project,
  references,
  type Vec3,
  VIEW_MODES,
} from "@wrela/model";
import { z } from "zod";
import { openingMembers } from "./assembly-intent";
import { domainControls, domainFor, domainQuality } from "./authoring-domain";
import { discoverAuthoring } from "./discovery";
import { type ResultConstraint, reviewCameraSchema } from "./review-contract";
export const reviewViewSchema = z
  .strictObject({
    id: idSchema,
    camera: reviewCameraSchema,
    stage: idSchema.optional(),
    rig: z.enum(["neutral", "grazing"]).optional(),
    tick: z.number().int().min(0).max(600).default(0),
    mode: z.enum(VIEW_MODES).optional(),
  })
  .refine((v) => !(v.stage && v.rig), "Choose a source stage or a fixed review rig");
export const reviewViewsSchema = z
  .array(reviewViewSchema)
  .min(1)
  .max(6)
  .refine((v) => new Set(v.map((x) => x.id)).size === v.length, "View IDs must be unique");
export function authoringTaskContext(project: Project, target: string, constraints: ResultConstraint[] = []) {
  const doc = project.documents.find((d) => d.id === target);
  if (!doc) throw Error(`Unknown definition ${target}`);
  let min: Vec3 = [-1, 0, -1],
    max: Vec3 = [1, 2, 1];
  if (doc.kind === "object" || doc.kind === "character") {
    min = [...doc.field.bounds.min];
    max = [...doc.field.bounds.max];
  }
  if (doc.kind === "vegetation") {
    const r = Math.max(doc.radius * 1.5, doc.height * 0.35);
    min = [-r, 0, -r];
    max = [r, doc.height * 1.2, r];
  }
  if (doc.kind === "object" && doc.assembly) {
    min = [Infinity, Infinity, Infinity];
    max = [-Infinity, -Infinity, -Infinity];
    for (const part of doc.assembly.parts) {
      const radius =
        part.profile.kind === "rectangle"
          ? Math.hypot(part.profile.width, part.profile.height) / 2
          : part.profile.kind === "circle"
            ? part.profile.radius
            : Math.max(...part.profile.points.map((p) => Math.hypot(...p)));
      const sourcePoints = part.shell ? part.shell.profile.map((p): Vec3 => [0, p[1], 0]) : part.path;
      for (let n = 0; n < part.repeat.count; n++)
        for (const point of sourcePoints) {
          const p = assemblyPartPoint(doc.assembly, part, point, {}, n);
          for (let a = 0; a < 3; a++) {
            min[a] = Math.min(min[a], p[a] - radius);
            max[a] = Math.max(max[a], p[a] + radius);
          }
        }
    }
  }
  const center = min.map((v, i) => (v + max[i]) / 2) as Vec3,
    distance = Math.max(
      1,
      Math.hypot(...max.map((v, i) => v - min[i])) * (doc.kind === "vegetation" ? 0.88 : 1.6),
    );
  const views = reviewViewsSchema.parse(
    [
      [0, 0, 1],
      [0.65, 0.28, 1],
      [0, 0, -1],
    ].map((offset, i) => ({
      id: ["front", "angle", "reverse"][i],
      camera: { position: center.map((v, a) => v + offset[a] * distance), target: center, fov: 38 },
    })),
  );
  const assembly = doc.kind === "object" ? doc.assembly : undefined,
    members = assembly ? openingMembers(assembly.parts) : undefined;
  if (assembly?.parts.length) {
    const part = assembly.parts.find((p) => p.id === members?.span) ?? assembly.parts[0];
    const end = assemblyPartPoint(assembly, part, part.path[0]);
    const radius = Math.max(
      0.03,
      part.profile.kind === "rectangle"
        ? Math.hypot(part.profile.width, part.profile.height) / 2
        : part.profile.kind === "circle"
          ? part.profile.radius
          : Math.max(...part.profile.points.map((p) => Math.hypot(...p))),
    );
    views.push({
      id: "detail",
      tick: 0,
      camera: {
        position: [end[0] - radius * 5, end[1] + radius * 4, end[2] + radius * 7],
        target: end,
        fov: 42,
      },
    });
  }
  if (doc.kind === "vegetation") {
    const target: Vec3 = [0, doc.height * 0.65, 0];
    const radius = Math.max(0.5, doc.radius);
    views.push({
      id: "detail",
      tick: 0,
      camera: { position: [radius * 1.2, target[1] + radius * 0.3, radius * 2.4], target, fov: 42 },
    });
  }
  const deps = references(doc)
    .map((id) => project.documents.find((d) => d.id === id))
    .filter((d) => !!d);
  const domain = domainFor(project, target);
  return {
    version: 1,
    target,
    sourceKey: contentKey(doc),
    projectKey: contentKey(project),
    kind: doc.kind,
    name: doc.name,
    bounds: {
      min,
      max,
      scope: "Conservative source envelope for framing; measured geometry belongs to review",
    },
    views,
    constraints,
    study: domain
      ? {
          domain,
          controls: domainControls,
          quality: domainQuality(domain),
          input: {
            id: "appearance-1",
            expectedKey: "Use current work key",
            brief: {
              domain,
              target,
              conditions: { exposure: 0.7, moisture: 0.35, maturity: 0.8, variation: 0.45 },
              quality: domainQuality(domain),
            },
            candidates: 3,
          },
          next: "work study <work-id> request.json returns reviewed alternatives and one gallery. Select a passing proposal with work finish.",
          rigPolicy:
            "Neutral and grazing rigs are identical ephemeral review fixtures for every candidate and baseline. They do not modify authored source.",
        }
      : undefined,
    parts: assembly?.parts.slice(0, 12).map((p) => ({
      id: p.id,
      name: p.name,
      profile: p.profile,
      path: p.path,
      position: p.position,
      rotation: p.rotation,
      material: p.material ?? ("material" in doc ? doc.material : undefined),
      sockets: p.sockets,
      attachment: p.mate ?? p.parent,
      joint: p.joint,
      bevel: p.bevel,
    })),
    totalParts: assembly?.parts.length ?? 0,
    repairTargets: assembly?.parts.map((p) => ({
      id: p.id,
      name: p.name,
      material: p.material ?? ("material" in doc ? doc.material : undefined),
      supportsEdgeWear: !p.shell && p.profile.kind === "rectangle" && p.path.length === 2 && p.bevel > 0,
    })),
    diagnosticModes: ["clay", "albedo", "roughness", "normals", "indirect-lighting"],
    openingMembers: members,
    dependencies: deps.slice(0, 12).map((d) => ({
      id: d.id,
      kind: d.kind,
      name: d.name,
      sourceKey: contentKey(d),
      ...(d.kind === "material"
        ? { color: d.color, roughness: d.roughness, detail: d.appearance?.detail }
        : {}),
    })),
    totalDependencies: deps.length,
    operations: discoverAuthoring(project, 0, { target, limit: 8 }).operations,
    examples: {
      ...(members ? { resize: { kind: "assembly.opening", target, width: 2.5, members } } : {}),
      ...(assembly
        ? {
            timber: {
              kind: "assembly.timber",
              target,
              material: `timber-${contentKey(doc).slice(0, 8)}`,
              age: 0.35,
              grainScale: 1.8,
            },
          }
        : {}),
      rename: { kind: "document.rename", target, name: doc.name },
    },
    workflow: {
      context: "author <workspace> work context <work-id> <target-id>",
      evaluate: "author <workspace> work evaluate <work-id> request.json",
      request: {
        expectedKey: "Use work key from inspect",
        proposal: "candidate-1",
        target,
        intent: "Use a typed example above, or supply operations",
        views,
      },
      finish: "author <workspace> work finish <work-id> request.json",
      finishRequest: { expectedKey: "Use key returned by evaluate", proposal: "candidate-1" },
      notes: [
        "Examples are request templates: use the dimensions, appearance and prescribed cameras from your brief.",
        "evaluate proposes, runs constraints, captures matched before/after views and retains a contact sheet in one call.",
        "finish adopts the reviewed source and exports a portable handoff; no artistic acceptance is inferred.",
        "Use fields for paginated source and discover for one schema; repository inspection is unnecessary for supported edits.",
      ],
    },
  };
}
