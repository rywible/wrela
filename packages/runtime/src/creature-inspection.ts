import {
  type CompiledCharacter,
  type EvaluatedScene,
  identityColor,
  type MeshData,
  type RenderSurface,
  type Vec3,
} from "@wrela/model";

import type { SceneHit } from "./picking";

type Source = NonNullable<RenderSurface["creatureInspection"]>;
const sourceCache = new WeakMap<CompiledCharacter, Source>();
const regionMeshes = new WeakMap<MeshData, { source: Source["regions"]; mesh: MeshData }>();
const regionEncoded = new WeakSet<MeshData>();

/** Reconstruct exact detail attribution from persistent guide roots instead of borrowing another LOD's vertex IDs. */
export function creatureInspectionSource(artifact: CompiledCharacter): Source | undefined {
  if (!artifact.creature) return;
  const cached = sourceCache.get(artifact);
  if (cached) return cached;
  const count = artifact.mesh.positions.length / 3;
  const bodyCount = artifact.creatureBodyVertexCount ?? count;
  const regions: Source["regions"] = Array(count).fill(null);
  const coordinates: Source["coordinates"] = Array(count).fill(null);
  for (let i = 0; i < Math.min(count, bodyCount); i++) {
    regions[i] = artifact.creatureRegions?.[i] ?? null;
    coordinates[i] = artifact.creatureCoordinates?.[i] ?? null;
  }
  const detail = artifact.creatureGroom?.details.find(
    (value) => value.label === artifact.creatureGroomDetail,
  );
  if (detail && bodyCount + detail.vertexGuideIndices.length === count)
    for (let i = 0; i < detail.vertexGuideIndices.length; i++) {
      const root = artifact.creatureGroom?.guides[detail.vertexGuideIndices[i]]?.root;
      regions[bodyCount + i] = root?.region ?? null;
      coordinates[bodyCount + i] = root ?? null;
    }
  const source: Source = {
    regions,
    coordinates,
    ...(detail && bodyCount + detail.vertexGuideIndices.length === count
      ? { groomVertexStart: bodyCount }
      : {}),
    materialId: artifact.material,
    albedoColors: artifact.mesh.colors,
  };
  sourceCache.set(artifact, source);
  return source;
}

/** Review-only color buffer; positions, topology, bindings, dynamics and source records remain untouched. */
function regionMesh(mesh: MeshData, source: Source): MeshData {
  if (regionEncoded.has(mesh)) return mesh;
  const previous = regionMeshes.get(mesh);
  if (previous?.source === source.regions) return previous.mesh;
  const colors = new Float32Array(mesh.positions.length);
  for (let i = 0; i < colors.length / 3; i++)
    colors.set(source.regions[i] ? identityColor(`region:${source.regions[i]}`) : [0.12, 0.12, 0.12], i * 3);
  const result = { ...mesh, colors };
  regionMeshes.set(mesh, { source: source.regions, mesh: result });
  regionEncoded.add(result);
  return result;
}

/** Hides only triangles explicitly compiled from groom roots. Material names never decide anatomical visibility. */
export function applyCreatureInspection(
  scene: EvaluatedScene,
  options: { hideGroom?: boolean; hideSourceIds?: readonly string[] } = {},
): EvaluatedScene {
  if (!options.hideGroom && !options.hideSourceIds?.length && scene.mode !== "regions") return scene;
  const hidden = new Set(options.hideSourceIds ?? []);
  return {
    ...scene,
    surfaces: scene.surfaces.flatMap((surface) => {
      const source = surface.creatureInspection;
      if (!source) return [surface];
      const viewed =
        scene.mode === "regions" ? { ...surface, mesh: regionMesh(surface.mesh, source) } : surface;
      if ((!options.hideGroom || source.groomVertexStart === undefined) && !hidden.size) return [viewed];
      const first = surface.drawRange?.start ?? 0;
      const end = Math.min(
        surface.mesh.indices.length,
        first + (surface.drawRange?.count ?? surface.mesh.indices.length),
      );
      const ranges: { start: number; count: number }[] = [];
      for (let offset = first; offset < end; offset += 3) {
        if (
          [0, 1, 2].some((corner) => {
            const vertex = surface.mesh.indices[offset + corner];
            return (
              (options.hideGroom &&
                source.groomVertexStart !== undefined &&
                vertex >= source.groomVertexStart) ||
              hidden.has(surface.mesh.sourceIds?.[vertex] ?? "")
            );
          })
        )
          continue;
        const previous = ranges.at(-1);
        if (previous && previous.start + previous.count === offset) previous.count += 3;
        else ranges.push({ start: offset, count: 3 });
      }
      if (ranges.length === 1 && ranges[0].start === first && ranges[0].count === end - first)
        return [viewed];
      return ranges.map((range) => ({
        ...viewed,
        id: `${surface.id}/inspect-range/${range.start}`,
        drawRange: range,
      }));
    }),
  };
}

