import { contentKey, type Document, type Quality } from "@wrela/model";
import { PRODUCT_VERSIONS } from "./versions";

export function geometrySource(doc: Document, quality: Quality): unknown {
  if (doc.kind === "object" || doc.kind === "character")
    return {
      version: PRODUCT_VERSIONS.geometry,
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
  });
}
export function productKeys(doc: Document, quality: Quality = "review") {
  return {
    geometry: geometryKey(doc, quality),
    binding: bindingKey(doc, quality),
    motion: motionKey(doc),
    material: materialBindingKey(doc),
  };
}
