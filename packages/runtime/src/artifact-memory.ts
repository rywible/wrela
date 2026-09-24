import { type MeshData, ownedBuffers, type SurfaceArtifact } from "@wrela/model";
export function artifactBytes(artifacts: Iterable<SurfaceArtifact>): number {
  const uniqueArtifacts = [...new Set(artifacts)];
  const buffers = new Set<ArrayBufferLike>(uniqueArtifacts.flatMap(ownedBuffers)),
    meshes = new Set<MeshData>();
  const metadataArrays = new Set<object>();
  let metadata = 0;
  for (const artifact of uniqueArtifacts) {
    const surfaces = artifact.kind === "vegetation" ? artifact.surfaces : [artifact];
    for (const mesh of surfaces.flatMap((surface) => [
      surface.mesh,
      ...("details" in surface ? (surface.details?.map((detail) => detail.mesh) ?? []) : []),
      ...(surface.kind === "character" ? (surface.creatureDetails?.map((detail) => detail.mesh) ?? []) : []),
      ...(surface.kind === "character"
        ? (surface.creatureGroom?.details.map((detail) => detail.mesh) ?? [])
        : []),
      ...("renderProducts" in surface
        ? (surface.renderProducts?.flatMap((product) =>
            product.kind === "parametric-mesh" ? [product.mesh] : [],
          ) ?? [])
        : []),
    ])) {
      if (meshes.has(mesh)) continue;
      meshes.add(mesh);
      if (mesh.shoots) {
        if (!metadataArrays.has(mesh.shoots.sourceIds)) {
          metadataArrays.add(mesh.shoots.sourceIds);
          metadata += mesh.shoots.sourceIds.reduce((n, id) => n + id.length * 2 + 8, 0);
        }
      }
      if (mesh.sourceIds && !metadataArrays.has(mesh.sourceIds)) {
        metadataArrays.add(mesh.sourceIds);
        metadata +=
          mesh.sourceIds.length * 8 +
          [...new Set(mesh.sourceIds)].reduce((bytes, id) => bytes + id.length * 2, 0);
      }
      if (mesh.materialGroups && !metadataArrays.has(mesh.materialGroups)) {
        metadataArrays.add(mesh.materialGroups);
        metadata += mesh.materialGroups.reduce((bytes, group) => bytes + 32 + group.material.length * 2, 0);
      }
    }
    for (const surface of surfaces) {
      if ("details" in surface)
        for (const detail of surface.details ?? [])
          if (detail.vegetation) metadata += JSON.stringify(detail.vegetation).length * 2;
      if ("reliefAppearance" in surface && surface.reliefAppearance)
        metadata += JSON.stringify(surface.reliefAppearance).length * 2;
      if ("renderProducts" in surface && surface.renderProducts) {
        metadata +=
          JSON.stringify(
            surface.renderProducts.map((product) =>
              product.kind === "parametric-mesh" ? { ...product, mesh: undefined } : product,
            ),
          ).length * 2;
      }
      if ("opaqueVisibility" in surface && surface.opaqueVisibility)
        metadata += JSON.stringify(surface.opaqueVisibility).length * 2;
    }
    if (artifact.kind === "character") {
      metadata +=
        JSON.stringify({
          joints: artifact.joints,
          motions: artifact.motions,
          performance: artifact.performance,
          creature: artifact.creature,
          coordinates: artifact.creatureCoordinates,
          regions: artifact.creatureRegions,
          anchors: artifact.creatureAnchors,
          materials: artifact.creatureMaterials,
          guides: artifact.creatureGroom?.guides,
        }).length * 2;
    }
  }
  return [...buffers].reduce((sum, buffer) => sum + buffer.byteLength, metadata);
}
