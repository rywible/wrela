import { contentKey, type Document, documentSchema, type Quality, type SurfaceArtifact } from "@wrela/model";

import { COMPILER_VERSION, compileCharacter, compileSurface, surfaceGeometrySource } from "./surface";
import { compileVegetation, vegetationGeometrySource } from "./vegetation";

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
