import {
  add,
  alpineConiferSchema,
  cross,
  normalize,
  type Quality,
  scale,
  sub,
  type Vec3,
  type VegetationDefinition,
} from "@wrela/model";

import { type BotanicalBranch, botanicalBranchPoint, botanicalRandom } from "./botanical-branch";
import { developmentalStructure } from "./botanical-growth-structure";
import { bindBotanicalMotion } from "./botanical-motion";
import { type Builder, builder, finish, MAX_BOTANICAL_VERTICES, vertex } from "./botanical-primitives";
import { botanicalWood } from "./botanical-surfaces";
import { coniferStructure } from "./conifer-growth";
import { needleRibbons } from "./needle-ribbons";
import { appendShootReference, compileNeedleShoot, SHOOT_VARIANTS, type ShootRecipe } from "./needle-shoot";
import { appendShootCoverage, compileShootCoverage } from "./shoot-coverage";
import { needleCoverageModules, needleCoverageTile } from "./thin-coverage";

export const MAX_CONIFER_NEEDLES = 60000;
export function coniferShootRecipe(doc: VegetationDefinition): ShootRecipe {
  const conifer = doc.botanical?.conifer;
  if (!conifer) throw Error("Needle shoot requires a conifer source");
  return {
    length: conifer.architecture?.twigLength ?? 0.26,
    needleLength: conifer.needleLength,
    needleWidth: conifer.needleWidth,
    count: conifer.needlesPerShoot,
    loss: doc.botanical?.damage.leafLoss ?? 0,
    ...(conifer.architecture?.cohortContrast !== undefined
      ? { cohortContrast: conifer.architecture.cohortContrast }
      : {}),
  };
}
/** Curved, tapering triangular cross section; paired needles share an attachment. */
function needle(
  mesh: Builder,
  base: Vec3,
  direction: Vec3,
  length: number,
  width: number,
  roll: number,
  color: Vec3,
  source: string,
): void {
  const axis = normalize(direction);
  const u = normalize(cross(axis, Math.abs(axis[1]) > 0.9 ? [1, 0, 0] : [0, 1, 0]));
  const v = cross(axis, u);
  const bend = add(scale(u, Math.cos(roll + 0.7)), scale(v, Math.sin(roll + 0.7)));
  const first = mesh.positions.length / 3;
  for (let row = 0; row < 3; row++) {
    const t = row / 3;
    const center = add(
      add(base, scale(axis, length * t)),
      scale(bend, length * 0.085 * Math.sin(t * Math.PI)),
    );
    const tangent = normalize(add(axis, scale(bend, 0.085 * Math.PI * Math.cos(t * Math.PI))));
    const sideU = normalize(
      sub(u, scale(tangent, u[0] * tangent[0] + u[1] * tangent[1] + u[2] * tangent[2])),
    );
    const sideV = cross(tangent, sideU);
    for (let side = 0; side < 3; side++) {
      const angle = roll + (side * Math.PI * 2) / 3 + t * 0.24;
      const radial = add(scale(sideU, Math.cos(angle)), scale(sideV, Math.sin(angle)));
      vertex(
        mesh,
        add(center, scale(radial, width * 0.5 * (1 - t) ** 0.65)),
        normalize(add(radial, scale(tangent, width / length))),
        scale(color, 0.9 + t * 0.1),
        source,
      );
    }
  }
  const tip = vertex(mesh, add(base, scale(axis, length)), axis, color, source);
  for (let row = 0; row < 2; row++)
    for (let side = 0; side < 3; side++) {
      const a = first + row * 3 + side,
        b = first + row * 3 + ((side + 1) % 3);
      mesh.indices.push(a, b, a + 3, b, b + 3, a + 3);
    }
  for (let side = 0; side < 3; side++) mesh.indices.push(first + 6 + side, first + 6 + ((side + 1) % 3), tip);
}

