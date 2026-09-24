import { add, contentKey, dot, normalize, scale, type ThinCoverage, type Vec3 } from "@wrela/model";

import type { BotanicalBranch } from "./botanical-branch";
import { type Builder, vertex } from "./botanical-primitives";
import { ProductCache } from "./cache";
import {
  compileNeedleShoot,
  type NeedleShoot,
  SHOOT_COHORTS,
  SHOOT_VARIANTS,
  SHOOT_VERSION,
  type ShootRecipe,
  shootFrame,
  shootPoint,
} from "./needle-shoot";
import { crownMipChain } from "./vegetation-crown";

const WIDTH = 128,
  HEIGHT = 256,
  SAMPLES = 2;
export type ShootPlane = { min: [number, number]; max: [number, number]; angle: number };
export type ShootCoverage = {
  key: string;
  shoots: NeedleShoot[];
  planes: ShootPlane[];
  coverage: Omit<ThinCoverage, "uv" | "layer">;
};
const products = new ProductCache<ShootCoverage>(32 * 1024 * 1024, 8);

/** Rasterize only the 3D needles assigned to this plane. The texture never invents
 * needles or applies a density boost. Normals use the covered samples' orientation. */
function project(shoot: NeedleShoot, plane: number): { plane: ShootPlane; levels: Uint8Array[] } {
  const angle = (plane * Math.PI) / 3;
  const x: Vec3 = [Math.cos(angle), 0, Math.sin(angle)];
  const z: Vec3 = [-Math.sin(angle), 0, Math.cos(angle)];
  const { positions, normals, indices } = shoot.mesh;
  const ranges = shoot.needleRanges.filter((range) => range.plane === plane);
  const min: [number, number] = [Infinity, Infinity],
    max: [number, number] = [-Infinity, -Infinity];
  for (const range of ranges)
    for (let i = range.firstIndex; i < range.firstIndex + range.count; i++) {
      const v = indices[i] * 3,
        p: Vec3 = [positions[v], positions[v + 1], positions[v + 2]];
      min[0] = Math.min(min[0], dot(p, x));
      max[0] = Math.max(max[0], dot(p, x));
      min[1] = Math.min(min[1], p[1]);
      max[1] = Math.max(max[1], p[1]);
    }
  if (!ranges.length) {
    min[0] = -shoot.recipe.needleLength;
    min[1] = 0;
    max[0] = shoot.recipe.needleLength;
    max[1] = shoot.recipe.length;
  }
  for (let a = 0; a < 2; a++) {
    const guard = (Math.max(1e-5, max[a] - min[a]) * 2) / ((a ? HEIGHT : WIDTH) - 4);
    min[a] -= guard;
    max[a] += guard;
  }
  const w = WIDTH * SAMPLES,
    h = HEIGHT * SAMPLES;
  const depth = new Float32Array(w * h).fill(-Infinity),
    field = new Float32Array(w * h * 3);
  for (const range of ranges)
    for (let triangle = range.firstIndex; triangle < range.firstIndex + range.count; triangle += 3) {
      const verts = [0, 1, 2].map((c) => {
        const index = indices[triangle + c] * 3;
        const p: Vec3 = [positions[index], positions[index + 1], positions[index + 2]];
        const normal: Vec3 = [normals[index], normals[index + 1], normals[index + 2]];
        return {
          x: ((dot(p, x) - min[0]) / (max[0] - min[0])) * w,
          y: ((p[1] - min[1]) / (max[1] - min[1])) * h,
          z: dot(p, z),
          n: [dot(normal, x), normal[1], dot(normal, z)] as Vec3,
        };
      });
      const [a, b, c] = verts;
      const area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
      if (Math.abs(area) < 1e-10) continue;
      for (
        let py = Math.max(0, Math.floor(Math.min(a.y, b.y, c.y)));
        py <= Math.min(h - 1, Math.ceil(Math.max(a.y, b.y, c.y)));
        py++
      )
        for (
          let px = Math.max(0, Math.floor(Math.min(a.x, b.x, c.x)));
          px <= Math.min(w - 1, Math.ceil(Math.max(a.x, b.x, c.x)));
          px++
        ) {
          const u = ((px + 0.5 - a.x) * (c.y - a.y) - (py + 0.5 - a.y) * (c.x - a.x)) / area;
          const v = ((b.x - a.x) * (py + 0.5 - a.y) - (b.y - a.y) * (px + 0.5 - a.x)) / area;
          const t = 1 - u - v;
          if (u < 0 || v < 0 || t < 0) continue;
          const index = py * w + px,
            d = a.z * t + b.z * u + c.z * v;
          if (d <= depth[index]) continue;
          depth[index] = d;
          const normal = normalize(
            a.n.map((value, axis) => value * t + b.n[axis] * u + c.n[axis] * v) as Vec3,
          );
          field.set([normal[0], normal[1], Math.abs(normal[2])], index * 3);
        }
    }
  const base = new Uint8Array(WIDTH * HEIGHT * 4);
  for (let y = 0; y < HEIGHT; y++)
    for (let x = 0; x < WIDTH; x++) {
      let count = 0;
      const sum = [0, 0, 0];
      for (let dy = 0; dy < SAMPLES; dy++)
        for (let dx = 0; dx < SAMPLES; dx++) {
          const index = (y * SAMPLES + dy) * w + x * SAMPLES + dx;
          if (depth[index] === -Infinity) continue;
          count++;
          for (let c = 0; c < 3; c++) sum[c] += field[index * 3 + c];
        }
      const index = (y * WIDTH + x) * 4;
      base[index] = Math.round((count * 255) / (SAMPLES * SAMPLES));
      for (let c = 0; c < 3; c++)
        base[index + 1 + c] = Math.round((count ? (sum[c] / count) * 0.5 + 0.5 : c === 2 ? 1 : 0.5) * 255);
    }
  return { plane: { min, max, angle }, levels: crownMipChain(base, WIDTH, HEIGHT) };
}

