import {
  add,
  contentKey,
  cross,
  dot,
  type MeshData,
  normalize,
  scale,
  type Vec3,
  type VegetationCrownProduct,
} from "@wrela/model";

import { builder, finish, vertex } from "./botanical-primitives";
import { partitionFoliage } from "./vegetation-clusters";

const TILE = 64,
  GRID = 8,
  PAD = 4;
const clamp = (x: number, a: number, b: number) => Math.max(a, Math.min(b, x));
function alpha(mesh: MeshData, u: number, v: number, layer: number): number {
  const field = mesh.thinCoverage;
  if (!field) return 1;
  const x = clamp(u * field.width - 0.5, 0, field.width - 1),
    y = clamp(v * field.height - 0.5, 0, field.height - 1);
  const x0 = Math.floor(x),
    y0 = Math.floor(y),
    x1 = Math.min(field.width - 1, x0 + 1),
    y1 = Math.min(field.height - 1, y0 + 1);
  const stride = field.format === "coverage-normal" ? 4 : 1;
  const sample = (x: number, y: number) =>
    field.levels[0][(layer * field.width * field.height + y * field.width + x) * stride] / 255;
  return (
    (sample(x0, y0) * (1 - (x - x0)) + sample(x1, y0) * (x - x0)) * (1 - (y - y0)) +
    (sample(x0, y1) * (1 - (x - x0)) + sample(x1, y1) * (x - x0)) * (y - y0)
  );
}
/** Coverage averages in linear space; normals average with covered area, never empty texels. */
export function crownMipChain(base: Uint8Array, width: number, height: number): Uint8Array[] {
  const levels = [base];
  let previous = Float32Array.from(base, (v) => v / 255);
  while (width > 1 || height > 1) {
    const w = Math.max(1, width >> 1),
      h = Math.max(1, height >> 1),
      next = new Float32Array(w * h * 4);
    for (let y = 0; y < h; y++)
      for (let x = 0; x < w; x++) {
        let coverage = 0;
        const n = [0, 0, 0];
        let count = 0;
        for (let dy = 0; dy < Math.min(2, height); dy++)
          for (let dx = 0; dx < Math.min(2, width); dx++) {
            const i = ((y * 2 + dy) * width + x * 2 + dx) * 4,
              a = previous[i];
            coverage += a;
            count++;
            for (let c = 0; c < 3; c++) n[c] += (previous[i + 1 + c] * 2 - 1) * a;
          }
        const i = (y * w + x) * 4;
        next[i] = coverage / count;
        for (let c = 0; c < 3; c++)
          next[i + 1 + c] = coverage ? (n[c] / coverage) * 0.5 + 0.5 : c === 2 ? 1 : 0.5;
      }
    levels.push(Uint8Array.from(next, (v) => Math.round(clamp(v, 0, 1) * 255)));
    previous = next;
    width = w;
    height = h;
  }
  return levels;
}
/** Candidate crown built from source geometry/coverage, not a fixed-light beauty capture.
 * Runtime admission is explicit until the angular, motion and lighting study qualifies it. */
