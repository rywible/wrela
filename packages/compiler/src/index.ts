import { contentKey, type Document, documentSchema, type Quality, type SurfaceArtifact } from "@wrela/model";
import { COMPILER_VERSION, compileCharacter, compileSurface, surfaceGeometrySource } from "./surface";
import { compileVegetation, vegetationGeometrySource } from "./vegetation";

export * from "./cooked";
export * from "./field";
export * from "./ir";
export * from "./products";
export * from "./surface";
export * from "./terrain";
export * from "./vegetation";
export { BUNDLE_COMPILER_SOURCE, COOK_FORMAT_VERSION, PRODUCT_VERSIONS } from "./versions";

/** Geometry keys depend only on source relevant to this product. Separate
 * material parameter edits never invalidate extracted geometry. */
export function compilerKey(doc: Document, quality: Quality = "review"): string {
  if (doc.kind === "object" || doc.kind === "character")
    return contentKey(surfaceGeometrySource(doc, quality));
  if (doc.kind === "vegetation") return contentKey(vegetationGeometrySource(doc, quality));
  const source = Object.fromEntries(
    Object.entries(doc).filter(([key]) => key !== "name" && key !== "dependencies"),
  );
  return contentKey({ compiler: COMPILER_VERSION, quality, source });
}
export function compileDocument(document: Document, quality: Quality = "review"): SurfaceArtifact | null {
  if (!["interactive", "review", "export"].includes(quality)) throw new Error("Unknown compilation quality");
  const doc = documentSchema.parse(document);
  if (doc.kind === "object") return compileSurface(doc, quality);
  if (doc.kind === "character") return compileCharacter(doc, quality);
  if (doc.kind === "vegetation") return compileVegetation(doc, quality);
  return null;
}
export function artifactTransfers(artifact: SurfaceArtifact | null): ArrayBuffer[] {
  if (!artifact) return [];
  const surfaces = artifact.kind === "vegetation" ? artifact.surfaces : [artifact];
  const meshes = surfaces.flatMap((surface) => [
      surface.mesh,
      ...("details" in surface ? (surface.details?.map((detail) => detail.mesh) ?? []) : []),
    ]),
    buffers: ArrayBuffer[] = [];
  for (const mesh of meshes) {
    buffers.push(
      mesh.positions.buffer as ArrayBuffer,
      mesh.normals.buffer as ArrayBuffer,
      mesh.indices.buffer as ArrayBuffer,
    );
    if (mesh.colors) buffers.push(mesh.colors.buffer as ArrayBuffer);
  }
  if (artifact.kind === "character")
    buffers.push(artifact.jointIndices.buffer as ArrayBuffer, artifact.weights.buffer as ArrayBuffer);
  return [...new Set(buffers)];
}
