import { botanicalGeometrySource, contentKey, type Document, type Quality } from "@wrela/model";

import { PRODUCT_VERSIONS } from "./versions";

/** Creature-only algorithm identity; legacy rendering research versions remain independent. */
export const CREATURE_COMPILER_VERSION = "creature-compiler-5";

export function geometrySource(doc: Document, quality: Quality): unknown {
  if (doc.kind === "object" || doc.kind === "character")
    return {
      version: PRODUCT_VERSIONS.geometry,
      assembly: doc.kind === "object" ? doc.assembly : undefined,
      quality,
      field: {
        ...doc.field,
        nodes: doc.field.nodes.map(({ name: _name, material: _material, ...node }) => node),
      },
    };
  if (doc.kind === "vegetation")
    return {
      version: PRODUCT_VERSIONS.vegetation,
      quality,
      seed: doc.seed,
      height: doc.height,
      radius: doc.radius,
      branches: doc.branches,
      botanical: botanicalGeometrySource(doc.botanical),
      variation: doc.variation,
    };
  return { kind: doc.kind, quality };
}
export function geometryKey(doc: Document, quality: Quality = "review"): string {
  return contentKey(geometrySource(doc, quality));
}
export function materialBindingKey(doc: Document): string {
  return contentKey({
    version: PRODUCT_VERSIONS.material,
    material: "material" in doc ? doc.material : undefined,
    nodes:
      doc.kind === "character" || doc.kind === "object"
        ? doc.field.nodes.map((node) => [node.id, node.material])
        : undefined,
    trunk: doc.kind === "vegetation" ? doc.trunkMaterial : undefined,
    assembly: doc.kind === "object" ? doc.assembly?.parts.map((p) => [p.id, p.material]) : undefined,
  });
}
export function bindingKey(doc: Document, quality: Quality = "review"): string {
  return contentKey({
    version: PRODUCT_VERSIONS.binding,
    geometry: geometryKey(doc, quality),
    joints: doc.kind === "character" ? doc.joints.map(({ name: _name, ...joint }) => joint) : undefined,
  });
}
export function motionKey(doc: Document): string {
  return contentKey({
    version: PRODUCT_VERSIONS.motion,
    motions: doc.kind === "character" ? doc.motions : undefined,
    performance: doc.kind === "character" ? doc.performance : undefined,
  });
}
/** Appearance identities depend on authored material/waves, not names or time. */
export function appearanceKey(doc: Document): string {
  const { name: _name, dependencies: _dependencies, id: _id, ...source } = doc;
  return contentKey({
    version: PRODUCT_VERSIONS.render,
    source: doc.kind === "material" || doc.kind === "water" ? source : materialBindingKey(doc),
  });
}
export function renderSourceKey(doc: Document, quality: Quality = "review"): string {
  return contentKey({ version: PRODUCT_VERSIONS.render, geometry: geometryKey(doc, quality) });
}
export function productKeys(doc: Document, quality: Quality = "review") {
  return {
    geometry: geometryKey(doc, quality),
    binding: bindingKey(doc, quality),
    motion: motionKey(doc),
    material: materialBindingKey(doc),
    appearance: appearanceKey(doc),
    render: renderSourceKey(doc, quality),
    ...(doc.kind === "character" && doc.creature
      ? {
          creature: contentKey({
            version: CREATURE_COMPILER_VERSION,
            products: creatureProductKeys(doc),
            source: doc.creature,
          }),
        }
      : {}),
  };
}

/** Creature products keep their own dependencies; legacy field extraction stays reusable. */
export function creatureProductKeys(doc: Extract<Document, { kind: "character" }>) {
  const source = doc.creature;
  if (!source) return undefined;
  const regions = source.regions.map(({ name: _name, material: _material, ...region }) => region);
  const anatomy = contentKey({ version: "creature-anatomy-1", regions, landmarks: source.landmarks });
  const geometry = contentKey({
    version: "creature-shape-2",
    anatomy,
    field: geometryKey(doc),
    charts: source.charts.map(({ material: _material, ...chart }) => chart),
    sculpts: source.sculpts,
    attachments: source.attachments,
    anchors: source.anchors,
  });
  const appearance = contentKey({
    version: "creature-appearance-2",
    anatomy,
    appearance: source.appearance,
    assignments: [
      source.regions.map((r) => [r.id, r.material]),
      source.charts.map((c) => [c.id, c.material]),
    ],
  });
  const binding = contentKey({
    version: "creature-binding-1",
    geometry,
    joints: doc.joints,
    influences: source.influenceRules,
    correctives: source.correctives,
  });
  const groom = contentKey({ version: "creature-groom-2", geometry, appearance, grooms: source.grooms });
  const performance = contentKey({
    version: "creature-performance-2",
    joints: doc.joints,
    motions: doc.motions,
    contacts: source.contacts,
    ik: source.ikChains,
    secondary: source.secondaryChains,
    expressions: source.expressions,
    cloth: source.cloth,
    articulation: source.articulation,
  });
  return {
    anatomy,
    geometry,
    appearance,
    binding,
    groom,
    performance,
    correspondence: contentKey({ geometry, anchors: source.anchors }),
    review: contentKey(source.reviewScenarios),
  };
}