export function compileShootCoverage(recipe: ShootRecipe): ShootCoverage {
  const key = contentKey({ algorithm: "projected-paired-shoot-1", source: SHOOT_VERSION, recipe });
  const cached = products.get(key);
  if (cached) return cached;
  const shoots: NeedleShoot[] = [],
    planes: ShootPlane[] = [],
    tiles: Uint8Array[][] = [];
  for (let cohort = 0; cohort < SHOOT_COHORTS; cohort++)
    for (let variant = 0; variant < SHOOT_VARIANTS; variant++) {
      const shoot = compileNeedleShoot(recipe, variant, cohort);
      shoots.push(shoot);
      for (let plane = 0; plane < 3; plane++) {
        const tile = project(shoot, plane);
        planes.push(tile.plane);
        tiles.push(tile.levels);
      }
    }
  const levels = tiles[0].map((base, mip) => {
    const packed = new Uint8Array(base.length * tiles.length);
    for (let layer = 0; layer < tiles.length; layer++) packed.set(tiles[layer][mip], layer * base.length);
    return packed;
  });
  const product: ShootCoverage = {
    key,
    shoots,
    planes,
    coverage: {
      version: 1,
      format: "coverage-normal",
      key,
      layers: tiles.length,
      width: WIDTH,
      height: HEIGHT,
      levels,
    },
  };
  products.set(
    key,
    product,
    levels.reduce((n, v) => n + v.byteLength, 0) +
      shoots.reduce((n, s) => n + s.mesh.positions.byteLength * 3 + s.mesh.indices.byteLength, 0),
  );
  return product;
}

export function appendShootCoverage(
  mesh: Builder,
  uv: number[],
  layers: number[],
  product: ShootCoverage,
  shootIndex: number,
  branch: BotanicalBranch,
  source: string,
  segments: number,
) {
  const shoot = product.shoots[shootIndex];
  const tint =
    shoot.needles.reduce((sum, needle) => sum + needle.tint * 0.96, 0) / Math.max(1, shoot.needles.length);
  for (let side = 0; side < 3; side++) {
    const layer = shootIndex * 3 + side,
      plane = product.planes[layer],
      start = mesh.positions.length / 3;
    for (let row = 0; row <= segments; row++) {
      const y = plane.min[1] + ((plane.max[1] - plane.min[1]) * row) / segments;
      const frame = shootFrame(branch, y / shoot.recipe.length);
      const normal = normalize(
        add(scale(frame.x, -Math.sin(plane.angle)), scale(frame.z, Math.cos(plane.angle))),
      );
      for (let edge = 0; edge < 2; edge++) {
        const x = edge ? plane.max[0] : plane.min[0];
        vertex(
          mesh,
          shootPoint(branch, shoot.recipe, [x * Math.cos(plane.angle), y, x * Math.sin(plane.angle)]),
          normal,
          [tint, tint, tint],
          `${source}/plane${side}`,
        );
        uv.push(edge, row / segments);
        layers.push(layer);
      }
    }
    for (let row = 0; row < segments; row++) {
      const a = start + row * 2;
      mesh.indices.push(a, a + 1, a + 2, a + 1, a + 3, a + 2);
    }
  }
}
