import {
  type Bounds,
  type CompiledSurface,
  contentKey,
  type MeshData,
  type Vec3,
  type VegetationDefinition,
} from "@wrela/model";

import { botanicalRandom } from "./botanical-branch";
import { builder, finish } from "./botanical-primitives";
import { coniferShootRecipe } from "./conifer-mesh";
import { SHOOT_VARIANTS, shootFrame } from "./needle-shoot";
import { pineArchitecture } from "./pine-architecture";
import { appendShootCoverage, compileShootCoverage } from "./shoot-coverage";
import { shootSkyVisibility } from "./shoot-visibility";

/** Exact sharing for straight authored shoots. Curved local edits retain the
 * expanded realization; they are never silently straightened to force reuse. */
export function compileShootInstances(doc: VegetationDefinition): CompiledSurface[] | undefined {
  if (!doc.botanical?.conifer?.architecture || doc.botanical.development) return;
  const source = doc.botanical,
    structure = pineArchitecture(doc),
    recipe = coniferShootRecipe(doc);
  const branches = structure.branches.filter(
    (b) =>
      b.cohort &&
      !b.bare &&
      !b.broken &&
      botanicalRandom(doc.seed, `${b.id}/foliage`) < source.canopy.density,
  );
  if (!branches.length || structure.truncated) return;
  for (const branch of branches) {
    const frame = shootFrame(branch, 0),
      points = branch.points ?? [];
    for (let i = 0; i < points.length; i++) {
      const point = points[i],
        t = i / (points.length - 1);
      if (Math.hypot(...point.map((v, a) => v - branch.start[a] - frame.y[a] * frame.length * t)) > 1e-5)
        return;
    }
  }
  const atlas = compileShootCoverage(recipe),
    byId = new Map(structure.branches.map((b) => [b.id, b]));
  const groups = atlas.shoots.map(() => ({
    transforms: [] as number[],
    anchors: [] as number[],
    motion: [] as number[],
    sourceIds: [] as string[],
    skyVisibility: [] as number[],
  }));
  const sky = shootSkyVisibility(
    branches.map((branch) => ({
      center: branch.start.map((v, a) => (v + branch.end[a]) / 2) as Vec3,
      projectedArea:
        recipe.count *
        (1 - recipe.loss) *
        ((branch.cohort?.age ?? 0) > 0 ? 0.78 : 1) *
        recipe.needleLength *
        recipe.needleWidth *
        0.5,
    })),
  );
  for (let index = 0; index < branches.length; index++) {
    const branch = branches[index];
    const variant = Math.floor(botanicalRandom(doc.seed, `${branch.id}/needle-module`) * SHOOT_VARIANTS);
    const group = groups[((branch.cohort?.age ?? 0) > 0 ? SHOOT_VARIANTS : 0) + variant];
    const frame = shootFrame(branch, 0),
      yScale = frame.length / recipe.length;
    group.transforms.push(
      ...frame.x,
      0,
      ...frame.y.map((v) => v * yScale),
      0,
      ...frame.z,
      0,
      ...frame.center,
      1,
    );
    let primary = branch;
    while (primary.parent) {
      const parent = byId.get(primary.parent);
      if (!parent || parent.level === 0) break;
      primary = parent;
    }
    group.anchors.push(...primary.start, botanicalRandom(doc.seed, `${primary.id}/motion`) * Math.PI * 2);
    group.motion.push(
      source.motion.branchSway,
      source.motion.leafFlutter,
      botanicalRandom(doc.seed, `${branch.id}/shoot-flutter`) * Math.PI * 2,
      primary.radius,
    );
    group.sourceIds.push(`${doc.id}/${branch.id}`);
    group.skyVisibility.push(source.conifer?.architecture?.canopyVisibility === false ? 1 : sky[index]);
  }
  const surfaces: CompiledSurface[] = [];
  for (let groupIndex = 0; groupIndex < groups.length; groupIndex++) {
    const data = groups[groupIndex];
    if (!data.sourceIds.length) continue;
    const canonical = atlas.shoots[groupIndex],
      proxy = builder(),
      uv: number[] = [],
      layers: number[] = [];
    const straight = {
      id: "template",
      parent: null,
      level: 3,
      start: [0, 0, 0] as Vec3,
      end: [0, recipe.length, 0] as Vec3,
      bend: [0, recipe.length / 2, 0] as Vec3,
      radius: 0.001,
      bare: false,
      broken: false,
    };
    // shootFrame chooses X=-Z for this vertical source; undo that basis so the
    // stored template is exactly in canonical X/Y/Z, like the explicit reference.
    appendShootCoverage(proxy, uv, layers, atlas, groupIndex, straight, "template", 1);
    for (let i = 0; i < proxy.positions.length; i += 3) {
      const x = proxy.positions[i],
        nx = proxy.normals[i];
      proxy.positions[i] = -proxy.positions[i + 2];
      proxy.positions[i + 2] = x;
      proxy.normals[i] = -proxy.normals[i + 2];
      proxy.normals[i + 2] = nx;
    }
    const coverage = {
      ...atlas.coverage,
      key: `${atlas.key}-${groupIndex}`,
      layers: 3,
      levels: atlas.coverage.levels.map((level) => {
        const bytes = level.length / atlas.shoots.length;
        return level.subarray(groupIndex * bytes, (groupIndex + 1) * bytes);
      }),
      uv: new Float32Array(uv),
      layer: Uint16Array.from(layers, (value) => value % 3),
    };
    const proxyMesh: MeshData = { ...finish(proxy), thinCoverage: coverage };
    const templateBounds: Bounds = {
      min: canonical.mesh.bounds.min.map((v, a) => Math.min(v, proxyMesh.bounds.min[a])) as Vec3,
      max: canonical.mesh.bounds.max.map((v, a) => Math.max(v, proxyMesh.bounds.max[a])) as Vec3,
    };
    const shoots: NonNullable<MeshData["shoots"]> = {
      transforms: new Float32Array(data.transforms),
      anchors: new Float32Array(data.anchors),
      motion: new Float32Array(data.motion),
      skyVisibility: new Float32Array(data.skyVisibility),
      sourceIds: data.sourceIds,
      templateBounds,
    };
    const bounds: Bounds = { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] };
    for (let i = 0; i < data.sourceIds.length; i++)
      for (let corner = 0; corner < 8; corner++) {
        const p = [0, 1, 2].map((a) => templateBounds[corner & (1 << a) ? "max" : "min"][a]);
        for (let a = 0; a < 3; a++) {
          const m = i * 16,
            v =
              data.transforms[m + a] * p[0] +
              data.transforms[m + 4 + a] * p[1] +
              data.transforms[m + 8 + a] * p[2] +
              data.transforms[m + 12 + a];
          bounds.min[a] = Math.min(bounds.min[a], v);
          bounds.max[a] = Math.max(bounds.max[a], v);
        }
      }
    const key = contentKey({
      source: canonical.key,
      coverage: atlas.key,
      transforms: data.transforms,
      motion: data.motion,
      anchors: data.anchors,
      skyVisibility: data.skyVisibility,
    });
    surfaces.push({
      kind: "surface",
      id: `${doc.id}-shoot-${groupIndex}-foliage`,
      key,
      material: doc.material,
      diagnostics: [],
      mesh: { ...canonical.mesh, bounds, shoots },
      details: [
        {
          label: "projected-shoot",
          mesh: { ...proxyMesh, bounds, shoots },
          maxProjectedDiameter: 96,
          maxError: null,
        },
      ],
    });
  }
  return surfaces;
}
