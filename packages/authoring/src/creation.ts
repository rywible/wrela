import { type Document, type Project, parseProject } from "@wrela/model";

export type DefinitionCreationPlan = { document: Document; dependencies: Document[] };

/** A previewable creation plan, committed by the caller as one document.create batch. */
export function planDefinitionCreation(
  project: Project,
  kind: Document["kind"],
  templates: readonly Document[] = project.documents,
): DefinitionCreationPlan {
  const source = parseProject(project),
    fixtures = templates,
    dependencies: Document[] = [],
    identities = new Set(source.documents.map((document) => document.id)),
    resolved = new Map<string, string>();
  const allocate = (kind: Document["kind"]) => {
    const base = `${kind}-${crypto.randomUUID().slice(0, 8)}`;
    let id = base,
      suffix = 0;
    while (identities.has(id)) id = `${base}-${++suffix}`;
    identities.add(id);
    return id;
  };
  const create = (template: Document): Document => {
    const document = structuredClone(template);
    document.id = allocate(template.kind);
    if (document.generated) document.generated.policy = "detached";
    return document;
  };
  const resolve = (id: string, reuseKind = true): string => {
    const known = resolved.get(id);
    if (known) return known;
    const template = fixtures.find((document) => document.id === id);
    if (!template) throw Error(`Missing creation template dependency: ${id}`);
    const existing =
      source.documents.find((document) => document.id === id && document.kind === template.kind) ??
      (reuseKind ? source.documents.find((document) => document.kind === template.kind) : undefined);
    if (existing) {
      resolved.set(id, existing.id);
      return existing.id;
    }
    const document = create(template);
    resolved.set(id, document.id);
    remap(document);
    dependencies.push(document);
    return document.id;
  };
  const remap = (document: Document) => {
    switch (document.kind) {
      case "object":
      case "character":
        document.material = resolve(document.material);
        for (const node of document.field.nodes)
          if (node.material) node.material = resolve(node.material, false);
        break;
      case "vegetation":
        document.material = resolve(document.material);
        document.trunkMaterial = resolve(document.trunkMaterial, false);
        break;
      case "terrain":
        document.material = resolve(document.material);
        break;
      case "world":
        document.terrain = resolve(document.terrain);
        document.environment = resolve(document.environment);
        document.lighting = resolve(document.lighting);
        if (document.water) document.water = resolve(document.water);
        if (document.waters) document.waters = document.waters.map((id) => resolve(id));
        for (const population of document.populations) population.definition = resolve(population.definition);
        for (const instance of document.instances) instance.definition = resolve(instance.definition);
        break;
      case "stage":
        document.environment = resolve(document.environment);
        document.lighting = resolve(document.lighting);
        document.subjects = document.subjects.map((id) => resolve(id));
        break;
    }
    document.dependencies = document.dependencies.map((id) => resolve(id));
  };
  const existing = source.documents.find((document) => document.kind === kind),
    template = existing ?? fixtures.find((document) => document.kind === kind);
  if (!template) throw Error(`Unsupported definition kind: ${kind}`);
  const document = create(template);
  document.name = `Untitled ${kind}`;
  if (!existing) {
    // A new composition starts empty; scenery belongs to an explicit authoring
    // choice rather than silently importing the reference scene's entire cast.
    if (document.kind === "world") {
      document.populations = [];
      document.instances = [];
      delete document.water;
      delete document.waters;
    }
    if (document.kind === "stage") document.subjects = [];
    remap(document);
  }
  // Also enforces project capacity and catches wrong-kind identities before the
  // caller commits any part of this plan. Existing source remains untouched.
  parseProject({ ...source, documents: [...source.documents, ...dependencies, document] });
  return { document, dependencies };
}