/** Open, pointed needle masses follow each shoot and survive gameplay resolution. */
function foliageSpray(
  mesh: Builder,
  branch: BotanicalBranch,
  width: number,
  tint: number,
  source: string,
  distant: boolean,
): void {
  const spokes = 3;
  const teeth = distant ? 3 : 5;
  for (let spoke = 0; spoke < spokes; spoke++) {
    for (let tooth = 0; tooth < teeth; tooth++) {
      const t = 0.08 + tooth * (0.84 / teeth) + spoke * 0.019;
      const tangent = normalize(
        sub(
          botanicalBranchPoint(branch, Math.min(1, t + 0.04)),
          botanicalBranchPoint(branch, Math.max(0, t - 0.04)),
        ),
      );
      const lateral = normalize(cross(tangent, Math.abs(tangent[1]) > 0.9 ? [1, 0, 0] : [0, 1, 0]));
      const vertical = cross(tangent, lateral);
      const angle = (spoke * Math.PI * 2) / spokes + tooth * 0.71;
      const radial = normalize(add(scale(lateral, Math.cos(angle)), scale(vertical, Math.sin(angle))));
      const envelope = Math.sin(Math.PI * Math.min(1, t)) ** 0.65;
      const reach = width * envelope * (0.85 + 0.15 * Math.sin(tooth * 3.7 + spoke * 2.1));
      const a = botanicalBranchPoint(branch, Math.max(0, t - 0.025));
      const b = add(botanicalBranchPoint(branch, Math.min(1, t + 0.03)), scale(radial, reach));
      const c = botanicalBranchPoint(branch, Math.min(1, t + (0.84 / teeth) * 0.74));
      const outward = normalize(cross(sub(b, a), sub(c, a)));
      const shade = tint * (0.86 + 0.14 * Math.max(0, radial[1]));
      const color: Vec3 = [shade, shade * 1.04, shade * 0.94];
      const front = mesh.positions.length / 3;
      const leafSource = `${source}/${spoke}-${tooth}`;
      // Rounded branch-oriented shading avoids the uniform flat triangular highlights of a card fan.
      const crownNormal = normalize(add(outward, scale(add(radial, [0, 0.35, 0]), 0.4)));
      vertex(mesh, a, normalize(add(crownNormal, tangent)), scale(color, 0.82), leafSource);
      vertex(mesh, b, crownNormal, color, leafSource);
      vertex(mesh, c, normalize(sub(crownNormal, scale(tangent, 0.2))), color, leafSource);
      mesh.indices.push(front, front + 1, front + 2);
      const back = mesh.positions.length / 3;
      vertex(mesh, a, scale(crownNormal, -1), scale(color, 0.82), leafSource);
      vertex(mesh, c, scale(crownNormal, -1), color, leafSource);
      vertex(mesh, b, scale(crownNormal, -1), color, leafSource);
      mesh.indices.push(back, back + 1, back + 2);
    }
  }
}

