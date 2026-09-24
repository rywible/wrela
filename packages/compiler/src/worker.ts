import {
  type Document,
  type GrowthEvent,
  type PersistentVegetationGrowth,
  type Quality,
  type TerrainDefinition,
  terrainSchema,
  type VegetationDefinition,
  vegetationSchema,
} from "@wrela/model";

import { prepareVegetationGrowth, vegetationGrowthDocument } from "./botanical-growth-request";
import { compiledBotanicalGrowth, rememberBotanicalGrowth } from "./botanical-growth-structure";
import { artifactTransfers, compileDocument, generateTerrainPatch, type TerrainStitch } from "./index";
import { BUNDLE_COMPILER_SOURCE } from "./versions";

type GrowthRequest = {
  id: string;
  growth: {
    document: VegetationDefinition;
    instanceId: string;
    steps: number;
    previous?: PersistentVegetationGrowth;
    events: GrowthEvent[];
  };
  quality: Quality;
};
type InspectGrowthRequest = { id: string; inspectGrowth: VegetationDefinition };
type DocumentRequest = { id: string; document: Document; quality: Quality };
type TerrainRequest = {
  id: string;
  terrain: TerrainDefinition;
  patch: { x: number; z: number; size: number; resolution: number; stitch?: TerrainStitch };
};
/** One job per worker at a time. Hosts own queue admission and discard stale
 * replies by dependency key. Validated document limits bound individual jobs. */
const scope = globalThis as unknown as {
  onmessage: (
    event: MessageEvent<DocumentRequest | TerrainRequest | GrowthRequest | InspectGrowthRequest>,
  ) => void;
  postMessage: (message: unknown, transfers: ArrayBuffer[]) => void;
};
scope.onmessage = ({ data }) => {
  const id = data?.id;
  try {
    if (!data || typeof id !== "string" || id.length > 128)
      throw new Error("Invalid compilation request identity");
    if ("inspectGrowth" in data) {
      const document = vegetationSchema.parse(data.inspectGrowth);
      scope.postMessage(
        {
          id,
          result: compiledBotanicalGrowth(document),
          error: null,
          compilerSource: BUNDLE_COMPILER_SOURCE,
        },
        [],
      );
    } else if ("growth" in data) {
      const request = data.growth;
      const source = vegetationSchema.parse(request.document);
      if (typeof request.instanceId !== "string" || request.instanceId.length > 512)
        throw Error("Invalid growth instance identity");
      const growth = prepareVegetationGrowth(source, request.steps, request.previous, request.events);
      const document = vegetationGrowthDocument(source, growth, request.instanceId);
      rememberBotanicalGrowth(document, growth.checkpoint);
      const artifact = structuredClone(compileDocument(document, data.quality));
      if (artifact?.kind !== "vegetation") throw Error("Expected vegetation compilation");
      scope.postMessage(
        { id, result: { growth, document, artifact }, error: null, compilerSource: BUNDLE_COMPILER_SOURCE },
        artifactTransfers(artifact),
      );
    } else if ("terrain" in data) {
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
