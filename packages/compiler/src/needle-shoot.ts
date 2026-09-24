import { add, contentKey, cross, dot, normalize, scale, sub, type Vec3 } from "@wrela/model";

import { type BotanicalBranch, botanicalBranchPoint, botanicalRandom } from "./botanical-branch";
import { type Builder, builder, finish, vertex } from "./botanical-primitives";
import { ProductCache } from "./cache";

export const SHOOT_VERSION = "paired-shoot-2";
export const SHOOT_VARIANTS = 4;
export const SHOOT_COHORTS = 2;
export type ShootRecipe = {
  length: number;
  needleLength: number;
  needleWidth: number;
  count: number;
  loss: number;
  cohortContrast?: number;
};
export type ShootNeedle = {
  id: string;
  pair: number;
  attachment: number;
  angle: number;
  width: number;
  centers: Vec3[];
  tint: number;
};
export type NeedleShoot = {
  key: string;
  recipe: ShootRecipe;
  variant: number;
  cohort: number;
  needles: ShootNeedle[];
  mesh: ReturnType<typeof finish>;
  /** Each needle owns ten vertices and fifteen triangles in the reference. */
  needleRanges: { firstIndex: number; count: number; plane: number }[];
};
const shoots = new ProductCache<NeedleShoot>(16 * 1024 * 1024, 128);

/** The source is a reusable 3D shoot, not a camera-facing pattern. Instance axial
 * scaling is an explicit part of its shape; reference and proxy use the same map. */
export function compileNeedleShoot(recipe: ShootRecipe, variant: number, cohort = 0): NeedleShoot {
  if (
    ![recipe.length, recipe.needleLength, recipe.needleWidth].every((v) => Number.isFinite(v) && v > 0) ||
    !Number.isInteger(recipe.count) ||
    recipe.count < 0 ||
    recipe.count > 160 ||
    !Number.isFinite(recipe.loss) ||
    recipe.loss < 0 ||
    recipe.loss > 1 ||
    (recipe.cohortContrast !== undefined &&
      (!Number.isFinite(recipe.cohortContrast) || recipe.cohortContrast < 0 || recipe.cohortContrast > 1)) ||
    !Number.isInteger(variant) ||
    variant < 0 ||
    variant >= SHOOT_VARIANTS ||
    !Number.isInteger(cohort) ||
    cohort < 0 ||
    cohort >= SHOOT_COHORTS
  )
    throw Error("Invalid canonical needle shoot");
  const key = contentKey({ version: SHOOT_VERSION, recipe, variant, cohort });
  const cached = shoots.get(key);
  if (cached) return cached;
  const mesh = builder(),
    needles: ShootNeedle[] = [],
    needleRanges: NeedleShoot["needleRanges"] = [];
  const random = (id: string) => botanicalRandom(7919 * (variant + 1), id);
  const retained = cohort ? 0.78 : 1;
  for (let n = 0; n < recipe.count; n++) {
    const pair = Math.floor(n / 2),
      id = `pair${pair}/n${n % 2}`;
    if (random(`${id}/loss`) < 1 - (1 - recipe.loss) * retained) continue;
    // A short bare base and clustered terminal growth leave meaningful exposed wood.
    const q = (pair + 0.5) / Math.ceil(recipe.count / 2);
    const attachment = 0.19 + 0.77 * q ** 0.78;
    const angle = pair * 2.3999632297 + random(`pair${pair}`) * 0.42 + (n % 2 ? 0.13 : -0.13);
    const radial: Vec3 = [Math.cos(angle), 0, Math.sin(angle)];
    const axis = normalize(add(scale(radial, 0.92), [0, 0.23 + attachment * 0.36, 0]));
    const length = recipe.needleLength * (0.84 + random(`${id}/length`) * 0.32) * (cohort ? 0.96 : 1);
    const base: Vec3 = [0, attachment * recipe.length, 0];
    const bend = normalize(add(scale(radial, -0.18), [0, 1, 0]));
    const centers = Array.from({ length: 4 }, (_, row) => {
      const t = row / 3;
      return add(add(base, scale(axis, length * t)), scale(bend, length * 0.11 * Math.sin(t * Math.PI)));
    });
    const needle: ShootNeedle = {
      id,
      pair,
      attachment,
      angle,
      width: recipe.needleWidth,
      centers,
      tint:
        ((cohort ? 0.79 : 0.91) + attachment * 0.06 + random(`${id}/tint`) * 0.06) *
        (1 + (recipe.cohortContrast ?? 0) * (cohort ? -0.04 : 0.6)),
    };
    const firstIndex = mesh.indices.length;
    appendNeedle(mesh, needle, id);
    const wrapped = ((angle % Math.PI) + Math.PI) % Math.PI;
    needleRanges.push({
      firstIndex,
      count: mesh.indices.length - firstIndex,
      plane: Math.round(wrapped / (Math.PI / 3)) % 3,
    });
    needles.push(needle);
  }
  const product = { key, recipe, variant, cohort, needles, needleRanges, mesh: finish(mesh) };
  shoots.set(key, product, mesh.positions.length * 12 + mesh.indices.length * 4 + needles.length * 220);
  return product;
}

