import type { Diagnostic } from "./contracts";
import { validateCreature } from "./creature-validation";
import { type Document, type Project, projectSchema } from "./documents";
import { migrateProjectSource } from "./migrations";
import { performanceIssues } from "./performance";
import { validateRecipeReferences } from "./recipes";
import { assertSourceKeysPreserved, SourceContractError } from "./source-contract";
import { validateWorldComposition, worldCompositionReferences } from "./world-authoring";
export function references(doc: Document): string[] {
  const refs = [...doc.dependencies];
  switch (doc.kind) {
    case "object":
    case "character":
      refs.push(doc.material);
      if (doc.kind === "object" && doc.assembly)
        for (const part of doc.assembly.parts) if (part.material) refs.push(part.material);
      for (const n of doc.field.nodes) if (n.material) refs.push(n.material);
      if (doc.kind === "character" && doc.creature)
        for (const item of [
          ...doc.creature.regions,
          ...doc.creature.charts,
          ...doc.creature.appearance,
          ...doc.creature.grooms,
          ...doc.creature.cloth,
        ])
          if (item.material) refs.push(item.material);
      break;
    case "vegetation":
      refs.push(doc.material, doc.trunkMaterial);
      break;
    case "terrain":
      refs.push(doc.material);
      for (const formation of doc.geology?.formations ?? [])
        if (formation.material) refs.push(formation.material);
      break;
    case "world":
      refs.push(...worldCompositionReferences(doc.composition));
      refs.push(
        doc.terrain,
        doc.environment,
        doc.lighting,
        ...doc.populations.map((p) => p.definition),
        ...doc.instances.map((i) => i.definition),
      );
      if (doc.water) refs.push(doc.water);
      refs.push(...(doc.waters ?? []));
      break;
    case "stage":
      refs.push(doc.environment, doc.lighting, ...doc.subjects);
      break;
  }
  return [...new Set(refs)];
}
export function validateProject(input: unknown): { project?: Project; diagnostics: Diagnostic[] } {
  let migrated: ReturnType<typeof migrateProjectSource>;
  try {
    migrated = migrateProjectSource(input);
  } catch (error) {
    return {
      diagnostics: [
        {
          severity: "error",
          code: "compatibility",
          message: error instanceof Error ? error.message : String(error),
        },
      ],
    };
  }
  const result = projectSchema.safeParse(migrated.source);
  if (!result.success)
    return {
      diagnostics: result.error.issues.map((i) => ({
        severity: "error",
        code: "schema",
        message: `${i.path.join(".")}: ${i.message}`,
      })),
    };
  try {
    assertSourceKeysPreserved(migrated.source, result.data);
  } catch (error) {
    return {
      diagnostics: [
        {
          severity: "error",
          code: error instanceof SourceContractError ? error.code : "schema",
          path: error instanceof SourceContractError ? error.path : undefined,
          message: error instanceof Error ? error.message : String(error),
        },
      ],
    };
  }
  const project = result.data,
    diagnostics = validateProjectRelationships(project);
  for (const step of migrated.steps)
    diagnostics.unshift({ severity: "info", code: "migration", message: step });
  return { project: diagnostics.some((d) => d.severity === "error") ? undefined : project, diagnostics };
}
/** Validate cross-document and domain invariants after schema validation.
 * The incremental form requires unchanged documents to be immutable members of
 * a previously validated project. Graph integrity is always checked in full. */