/** Ground the picked visible triangle back to immutable authoring coordinates and explicit response factors. */
export function inspectCreatureMaterialAtHit(scene: EvaluatedScene, hit: SceneHit) {
  const surface = scene.surfaces.find((value) => value.id === hit.surfaceId);
  if (!surface || hit.triangle < 0) return null;
  const triangle = hit.triangle * 3;
  if (triangle + 2 >= surface.mesh.indices.length) return null;
  const vertices = Array.from(surface.mesh.indices.slice(triangle, triangle + 3));
  const source = surface.creatureInspection;
  const regions = new Map<string, number>();
  const restPosition: Vec3 = [0, 0, 0],
    vertexColor: Vec3 = [0, 0, 0];
  for (let corner = 0; corner < 3; corner++) {
    const vertex = vertices[corner],
      weight = hit.barycentric[corner];
    const region = source?.regions[vertex];
    if (region) regions.set(region, (regions.get(region) ?? 0) + weight);
    for (let axis = 0; axis < 3; axis++) {
      restPosition[axis] += surface.mesh.positions[vertex * 3 + axis] * weight;
      vertexColor[axis] += ((source?.albedoColors ?? surface.mesh.colors)?.[vertex * 3 + axis] ?? 1) * weight;
    }
  }
  const material = surface.material;
  return {
    documentId: surface.source,
    instanceId: surface.instanceId,
    surfaceId: surface.id,
    materialId: source?.materialId ?? null,
    nodeId: hit.nodeId ?? null,
    restPosition,
    coordinateSpace: "character-rest-metres" as const,
    regions: [...regions].map(([id, weight]) => ({ id, weight })).sort((a, b) => b.weight - a.weight),
    anchors: vertices.map((vertex, corner) => ({
      coordinate: source?.coordinates[vertex] ?? null,
      weight: hit.barycentric[corner],
    })),
    optical: {
      family: material.creature?.family ?? "hard",
      roughness: material.roughness,
      metallic: material.metallic,
      thicknessMetres: material.creature?.thickness ?? 0.005,
      thicknessSource: material.creature?.thickness === undefined ? "shader-default" : "authored-material",
      fiberDirectionRest: material.creature?.fiberDirection ?? [0, 1, 0],
      frame: material.creature?.frame ?? null,
      subsurface: material.creature?.subsurface,
      transmission: material.creature?.transmission,
      sheen: material.creature?.sheen,
      clearcoat: material.creature?.clearcoat,
    },
    albedo: {
      materialColor: [...material.color],
      materialSecondary: [...material.secondary],
      vertexColor,
      minimumBeforeLayers: material.color.map(
        (value, axis) => Math.min(value, material.secondary[axis]) * vertexColor[axis],
      ),
      maximumBeforeLayers: material.color.map(
        (value, axis) => Math.max(value, material.secondary[axis]) * vertexColor[axis],
      ),
      note: "Linear reflectance factors before procedural pattern selection and material layers; independent of lighting and exposure.",
    },
    channels: {
      thickness: "gray = clamp(metres / 0.02, 0, 1)",
      "fiber-direction": "RGB = deformed tangent * 0.5 + 0.5",
      regions:
        "Stable region colors interpolated at triangle boundaries; inspect weighted source attribution rather than decoding boundary pixels.",
    },
  };
}