function appendNeedle(mesh: Builder, needle: ShootNeedle, source: string) {
  const first = mesh.positions.length / 3;
  const axis = normalize(sub(needle.centers[3], needle.centers[0]));
  const u = normalize(cross(axis, [0, 1, 0]));
  for (let row = 0; row < 3; row++) {
    const t = row / 3;
    const tangent = normalize(sub(needle.centers[row + 1], needle.centers[Math.max(0, row - 1)]));
    const sideU = normalize(sub(u, scale(tangent, dot(u, tangent))));
    const sideV = cross(tangent, sideU);
    for (let side = 0; side < 3; side++) {
      const a = (side * Math.PI * 2) / 3 + needle.angle;
      const radial = add(scale(sideU, Math.cos(a)), scale(sideV, Math.sin(a)));
      const shade = needle.tint * (0.92 + t * 0.08);
      vertex(
        mesh,
        add(needle.centers[row], scale(radial, needle.width * 0.5 * (1 - t) ** 0.65)),
        normalize(
          add(
            radial,
            scale(
              tangent,
              needle.width / Math.max(1e-6, Math.hypot(...sub(needle.centers[3], needle.centers[0]))),
            ),
          ),
        ),
        [shade, shade, shade],
        source,
      );
    }
  }
  const tip = vertex(mesh, needle.centers[3], axis, [needle.tint, needle.tint, needle.tint], source);
  for (let row = 0; row < 2; row++)
    for (let side = 0; side < 3; side++) {
      const a = first + row * 3 + side,
        b = first + row * 3 + ((side + 1) % 3);
      mesh.indices.push(a, b, a + 3, b, b + 3, a + 3);
    }
  for (let side = 0; side < 3; side++) mesh.indices.push(first + 6 + side, first + 6 + ((side + 1) % 3), tip);
}

/** Stable frame along an authored curve, including the needle tips beyond the wood. */
export function shootFrame(branch: BotanicalBranch, t: number) {
  const clamped = Math.max(0, Math.min(1, t));
  const tangent = normalize(
    sub(
      botanicalBranchPoint(branch, Math.min(1, clamped + 0.015)),
      botanicalBranchPoint(branch, Math.max(0, clamped - 0.015)),
    ),
  );
  const axis = normalize(sub(branch.end, branch.start));
  const side = normalize(cross(axis, Math.abs(axis[1]) > 0.95 ? [1, 0, 0] : [0, 1, 0]));
  const x = normalize(sub(side, scale(tangent, dot(side, tangent))));
  const z = cross(x, tangent);
  const length = Math.max(1e-6, Math.hypot(...sub(branch.end, branch.start)));
  const center = add(botanicalBranchPoint(branch, clamped), scale(tangent, (t - clamped) * length));
  return { center, x, y: tangent, z, length };
}
export function shootPoint(branch: BotanicalBranch, recipe: ShootRecipe, p: Vec3): Vec3 {
  const frame = shootFrame(branch, p[1] / recipe.length);
  return add(frame.center, add(scale(frame.x, p[0]), scale(frame.z, p[2])));
}
export function appendShootReference(
  mesh: Builder,
  shoot: NeedleShoot,
  branch: BotanicalBranch,
  source: string,
) {
  const offset = mesh.positions.length / 3;
  const { positions, normals, colors, indices, sourceIds } = shoot.mesh;
  for (let i = 0; i < positions.length; i += 3) {
    const p = Array.from(positions.subarray(i, i + 3)) as Vec3;
    const frame = shootFrame(branch, p[1] / shoot.recipe.length);
    const n: Vec3 = [normals[i], normals[i + 1], normals[i + 2]];
    const normal = normalize(
      add(
        add(scale(frame.x, n[0]), scale(frame.y, (n[1] * shoot.recipe.length) / frame.length)),
        scale(frame.z, n[2]),
      ),
    );
    vertex(
      mesh,
      shootPoint(branch, shoot.recipe, p),
      normal,
      colors ? (Array.from(colors.subarray(i, i + 3)) as Vec3) : [1, 1, 1],
      `${source}/${sourceIds?.[i / 3]}`,
    );
  }
  for (const index of indices) mesh.indices.push(offset + index);
}
