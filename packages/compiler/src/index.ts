import { ownedBuffers, type SurfaceArtifact } from "@wrela/model";

export * from "./atmosphere";
export * from "./branch-visibility";
export * from "./cloud-transport";
export { compileDocument, compilerKey } from "./compile";
export * from "./cooked";
export * from "./creature";
export * from "./creature-attachments";
export * from "./creature-features";
export * from "./creature-projection";
export * from "./field";
export * from "./finite-sun";
export * from "./groom";
export * from "./groom-appearance";
export * from "./groom-creature";
export * from "./ir";
export * from "./occluder-facts";
export * from "./periodic-material";
export * from "./phase";
export * from "./primitive";
export * from "./products";
export * from "./render-products";
export * from "./sky-visibility";
export * from "./surface";
export * from "./surface-relief";
export * from "./surface-relief-pattern";
export * from "./surface-relief-sampling";
export * from "./terrain";
export * from "./vegetation";
export { BUNDLE_COMPILER_SOURCE, COOK_FORMAT_VERSION, PRODUCT_VERSIONS } from "./versions";
/** Transfer and residency use the same owned-buffer contract, including new nested products. */
export function artifactTransfers(artifact: SurfaceArtifact | null): ArrayBuffer[] {
  return artifact ? ownedBuffers(artifact) : [];
}

export * from "./assembly";
export { assemblyModuleObstructs } from "./assembly-clearance";
export * from "./atmosphere-authoring";
export * from "./botanical-analysis";
export * from "./botanical-growth";
export type { GrowthCompileProvider, GrowthCompileResult } from "./botanical-growth-request";
export {
  prepareVegetationGrowth,
  validateVegetationGrowth,
  vegetationGrowthDefinitionKey,
  vegetationGrowthDocument,
} from "./botanical-growth-request";
export * from "./botanical-growth-structure";
export * from "./geology";
export * from "./indirect-probes";
export * from "./indirect-query";
export * from "./indirect-reference";
export * from "./indirect-surface-cache";
export { indirectTriangleCacheSteps, triangleProbeVisibility } from "./indirect-triangle-cache";
export * from "./indirect-visibility";
export { indirectVisibilityCellSteps } from "./indirect-visibility";
export * from "./material-lattice";
export { compileVegetationCrown, withVegetationCrowns } from "./vegetation-crown";
export {
  type RiverDiagnostic,
  resolveWaterWaves,
  reviewRiverChannel,
  riverWaterMesh,
  waterMeshSpacing,
} from "./water";
export * from "./water-domain";
export * from "./water-effects";
export * from "./water-reflections";
export * from "./water-spectrum";
export { compileRadianceLightingSteps, radianceReceiverSamples, radianceReceiverBinding, radianceReceiverMixture, type RadianceLightingProduct } from "./radiance-lighting";

export * from "./radiance-cooked";

export { yieldCompilation } from "./cooperative";