export function compileVegetationCrown(
  mesh: MeshData,
  sourceKey: string,
  windEnvelope: number,
  options: { depthGrid?: 0 | 16 } = {},
): { mesh: MeshData; product: VegetationCrownProduct } | undefined {
  if (!mesh.indices.length || mesh.shoots) return undefined;
  const DEPTH_GRID = options.depthGrid || 1;
  const algorithm = options.depthGrid ? "cluster-coverage-depth-5" : "cluster-coverage-orientation-4";
  const clusters = partitionFoliage(mesh);
  const layers = GRID * GRID * clusters.length;
  const atlas = new Uint8Array(TILE * TILE * layers * 4),
    geometry = builder(),
    uv: number[] = [],
    views: VegetationCrownProduct["views"] = [];
  const positions = mesh.positions,
    normals = mesh.normals,
    colors = mesh.colors;
  const projected = new Float32Array(positions.length);
  for (let view = 0; view < GRID * GRID; view++) {
    const y = 1 - (2 * (view + 0.5)) / (GRID * GRID),
      angle = view * 2.3999632297;
    const direction: Vec3 = [
      Math.cos(angle) * Math.sqrt(1 - y * y),
      y,
      Math.sin(angle) * Math.sqrt(1 - y * y),
    ];
    const right = normalize(cross(Math.abs(y) > 0.95 ? [1, 0, 0] : [0, 1, 0], direction)),
      up = cross(direction, right);
    const firstIndex = geometry.indices.length;
    for (let cluster = 0; cluster < clusters.length; cluster++) {
      const { mesh } = clusters[cluster];
      const layer = view * clusters.length + cluster;
      const center = mesh.bounds.min.map((v, i) => (v + mesh.bounds.max[i]) * 0.5) as Vec3;
      const extent = mesh.bounds.max.map((v, i) => (v - mesh.bounds.min[i]) * 0.5) as Vec3;
      const halfWidth = (Math.max(0.001, dot(right.map(Math.abs) as Vec3, extent)) * TILE) / (TILE - 2 * PAD);
      const halfHeight = (Math.max(0.001, dot(up.map(Math.abs) as Vec3, extent)) * TILE) / (TILE - 2 * PAD);
      const coverage = new Float32Array(TILE * TILE),
        normalSum = new Float32Array(TILE * TILE * 3),
        normalWeight = new Float32Array(TILE * TILE),
        depthSum = new Float32Array(TILE * TILE);
      let tint = 0,
        tintWeight = 0;
      for (let i = 0; i < positions.length; i += 3) {
        const p: Vec3 = [
          positions[i] - center[0],
          positions[i + 1] - center[1],
          positions[i + 2] - center[2],
        ];
        projected[i] = ((dot(p, right) / halfWidth) * 0.5 + 0.5) * TILE;
        projected[i + 1] = ((dot(p, up) / halfHeight) * 0.5 + 0.5) * TILE;
        projected[i + 2] = dot(p, direction);
      }
      for (let triangle = 0; triangle < mesh.indices.length; triangle += 3) {
        const ids = [mesh.indices[triangle], mesh.indices[triangle + 1], mesh.indices[triangle + 2]];
        const [a, b, c] = ids.map((i) => i * 3),
          ax = projected[a],
          ay = projected[a + 1],
          bx = projected[b],
          by = projected[b + 1],
          cx = projected[c],
          cy = projected[c + 1];
        const area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
        if (Math.abs(area) < 1e-9) continue;
        const minX = clamp(Math.floor(Math.min(ax, bx, cx)), 0, TILE - 1),
          maxX = clamp(Math.ceil(Math.max(ax, bx, cx)), 0, TILE - 1);
        const minY = clamp(Math.floor(Math.min(ay, by, cy)), 0, TILE - 1),
          maxY = clamp(Math.ceil(Math.max(ay, by, cy)), 0, TILE - 1);
        for (let py = minY; py <= maxY; py++)
          for (let px = minX; px <= maxX; px++) {
            const x = px + 0.5,
              y = py + 0.5;
            const w1 = ((x - ax) * (cy - ay) - (y - ay) * (cx - ax)) / area,
              w2 = ((bx - ax) * (y - ay) - (by - ay) * (x - ax)) / area,
              w0 = 1 - w1 - w2;
            if (w0 < 0 || w1 < 0 || w2 < 0) continue;
            const weights = [w0, w1, w2],
              field = mesh.thinCoverage;
            const u = field ? ids.reduce((sum, id, i) => sum + field.uv[id * 2] * weights[i], 0) : 0;
            const v = field ? ids.reduce((sum, id, i) => sum + field.uv[id * 2 + 1] * weights[i], 0) : 0;
            const occupied = alpha(mesh, u, v, field?.layer?.[ids[0]] ?? 0);
            if (occupied <= 0) continue;
            const pixel = py * TILE + px;
            coverage[pixel] = 1 - (1 - coverage[pixel]) * (1 - occupied);
            let n = normalize(
              [0, 1, 2].map((axis) =>
                ids.reduce((sum, id, i) => sum + normals[id * 3 + axis] * weights[i], 0),
              ) as Vec3,
            );
            if (dot(n, direction) < 0) n = scale(n, -1);
            const local = [dot(n, right), dot(n, up), dot(n, direction)];
            normalWeight[pixel] += occupied;
            depthSum[pixel] +=
              (projected[a + 2] * w0 + projected[b + 2] * w1 + projected[c + 2] * w2) * occupied;
            for (let axis = 0; axis < 3; axis++) normalSum[pixel * 3 + axis] += local[axis] * occupied;
            const color = colors
              ? ids.reduce(
                  (sum, id, i) =>
                    sum + ((colors[id * 3] + colors[id * 3 + 1] + colors[id * 3 + 2]) / 3) * weights[i],
                  0,
                )
              : 1;
            tint += color * occupied;
            tintWeight += occupied;
          }
      }
      for (let y = 0; y < TILE; y++)
        for (let x = 0; x < TILE; x++) {
          const pixel = y * TILE + x,
            i = (layer * TILE * TILE + y * TILE + x) * 4;
          atlas[i] = Math.round(coverage[pixel] * 255);
          for (let axis = 0; axis < 3; axis++)
            atlas[i + 1 + axis] = Math.round(
              clamp(
                normalWeight[pixel]
                  ? (normalSum[pixel * 3 + axis] / normalWeight[pixel]) * 0.5 + 0.5
                  : axis === 2
                    ? 1
                    : 0.5,
                0,
                1,
              ) * 255,
            );
        }
      // A coarse depth surface preserves parallax within each view's angular
      // domain. Extend occupied depths into padding before interpolation, so
      // uncovered texels cannot pull the surface toward an arbitrary zero plane.
      const depth = new Float32Array(TILE * TILE),
        filled = new Uint8Array(TILE * TILE);
      const queue: number[] = [];
      for (let i = 0; i < depth.length; i++)
        if (normalWeight[i] > 0) {
          depth[i] = depthSum[i] / normalWeight[i];
          filled[i] = 1;
          queue.push(i);
        }
      for (let head = 0; head < queue.length; head++) {
        const i = queue[head],
          x = i % TILE,
          y = Math.floor(i / TILE);
        for (const neighbor of [
          x > 0 ? i - 1 : -1,
          x + 1 < TILE ? i + 1 : -1,
          y > 0 ? i - TILE : -1,
          y + 1 < TILE ? i + TILE : -1,
        ]) {
          if (neighbor >= 0 && !filled[neighbor]) {
            filled[neighbor] = 1;
            depth[neighbor] = depth[i];
            queue.push(neighbor);
          }
        }
      }
      const sampleDepth = (u: number, v: number) => {
        const x = clamp(u * TILE - 0.5, 0, TILE - 1),
          y = clamp(v * TILE - 0.5, 0, TILE - 1);
        const x0 = Math.floor(x),
          y0 = Math.floor(y),
          x1 = Math.min(TILE - 1, x0 + 1),
          y1 = Math.min(TILE - 1, y0 + 1);
        return (
          (depth[y0 * TILE + x0] * (1 - x + x0) + depth[y0 * TILE + x1] * (x - x0)) * (1 - y + y0) +
          (depth[y1 * TILE + x0] * (1 - x + x0) + depth[y1 * TILE + x1] * (x - x0)) * (y - y0)
        );
      };
      const first = geometry.positions.length / 3;
      const color = tintWeight ? tint / tintWeight : 1;
      for (let y = 0; y <= DEPTH_GRID; y++)
        for (let x = 0; x <= DEPTH_GRID; x++) {
          const u = x / DEPTH_GRID,
            v = y / DEPTH_GRID;
          vertex(
            geometry,
            add(
              center,
              add(
                add(scale(right, (u * 2 - 1) * halfWidth), scale(up, (v * 2 - 1) * halfHeight)),
                scale(direction, options.depthGrid ? sampleDepth(u, v) : 0),
              ),
            ),
            direction,
            [color, color, color],
            `${sourceKey}/crown/${cluster}`,
          );
          uv.push(u, v);
        }
      for (let y = 0; y < DEPTH_GRID; y++)
        for (let x = 0; x < DEPTH_GRID; x++) {
          const a = first + y * (DEPTH_GRID + 1) + x,
            b = a + 1,
            c = a + DEPTH_GRID + 1,
            d = c + 1;
          geometry.indices.push(a, b, c, b, d, c);
        }
    }
    views.push({ direction, firstIndex, indexCount: geometry.indices.length - firstIndex });
  }
  const chains = Array.from({ length: layers }, (_, view) =>
    crownMipChain(atlas.slice(view * TILE * TILE * 4, (view + 1) * TILE * TILE * 4), TILE, TILE),
  );
  const levels = chains[0].map((level, mip) => {
    const combined = new Uint8Array(level.length * chains.length);
    chains.forEach((chain, view) => {
      combined.set(chain[mip], view * level.length);
    });
    return combined;
  });
  const layer = Uint16Array.from({ length: layers * (DEPTH_GRID + 1) ** 2 }, (_, i) =>
    Math.floor(i / (DEPTH_GRID + 1) ** 2),
  );
  const key = contentKey({
    sourceKey,
    algorithm,
    views: 64,
    clusters: clusters.length,
    size: TILE,
    depthGrid: DEPTH_GRID,
  });
  const coordinates = new Float32Array(uv);
  const result: MeshData = {
    ...finish(geometry),
    thinCoverage: {
      version: 1,
      format: "coverage-normal",
      key,
      width: TILE,
      height: TILE,
      layers,
      layer,
      levels,
      uv: coordinates,
    },
  };
  const byteLength =
    result.positions.byteLength +
    result.normals.byteLength +
    (result.colors?.byteLength ?? 0) +
    result.indices.byteLength +
    coordinates.byteLength +
    layer.byteLength +
    levels.reduce((sum, level) => sum + level.byteLength, 0);
  return {
    mesh: result,
    product: {
      version: 1,
      kind: "multiview-crown",
      key,
      sourceKey,
      algorithmVersion: algorithm,
      fallback: "source-mesh",
      byteLength,
      windEnvelope,
      sourceOrgans: clusters.flatMap((cluster) => cluster.sourceOrgans),
      clusters: clusters.map((cluster) => ({
        bounds: cluster.mesh.bounds,
        sourceOrgans: cluster.sourceOrgans,
      })),
      views,
      qualification: {
        status: "candidate",
        maximumPixels: 24,
        maxWind: 0,
        evidence:
          "Requires held-out angular coverage, radiance, shadow and temporal qualification; source is the compiled mesh and its geometric coverage.",
      },
    },
  };
}

/** Explicit background/research compilation. Unknown candidates keep the source fallback. */
export function withVegetationCrowns(
  plant: import("@wrela/model").CompiledVegetation,
  options: { depthGrid?: 0 | 16 } = {},
): import("@wrela/model").CompiledVegetation {
  return {
    ...plant,
    surfaces: plant.surfaces.map((surface) => {
      if (!surface.id.endsWith("-foliage")) return surface;
      const crown = compileVegetationCrown(surface.mesh, surface.key, plant.maxDisplacement, options);
      if (!crown) return surface;
      return {
        ...surface,
        details: [
          ...(surface.details ?? []).filter((detail) => !detail.vegetation),
          {
            label: "multiview-crown",
            mesh: crown.mesh,
            vegetation: crown.product,
            maxProjectedDiameter: crown.product.qualification.maximumPixels,
            maxError: null,
          },
        ],
      };
    }),
  };
}
