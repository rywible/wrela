import { type Document, type Quality, type TerrainDefinition, terrainSchema } from "@wrela/model";
import { artifactTransfers, compileDocument, generateTerrainPatch, type TerrainStitch } from "./index";
import { BUNDLE_COMPILER_SOURCE } from "./versions";

type DocumentRequest = { id: string; document: Document; quality: Quality };
type TerrainRequest = {
  id: string;
  terrain: TerrainDefinition;
  patch: { x: number; z: number; size: number; resolution: number; stitch?: TerrainStitch };
};
/** One job per worker at a time. Hosts own queue admission and discard stale
 * replies by dependency key. Validated document limits bound individual jobs. */
const scope = globalThis as unknown as {
  onmessage: (event: MessageEvent<DocumentRequest | TerrainRequest>) => void;
  postMessage: (message: unknown, transfers: ArrayBuffer[]) => void;
};
scope.onmessage = ({ data }) => {
  const id = data?.id;
  try {
    if (!data || typeof id !== "string" || id.length > 128)
      throw new Error("Invalid compilation request identity");
    if ("terrain" in data) {
      const terrain = terrainSchema.parse(data.terrain),
        patch = data.patch;
      if (!patch || ![patch.x, patch.z, patch.size, patch.resolution].every(Number.isFinite))
        throw new Error("Invalid terrain patch request");
      if (
        patch.stitch &&
        Object.entries(patch.stitch).some(
          ([edge, value]) => !["north", "east", "south", "west"].includes(edge) || typeof value !== "boolean",
        )
      )
        throw new Error("Invalid terrain stitch configuration");
      const mesh = generateTerrainPatch(
        terrain,
        patch.x,
        patch.z,
        patch.size,
        patch.resolution,
        patch.stitch,
      );
      scope.postMessage({ id, mesh, error: null, compilerSource: BUNDLE_COMPILER_SOURCE }, [
        mesh.positions.buffer,
        mesh.normals.buffer,
        mesh.indices.buffer,
      ] as ArrayBuffer[]);
    } else {
      // Published buffers are transferred. Retain the immutable compiler products
      // for later motion/material edits rather than detaching the cache itself.
      const artifact = structuredClone(compileDocument(data.document, data.quality));
      scope.postMessage(
        { id, artifact, error: null, compilerSource: BUNDLE_COMPILER_SOURCE },
        artifactTransfers(artifact),
      );
    }
  } catch (error) {
    scope.postMessage(
      { id, artifact: null, mesh: null, error: error instanceof Error ? error.message : String(error) },
      [],
    );
  }
};