/** Detail changes needle sampling, preserving all major limbs and shoot locations. */
export function coniferMeshes(
  doc: VegetationDefinition,
  quality: Quality,
  distant = false,
  representation: "filtered" | "triangles" | "legacy-filtered" = "filtered",
  sourcePrefix?: string,
) {
  const source = doc.botanical;
  const conifer = source?.conifer ?? (source?.development ? alpineConiferSchema.parse({}) : undefined);
  if (!source || !conifer) throw new Error("Conifer mesh requires authored source");
  const structure = source.development ? developmentalStructure(doc) : coniferStructure(doc);
  const architecture = source.development ? undefined : conifer.architecture;
  const cohorts = !!(source.development || architecture);
  const wood = builder(),
    leaves = builder();
  const random = (id: string) => botanicalRandom(doc.seed, id);
  const filtered = representation !== "triangles";
  const canonical = !!architecture && representation !== "legacy-filtered";
  const recipe = canonical ? coniferShootRecipe(doc) : undefined;
  const shootCoverage = filtered && recipe ? compileShootCoverage(recipe) : undefined;
  const coverageUV: number[] = [];
  const coverageLayers: number[] = [];
  const coverageWidth = conifer.needleLength * 1.05;
  const livingLengths = cohorts
    ? structure.branches
        .filter((branch) => !branch.bare && branch.cohort)
        .map((branch) => Math.hypot(...sub(branch.end, branch.start)))
        .sort((a, b) => a - b)
    : [];
  const cohortCount = cohorts
    ? Math.round(
        structure.branches
          .filter((branch) => !branch.bare && branch.cohort)
          .reduce((n, branch) => n + (branch.cohort?.count ?? 0), 0) / Math.max(1, livingLengths.length),
      )
    : conifer.needlesPerShoot;
  const coverage =
    shootCoverage?.coverage ??
    (filtered
      ? (cohorts ? needleCoverageModules : needleCoverageTile)({
          seed: doc.seed,
          count: Math.max(4, Math.round(cohortCount / 3)),
          width: conifer.needleWidth,
          length: conifer.needleLength,
          shootLength: Math.max(
            0.03,
            livingLengths[Math.floor(livingLengths.length / 2)] ?? doc.radius * conifer.shootSpread * 0.28,
          ),
          spread: coverageWidth,
          loss: source.damage.leafLoss,
        })
      : undefined);
  const sides = distant ? 4 : quality === "interactive" ? 6 : 10;
  const stride = quality === "interactive" ? 2 : 1;
  let leafCount = 0,
    truncated = structure.truncated;
  for (const branch of structure.branches) {
    if (sourcePrefix && branch.id !== sourcePrefix && !branch.id.startsWith(`${sourcePrefix}/`)) continue;
    const id = `${doc.id}/${branch.id}`;
    const shootSegments =
      architecture && branch.level === 2 ? 3 : filtered || quality === "interactive" ? 1 : 3;
    const woodVertices =
      branch.level === 0
        ? 17 * (sides + 2)
        : branch.level === 1
          ? (distant ? 2 : 6) * (sides * 6 + 2)
          : distant
            ? 0
            : shootSegments * 20;
    const woodFits = wood.positions.length / 3 + woodVertices <= MAX_BOTANICAL_VERTICES;
    if (!woodFits) truncated = true;
    if (woodFits && (branch.level < 2 || !distant)) {
      botanicalWood(
        wood,
        branch,
        branch.level === 0 ? sides + 2 : branch.level === 1 ? sides : 3,
        branch.level === 0
          ? source.development
            ? 2
            : 16
          : branch.level === 1
            ? distant
              ? 2
              : 6
            : shootSegments,
        id,
        "pine",
        source.age,
      );
    }
    if (
      branch.bare ||
      branch.broken ||
      (architecture ? !branch.cohort : !source.development && branch.level === 1 && source.growth.levels > 1)
    )
      continue;
    if (random(`${branch.id}/foliage`) >= source.canopy.density) continue;
    if (canonical && recipe && branch.cohort) {
      const variant = Math.floor(random(`${branch.id}/needle-module`) * SHOOT_VARIANTS);
      const cohort = branch.cohort.age > 0 ? 1 : 0;
      const shoot = compileNeedleShoot(recipe, variant, cohort);
      const required = filtered ? 3 * 2 * ((distant ? 1 : 3) + 1) : shoot.mesh.positions.length / 3;
      if (
        leaves.positions.length / 3 + required > MAX_BOTANICAL_VERTICES ||
        (!filtered && leafCount + shoot.needles.length > MAX_CONIFER_NEEDLES)
      ) {
        truncated = true;
        continue;
      }
      if (filtered && shootCoverage)
        appendShootCoverage(
          leaves,
          coverageUV,
          coverageLayers,
          shootCoverage,
          cohort * SHOOT_VARIANTS + variant,
          branch,
          `${id}/coverage`,
          distant ? 1 : 3,
        );
      else appendShootReference(leaves, shoot, branch, id);
      leafCount += shoot.needles.length;
      continue;
    }
    if (filtered) {
      if (source.damage.leafLoss >= 1 || (!source.development && branch.level === 0)) continue;
      if (leaves.positions.length / 3 + 24 > MAX_BOTANICAL_VERTICES) {
        truncated = true;
        continue;
      }
      const firstVertex = leaves.positions.length / 3;
      needleRibbons(
        leaves,
        coverageUV,
        branch,
        coverageWidth,
        `${id}/coverage`,
        branch.cohort?.tint ?? 0.88 + random(`${branch.id}/tint`) * 0.12,
        distant ? 1 : 3,
      );
      if (cohorts) {
        const layer =
          Math.floor(random(`${branch.id}/needle-module`) * 4) +
          (branch.cohort && branch.cohort.retention < 0.75 ? 4 : 0);
        for (let i = firstVertex; i < leaves.positions.length / 3; i++) coverageLayers.push(layer);
      }
      leafCount += Math.round(
        (branch.cohort?.count ?? conifer.needlesPerShoot) * (branch.cohort?.retention ?? 1),
      );
      continue;
    }
    if (leaves.positions.length / 3 + 90 > MAX_BOTANICAL_VERTICES) {
      truncated = true;
      continue;
    }
    if (branch.level === 2 && distant) {
      const shootLength = Math.hypot(...sub(branch.end, branch.start));
      const width = Math.max(conifer.needleLength * 0.5, Math.min(0.06, shootLength * 0.16));
      if (
        (!distant || random(`${branch.id}/distant`) < 0.34) &&
        random(`${branch.id}/spray-loss`) >= source.damage.leafLoss
      )
        foliageSpray(
          leaves,
          branch,
          width * (distant ? 1.25 : 1),
          0.86 + random(`${branch.id}/spray-tint`) * 0.2,
          `${id}/spray`,
          distant,
        );
    }
    const leader = branch.level === 0;
    const count = branch.cohort?.count ?? conifer.needlesPerShoot;
    const axisLength = Math.hypot(...sub(branch.end, branch.start)) * (leader ? 0.12 : 1);
    for (let n = 0; n < (distant ? 0 : count); n += stride) {
      if (leafCount >= MAX_CONIFER_NEEDLES || leaves.positions.length / 3 + 10 > MAX_BOTANICAL_VERTICES) {
        truncated = true;
        break;
      }
      const key = `${branch.id}/n${n}`;
      if (random(`${key}/loss`) < 1 - (1 - source.damage.leafLoss) * (branch.cohort?.retention ?? 1))
        continue;
      const t = 0.1 + ((Math.floor(n / 2) + 0.5) / Math.ceil(count / 2)) * 0.88;
      const along = leader ? 0.88 + t * 0.12 : t;
      const base = botanicalBranchPoint(branch, along);
      const direction = normalize(
        sub(
          botanicalBranchPoint(branch, Math.min(1, along + 0.02)),
          botanicalBranchPoint(branch, Math.max(0, along - 0.02)),
        ),
      );
      const u = normalize(cross(direction, Math.abs(direction[1]) > 0.9 ? [1, 0, 0] : [0, 1, 0]));
      const v = cross(direction, u);
      // Adjacent pairs share a fascicle angle with a small opening between their needles.
      const pair = Math.floor(n / 2);
      const angle = pair * 2.399963 + random(`${branch.id}/pair${pair}`) * 0.5 + (n % 2 ? 0.14 : -0.14);
      const radial = add(scale(u, Math.cos(angle)), scale(v, Math.sin(angle)));
      const needleDirection = normalize(add(scale(direction, 0.3 + t * 0.35), scale(radial, 0.9)));
      const length = cohorts
        ? conifer.needleLength * (0.8 + random(`${key}/length`) * 0.4)
        : Math.min(
            conifer.needleLength * (0.68 + random(`${key}/length`) * 0.55),
            Math.max(conifer.needleLength * 0.45, axisLength * 0.38),
          );
      // Reduced interactive sampling preserves the source's approximate projected
      // needle area rather than making the canopy disappear with lower quality.
      const width = conifer.needleWidth * stride;
      const shade = 0.74 + t * 0.18 + random(`${key}/tint`) * 0.14;
      needle(
        leaves,
        base,
        needleDirection,
        length * (distant ? 1.35 : 1),
        width,
        angle,
        [shade, shade, shade],
        `${doc.id}/${key}`,
      );
      leafCount++;
    }
  }
  return {
    trunk: bindBotanicalMotion(finish(wood), doc, structure.branches, false),
    foliage: bindBotanicalMotion(
      {
        ...finish(leaves),
        ...(coverage
          ? {
              thinCoverage: {
                ...coverage,
                uv: new Float32Array(coverageUV),
                ...(cohorts ? { layer: new Uint16Array(coverageLayers) } : {}),
              },
            }
          : {}),
      },
      doc,
      structure.branches,
      true,
    ),
    structure,
    leafCount,
    truncated,
  };
}