export function validateProjectRelationships(
  project: Project,
  changedDocuments?: ReadonlySet<string>,
): Diagnostic[] {
  const diagnostics: Diagnostic[] = validateRecipeReferences(project);

  const ids = new Map<string, Document>();
  const error = (message: string, document?: string, node?: string) =>
    diagnostics.push({ severity: "error", code: "reference", message, document, node });
  for (const d of project.documents) {
    if (ids.has(d.id)) error(`Duplicate document identity: ${d.id}`, d.id);
    ids.set(d.id, d);
  }
  if (!ids.has(project.entry)) error("Project entry is missing");
  for (const id of project.dynamicRoots ?? []) if (!ids.has(id)) error(`Dynamic root ${id} is missing`);
  for (const d of project.documents) {
    const refs = references(d);
    if (changedDocuments && !changedDocuments.has(d.id) && !refs.some((id) => changedDocuments.has(id)))
      continue;
    for (const ref of refs) if (!ids.has(ref)) error(`Missing definition: ${ref}`, d.id);
    const unique = (items: { id: string }[], label: string) => {
      const found = new Set<string>();
      for (const item of items) {
        if (found.has(item.id)) error(`${label} IDs must be unique: ${item.id}`, d.id, item.id);
        found.add(item.id);
      }
    };
    const expect = (id: string, kind: Document["kind"] | Document["kind"][]) => {
      const kinds = Array.isArray(kind) ? kind : [kind];
      const target = ids.get(id);
      if (target && !kinds.includes(target.kind)) error(`${id} must be ${kinds.join(" or ")}`, d.id);
    };
    if ("material" in d) expect(d.material, "material");
    if (d.kind === "character")
      for (const issue of performanceIssues(d.joints, d.motions, d.performance)) error(issue, d.id);
    if (d.kind === "object" && d.assembly)
      for (const part of d.assembly.parts) if (part.material) expect(part.material, "material");
    if (d.kind === "object" && d.collision === "compound" && !d.colliders?.length)
      error("Compound collision requires at least one authored collider", d.id);
    if (d.kind === "vegetation") expect(d.trunkMaterial, "material");
    if (d.kind === "terrain") {
      unique(d.interventions, "Intervention");
      for (const formation of d.geology?.formations ?? [])
        if (formation.material) expect(formation.material, "material");
    }
    if (d.kind === "water" && d.domain) {
      if (!d.domain.basins.length && !d.flow?.river)
        error("A bounded water domain needs a basin or river", d.id);
      for (const source of d.domain.sources)
        if (
          source.position.some(
            (coordinate, axis) =>
              coordinate < d.domain!.min[axis] || coordinate > d.domain!.min[axis] + d.domain!.size[axis],
          )
        )
          error("Water source lies outside its simulation domain", d.id);
    }
    if (d.kind === "lighting") unique(d.lights, "Light");
    if (d.kind === "world") {
      if (d.composition)
        diagnostics.push(
          ...validateWorldComposition(
            d.composition,
            d.populations.map((p) => p.id),
            {
              instanceIds: d.instances.map((i) => i.id),
              interventionCount:
                project.documents.find(
                  (item): item is Extract<Document, { kind: "terrain" }> =>
                    item.kind === "terrain" && item.id === d.terrain,
                )?.interventions.length ?? 0,
            },
          ).map((issue) => ({ ...issue, document: d.id })),
        );
      for (const reference of worldCompositionReferences(d.composition))
        expect(reference, ["object", "character", "vegetation"]);
      unique(d.populations, "Population");
      unique(d.instances, "Instance");
      for (const population of d.populations)
        if (population.minHeight > population.maxHeight)
          error("Population minimum height exceeds maximum", d.id, population.id);
      expect(d.terrain, "terrain");
      expect(d.environment, "environment");
      expect(d.lighting, "lighting");
      if (d.water) expect(d.water, "water");
      for (const water of d.waters ?? []) expect(water, "water");
      if (new Set([d.water, ...(d.waters ?? [])].filter(Boolean)).size > 8)
        error("World supports at most eight distinct water bodies", d.id);
      for (const p of d.populations) expect(p.definition, "vegetation");
      for (const i of d.instances) expect(i.definition, ["object", "character", "vegetation"]);
    }
    if (d.kind === "stage") {
      expect(d.environment, "environment");
      expect(d.lighting, "lighting");
    }
    if ("field" in d) {
      const nodes = new Map(d.field.nodes.map((n) => [n.id, n]));
      if (nodes.size !== d.field.nodes.length) error("Field node IDs must be unique", d.id);
      if (!nodes.has(d.field.root)) error("Field root is missing", d.id);
      const active = new Set<string>(),
        costs = new Map<string, { cost: number; depth: number }>();
      const visit = (id: string): { cost: number; depth: number } => {
        if (active.has(id)) {
          error("Field graph contains a cycle", d.id, id);
          return { cost: 0, depth: 0 };
        }
        const known = costs.get(id);
        if (known) return known;
        const n = nodes.get(id);
        if (!n) {
          error(`Missing field node ${id}`, d.id, id);
          return { cost: 0, depth: 0 };
        }
        active.add(id);
        const composition = ["union", "subtract", "intersect", "smoothUnion"].includes(n.kind);
        if (composition && n.children.length < 1) error("Composition requires at least one child", d.id, id);
        if (!composition && n.children.length) error("Primitive shapes cannot contain child nodes", d.id, id);
        if (n.size.some((value) => value <= 0)) error("Shape size must be positive", d.id, id);
        if (n.material) expect(n.material, "material");
        let cost = 1,
          depth = 0;
        for (const child of n.children) {
          const value = visit(child);
          cost = Math.min(513, cost + value.cost);
          depth = Math.max(depth, value.depth + 1);
        }
        if (cost > 512) error("Field exceeds the 512 operation evaluation budget", d.id, id);
        if (depth > 32) error("Field graph exceeds 32 levels", d.id, id);
        const result = { cost, depth };
        active.delete(id);
        costs.set(id, result);
        return result;
      };
      visit(d.field.root);
      for (const node of d.field.nodes) visit(node.id);
      for (let axis = 0; axis < 3; axis++)
        if (
          d.field.bounds.min[axis] >= d.field.bounds.max[axis] ||
          d.field.bounds.max[axis] - d.field.bounds.min[axis] > 200
        )
          error("Field bounds must have positive extent no greater than 200 metres", d.id);
    }
    if (d.kind === "character") {
      diagnostics.push(...validateCreature(d));
      if (d.creature)
        for (const item of [
          ...d.creature.regions,
          ...d.creature.charts,
          ...d.creature.appearance,
          ...d.creature.grooms,
          ...d.creature.cloth,
        ])
          if (item.material) expect(item.material, "material");
      unique(d.motions, "Motion");
      const joints = new Map(d.joints.map((j) => [j.id, j]));
      if (joints.size !== d.joints.length) error("Joint IDs must be unique", d.id);
      for (const j of d.joints) {
        if (j.minimum > j.maximum) error("Joint minimum exceeds maximum", d.id, j.id);
        const seen = new Set([j.id]);
        let parent = j.parent;
        while (parent) {
          if (seen.has(parent)) {
            error("Skeleton contains a cycle", d.id, j.id);
            break;
          }
          seen.add(parent);
          const p = joints.get(parent);
          if (!p) {
            error(`Missing parent joint ${parent}`, d.id, j.id);
            break;
          }
          parent = p.parent;
        }
      }
      for (const m of d.motions) {
        const keyIds = new Set<string>();
        for (const k of m.keys) {
          if (!joints.has(k.joint)) error(`Motion references missing joint ${k.joint}`, d.id);
          if (k.time > m.duration) error("Motion key is outside its duration", d.id, k.joint);
          const id = `${k.joint}:${k.time}`;
          if (keyIds.has(id)) error("Duplicate motion key at the same joint and time", d.id, k.joint);
          keyIds.add(id);
        }
      }
    }
  }
  const seen = new Set<string>(),
    active = new Set<string>();
  const visit = (id: string) => {
    if (active.has(id)) {
      error("Document dependencies contain a cycle", id);
      return;
    }
    if (seen.has(id)) return;
    active.add(id);
    const d = ids.get(id);
    if (d) for (const r of references(d)) visit(r);
    active.delete(id);
    seen.add(id);
  };
  for (const d of project.documents) visit(d.id);
  return diagnostics;
}
export function parseProject(input: unknown): Project {
  const result = validateProject(input);
  if (!result.project)
    throw Object.assign(new Error(result.diagnostics.map((d) => d.message).join("\n")), {
      code: result.diagnostics[0]?.code,
      path: result.diagnostics[0]?.path,
      diagnostics: result.diagnostics,
    });
  return result.project;
}
