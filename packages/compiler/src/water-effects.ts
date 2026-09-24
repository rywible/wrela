import { contentKey, type MeshData, type Vec3, type WaterDefinition } from "@wrela/model";

const cache = new Map<string, MeshData[]>();
/** Parameter sheets retain overhangs; bounded ballistic droplets share the event clock. */
export function compileWaterEffects(water: WaterDefinition): MeshData[] {
  const key = contentKey({ version: 1, effects: water.effects, level: water.level });
  const known = cache.get(key);
  if (known) return known;
  const meshes = (water.effects ?? []).map((effect) => {
    const positions: number[] = [],
      normals: number[] = [],
      colors: number[] = [],
      indices: number[] = [];
    const columns = 48,
      rows = 24;
    for (let j = 0; j <= rows; j++)
      for (let i = 0; i <= columns; i++) {
        const u = i / columns,
          v = j / rows;
        positions.push(
          effect.start[0] * (1 - u) + effect.end[0] * u,
          water.level,
          effect.start[1] * (1 - u) + effect.end[1] * u,
        );
        normals.push(0, 1, 0);
        colors.push(u, v, 0);
        if (j < rows && i < columns) {
          const k = j * (columns + 1) + i;
          indices.push(k, k + columns + 1, k + 1, k + 1, k + columns + 1, k + columns + 2);
        }
      }
    for (let drop = 0; drop < 72; drop++) {
      const first = positions.length / 3;
      for (const p of [
        [1, 0, 0],
        [-1, 0, 0],
        [0, 1, 0],
        [0, -1, 0],
        [0, 0, 1],
        [0, 0, -1],
      ]) {
        positions.push(...p);
        normals.push(...p);
        colors.push((drop * 0.61803398875) % 1, 0, drop + 1);
      }
      for (const face of [
        [2, 0, 4],
        [2, 4, 1],
        [2, 1, 5],
        [2, 5, 0],
        [3, 4, 0],
        [3, 1, 4],
        [3, 5, 1],
        [3, 0, 5],
      ])
        indices.push(...face.map((i) => first + i));
    }
    const r = effect.width + effect.height * 2.5 + 1;
    return {
      positions: new Float32Array(positions),
      normals: new Float32Array(normals),
      colors: new Float32Array(colors),
      indices: new Uint32Array(indices),
      bounds: {
        min: [
          Math.min(effect.start[0], effect.end[0]) - r,
          water.level - 1,
          Math.min(effect.start[1], effect.end[1]) - r,
        ] as Vec3,
        max: [
          Math.max(effect.start[0], effect.end[0]) + r,
          water.level + effect.height * 2 + 1,
          Math.max(effect.start[1], effect.end[1]) + r,
        ] as Vec3,
      },
    };
  });
  if (cache.size >= 16) cache.delete(cache.keys().next().value as string);
  cache.set(key, meshes);
  return meshes;
}
