import { z } from "zod";
import {
  type AssemblyDefinition,
  type AssemblyPart,
  type AssemblyProfile,
  assemblyPartPoint,
} from "./assembly";
import { hash32, normalize, sub, type Vec3 } from "./math";
import { type SurfaceHistorySignals, sampleSurfaceHistory, surfaceHistorySchema } from "./surface-history";
import { type SurfaceHistoryClass, surfaceHistoryClass } from "./surface-history-material";

export const constructionRecipeSchema = z.object({
  seed: z.number().int().min(0).max(0x7fffffff).default(73),
  history: surfaceHistorySchema,
  jointGap: z.number().finite().min(0.003).max(0.08).default(0.014),
  courseHeight: z.number().finite().min(0.08).max(1.5).default(0.3),
  moduleWidth: z.number().finite().min(0.15).max(3).default(0.58),
  stoneMaterial: z.string().min(1).max(100),
});
export type ConstructionRecipe = z.infer<typeof constructionRecipeSchema>;
export function constructionRandom(seed: number, id: string, stream = 0): number {
  let value = hash32(seed ^ Math.imul(stream, 2654435761));
  for (const character of id) value = hash32(value ^ character.charCodeAt(0));
  return (value >>> 0) / 0x1_0000_0000;
}
/** Convex corner losses preserve a measured envelope and never enlarge a clearance. */
export function cutStoneProfile(
  width: number,
  height: number,
  seed: number,
  id: string,
  damage: number,
): AssemblyProfile {
  if (!(width > 0) || !(height > 0) || ![width, height, damage].every(Number.isFinite))
    throw new Error("Stone dimensions and damage must be finite");
  const minimum = Math.min(width, height);
  const cuts = [0, 1, 2, 3].map(
    (corner) =>
      minimum *
      (0.025 + constructionRandom(seed, id, corner) * (0.075 + Math.max(0, Math.min(1, damage)) * 0.3)),
  );
  const [bl, br, tr, tl] = cuts;
  return {
    kind: "polygon",
    points: [
      [-width / 2 + bl, -height / 2],
      [width / 2 - br, -height / 2],
      [width / 2, -height / 2 + br],
      [width / 2, height / 2 - tr],
      [width / 2 - tr, height / 2],
      [-width / 2 + tl, height / 2],
      [-width / 2, height / 2 - tl],
      [-width / 2, -height / 2 + bl],
    ],
  };
}
export type ConstructionHistoryRecord = {
  part: string;
  class: SurfaceHistoryClass;
  signals: SurfaceHistorySignals;
};
/** Materialization remains ordinary editable assembly source, independent of rendering representation. */
export function createConstructionBuilder(authored: ConstructionRecipe) {
  const recipe = constructionRecipeSchema.parse(authored);
  const parts: AssemblyPart[] = [];
  const part = (
    id: string,
    profile: AssemblyProfile,
    position: Vec3,
    depth: number,
    material = recipe.stoneMaterial,
    bevel = 0.007,
  ): AssemblyPart => {
    if (parts.length >= 128) throw new Error("Construction exceeds the 128-part editable assembly budget");
    const result: AssemblyPart = {
      id,
      name: id.replaceAll("-", " "),
      profile,
      path: [
        [0, 0, 0],
        [0, 0, depth],
      ],
      position,
      rotation: [0, 0, 0],
      material,
      bevel,
      endBevel: Math.min(bevel * 1.4, depth * 0.06),
      repeat: { count: 1, offset: [0, 0, 0] },
      sockets: [],
      wear: { amount: 0, scale: 2.8, seed: Math.floor(constructionRandom(recipe.seed, id, 19) * 65536) },
    };
    parts.push(result);
    return result;
  };
  const stone = (
    id: string,
    width: number,
    height: number,
    position: Vec3,
    depth: number,
    material = recipe.stoneMaterial,
  ): AssemblyPart => {
    const history = sampleSurfaceHistory(recipe.history, { position, normal: [0, 0, -1] });
    return part(
      id,
      cutStoneProfile(width, height, recipe.seed, id, history.damage),
      position,
      depth,
      material,
      Math.min(width, height) * 0.016,
    );
  };
  const wall = (options: {
    id: string;
    position: Vec3;
    length: number;
    courses: number;
    depth: number;
    collapse?: number;
    yaw?: number;
  }) => {
    const generated: AssemblyPart[] = [];
    const { id, position, length, courses, depth, collapse = 0, yaw = 0 } = options;
    if (
      !(length > 0) ||
      !Number.isInteger(courses) ||
      courses < 1 ||
      courses > 16 ||
      collapse < 0 ||
      collapse > 1
    )
      throw new Error("Invalid masonry wall dimensions");
    for (let row = 0; row < courses; row++) {
      const loss = collapse * (row / Math.max(1, courses - 1)) ** 1.6 * length * 0.5;
      const span = length - loss,
        count = Math.max(1, Math.ceil(span / recipe.moduleWidth));
      const fractions = Array.from(
        { length: count },
        (_, index) => 0.8 + constructionRandom(recipe.seed, `${id}-${row}-${index}`, 1) * 0.4,
      );
      const total = fractions.reduce((sum, value) => sum + value, 0);
      let at = -length / 2;
      for (let column = 0; column < count; column++) {
        const width = (span * fractions[column]) / total;
        const localX = at + width / 2;
        const block = stone(
          `${id}-course-${row}-stone-${column}`,
          width - recipe.jointGap,
          recipe.courseHeight - recipe.jointGap,
          [
            position[0] + Math.cos(yaw) * localX,
            position[1] + recipe.courseHeight * (row + 0.5),
            position[2] - Math.sin(yaw) * localX,
          ],
          depth,
        );
        block.rotation[1] = yaw;
        block.rotation[2] = (constructionRandom(recipe.seed, block.id, 2) - 0.5) * 0.018 * collapse;
        generated.push(block);
        at += width;
      }
    }
    return generated;
  };
  const arch = (options: {
    id: string;
    position: Vec3;
    innerRadius: number;
    thickness: number;
    depth: number;
    stones: number;
  }) => {
    if (
      options.stones < 3 ||
      options.stones > 32 ||
      !Number.isInteger(options.stones) ||
      options.innerRadius <= 0 ||
      options.thickness <= 0
    )
      throw new Error("Invalid masonry arch dimensions");
    return Array.from({ length: options.stones }, (_, index) => {
      const gap = recipe.jointGap / (options.innerRadius + options.thickness / 2);
      const start = (index * Math.PI) / options.stones + gap / 2,
        end = ((index + 1) * Math.PI) / options.stones - gap / 2;
      if (gap >= Math.PI / options.stones) throw new Error("Arch joints exceed the voussoir angular width");
      const outer = options.innerRadius + options.thickness;
      return part(
        `${options.id}-${index}`,
        {
          kind: "polygon",
          points: [
            [Math.cos(start) * options.innerRadius, Math.sin(start) * options.innerRadius],
            [Math.cos(start) * outer, Math.sin(start) * outer],
            [Math.cos(end) * outer, Math.sin(end) * outer],
            [Math.cos(end) * options.innerRadius, Math.sin(end) * options.innerRadius],
          ],
        },
        [...options.position],
        options.depth,
        recipe.stoneMaterial,
        0.006,
      );
    });
  };
  const rubble = (options: {
    id: string;
    center: Vec3;
    radius: [number, number];
    count: number;
    size?: number;
    exclude?: (position: Vec3) => boolean;
  }) => {
    if (!Number.isInteger(options.count) || options.count < 0 || options.count > 40)
      throw new Error("Rubble count must be 0–40");
    const generated: AssemblyPart[] = [];
    for (let index = 0; index < options.count; index++) {
      const id = `${options.id}-${index}`;
      const angle = constructionRandom(recipe.seed, id, 0) * Math.PI * 2;
      const radius = Math.sqrt(constructionRandom(recipe.seed, id, 1));
      const size = (options.size ?? 0.22) * (0.55 + constructionRandom(recipe.seed, id, 2) * 0.7);
      const position: Vec3 = [
        options.center[0] + Math.cos(angle) * radius * options.radius[0],
        options.center[1] + size * 0.25,
        options.center[2] + Math.sin(angle) * radius * options.radius[1],
      ];
      if (options.exclude?.(position)) continue;
      const block = stone(id, size, size * 0.53, position, size * 0.75);
      block.rotation = [
        constructionRandom(recipe.seed, id, 3) * 0.18,
        angle,
        (constructionRandom(recipe.seed, id, 4) - 0.5) * 0.2,
      ];
      block.collision = false;
      generated.push(block);
    }
    return generated;
  };
  return { recipe, parts, part, stone, wall, arch, rubble };
}

export function applyConstructionHistory(
  assembly: AssemblyDefinition,
  recipe: ConstructionRecipe,
  resolveMaterial: (base: string, kind: SurfaceHistoryClass) => string,
): ConstructionHistoryRecord[] {
  return assembly.parts.map((part) => {
    const center = assemblyPartPoint(assembly, part, [0, 0, 0]);
    const signals = sampleSurfaceHistory(recipe.history, {
      position: center,
      normal: normalize(sub(assemblyPartPoint(assembly, part, [0, 0, -1]), center)),
      shelter: part.parent ? 0.35 : 0.05,
    });
    const broken = /rubble|fallen|fragment|collapse/.test(part.id);
    const kind = surfaceHistoryClass(signals, broken);
    part.material = resolveMaterial(part.material ?? recipe.stoneMaterial, kind);
    part.wear.amount = Math.max(part.wear.amount, signals.damage * 0.4 + signals.dirt * 0.1);
    return { part: part.id, class: kind, signals };
  });
}
