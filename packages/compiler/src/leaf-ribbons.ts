import { add, type BotanicalSpecies, contentKey, cross, normalize, scale, type Vec3 } from "@wrela/model";

import { type Builder, vertex } from "./botanical-primitives";
import { type CoverageTile, coverageMipChain } from "./thin-coverage";

/** Species outlines remain mathematical source descriptions; the compiler
 * samples their scalar occupancy and complete area mip chain. Clear margins
 * allow the boundary filter to reach zero without clipping at the proxy edge. */
export function leafCoverageTile(species: BotanicalSpecies): CoverageTile {
  const width = 64,
    height = 128,
    samples = 4,
    base = new Uint8Array(width * height);
  for (let y = 0; y < height; y++)
    for (let x = 0; x < width; x++) {
      let coverage = 0;
      for (let sy = 0; sy < samples; sy++)
        for (let sx = 0; sx < samples; sx++) {
          const u = (x + (sx + 0.5) / samples) / width,
            v = (y + (sy + 0.5) / samples) / height;
          const t = (v - 0.05) / 0.9;
          if (t < 0 || t > 1) continue;
          let halfWidth: number;
          if (species === "grass" || species === "pine") halfWidth = 0.45 * (1 - t) ** 0.7;
          else {
            halfWidth = 0.45 * Math.sin(Math.PI * t) ** (species === "fern" ? 1 : 0.72);
            if (species === "oak") halfWidth *= 0.82 + 0.18 * Math.cos(t * Math.PI * 12);
            if (species === "birch") halfWidth *= 0.94 + 0.06 * Math.cos(t * Math.PI * 32);
          }
          if (Math.abs(u - 0.5) <= halfWidth) coverage++;
        }
      base[y * width + x] = Math.round((255 * coverage) / (samples * samples));
    }
  return {
    version: 1,
    key: contentKey({ algorithm: "semantic-leaf-coverage-1", species }),
    width,
    height,
    levels: coverageMipChain(base, width, height),
  };
}

/** Folded blade with a curved centerline and source-local coverage coordinates.
 * Detail only simplifies curvature: leaf count, dimensions and outline stay. */
export function leafRibbon(
  mesh: Builder,
  uv: number[],
  base: Vec3,
  direction: Vec3,
  length: number,
  width: number,
  roll: number,
  droop: number,
  color: Vec3,
  source: string,
  segments: number,
) {
  const axis = normalize(direction);
  const u = normalize(cross(axis, Math.abs(axis[1]) > 0.9 ? [1, 0, 0] : [0, 1, 0]));
  const v = cross(axis, u),
    lateral = add(scale(u, Math.cos(roll)), scale(v, Math.sin(roll)));
  const fold = normalize(cross(lateral, axis));
  const first = mesh.positions.length / 3;
  for (let row = 0; row <= segments; row++) {
    const t = (row / segments - 0.05) / 0.9;
    const center = add(add(base, scale(axis, length * t)), [0, -length * droop * 0.5 * t * t, 0]);
    for (let column = 0; column < 3; column++) {
      const across = (column * 0.5 - 0.5) / 0.9;
      const point = add(
        add(center, scale(lateral, across * width)),
        scale(fold, column === 1 ? width * 0.08 : 0),
      );
      vertex(mesh, point, normalize(add(fold, scale(lateral, across * 0.3))), color, source);
      uv.push(column * 0.5, row / segments);
    }
  }
  for (let row = 0; row < segments; row++)
    for (let column = 0; column < 2; column++) {
      const a = first + row * 3 + column;
      mesh.indices.push(a, a + 1, a + 3, a + 1, a + 4, a + 3);
    }
}
