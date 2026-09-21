/** Analytic inner-occluder certificates. A sphere must lie INSIDE opaque geometry.
 * Camera coordinates use +Z forward. Camera-inside / near-plane cases stay visible.
 * The mathematical bound needs outward rounding before use as a production proof. */
export interface Sphere {
  x: number;
  y: number;
  z: number;
  radius: number;
}
export interface Rect {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}
export function sphereFront(sphere: Sphere, u: number, v: number) {
  const { x, y, z, radius } = sphere;
  if (z <= radius) return Number.POSITIVE_INFINITY;
  const c = x * x + y * y + z * z - radius * radius;
  const dot = x * u + y * v + z;
  const discriminant = dot * dot - c * (u * u + v * v + 1);
  if (discriminant < 0 || dot <= 0) return Number.POSITIVE_INFINITY;
  return c / (dot + Math.sqrt(discriminant));
}
export function tileOccluder(sphere: Sphere, rect: Rect) {
  return Math.max(
    sphereFront(sphere, rect.x0, rect.y0),
    sphereFront(sphere, rect.x1, rect.y0),
    sphereFront(sphere, rect.x0, rect.y1),
    sphereFront(sphere, rect.x1, rect.y1),
  );
}
export function projectedSphere(sphere: Sphere): Rect | null {
  const { x, y, z, radius: r } = sphere;
  if (z <= r) return null;
  const denominator = z * z - r * r;
  const ex = r * Math.sqrt(z * z + x * x - r * r),
    ey = r * Math.sqrt(z * z + y * y - r * r);
  return {
    x0: (x * z - ex) / denominator,
    x1: (x * z + ex) / denominator,
    y0: (y * z - ey) / denominator,
    y1: (y * z + ey) / denominator,
  };
}
export interface TileGrid {
  width: number;
  height: number;
  halfWidth: number;
  halfHeight: number;
}
export function tileRect(grid: TileGrid, x: number, y: number): Rect {
  return {
    x0: ((2 * x) / grid.width - 1) * grid.halfWidth,
    x1: ((2 * (x + 1)) / grid.width - 1) * grid.halfWidth,
    y0: ((2 * y) / grid.height - 1) * grid.halfHeight,
    y1: ((2 * (y + 1)) / grid.height - 1) * grid.halfHeight,
  };
}
export function tileRange(grid: TileGrid, rect: Rect) {
  return {
    x0: Math.max(0, Math.floor(((rect.x0 / grid.halfWidth + 1) * grid.width) / 2)),
    x1: Math.min(grid.width - 1, Math.floor(((rect.x1 / grid.halfWidth + 1) * grid.width) / 2)),
    y0: Math.max(0, Math.floor(((rect.y0 / grid.halfHeight + 1) * grid.height) / 2)),
    y1: Math.min(grid.height - 1, Math.floor(((rect.y1 / grid.halfHeight + 1) * grid.height) / 2)),
  };
}
export function compileOccluders(spheres: Sphere[], grid: TileGrid) {
  const depths = new Float64Array(grid.width * grid.height).fill(Number.POSITIVE_INFINITY);
  for (const sphere of spheres) {
    const rect = projectedSphere(sphere);
    if (!rect) continue;
    const range = tileRange(grid, rect);
    for (let y = range.y0; y <= range.y1; y++)
      for (let x = range.x0; x <= range.x1; x++) {
        const i = y * grid.width + x;
        depths[i] = Math.min(depths[i], tileOccluder(sphere, tileRect(grid, x, y)));
      }
  }
  return depths;
}
export function hidden(sphere: Sphere, grid: TileGrid, depths: Float64Array | Float32Array) {
  const rect = projectedSphere(sphere);
  if (!rect) return false;
  const range = tileRange(grid, rect);
  if (range.x0 > range.x1 || range.y0 > range.y1) return false;
  const nearest = sphere.z - sphere.radius;
  for (let y = range.y0; y <= range.y1; y++)
    for (let x = range.x0; x <= range.x1; x++) {
      if (!(nearest > depths[y * grid.width + x])) return false;
    }
  return true;
}
export function crowdFixture() {
  const grid: TileGrid = { width: 160, height: 90, halfWidth: (16 / 9) * 0.6, halfHeight: 0.6 };
  const occluders: Sphere[] = [],
    candidates: Sphere[] = [];
  // Staggered opaque field stones across the foreground. Candidates are independent
  // small character bounds behind the stones, not occluders in their own proof.
  for (let y = -3; y <= 3; y++)
    for (let x = -5; x <= 5; x++) {
      occluders.push({ x: x * 1.15 + (y % 2) * 0.25, y: y * 1.1, z: 6 + (x % 3) * 0.15, radius: 0.77 });
    }
  for (let layer = 0; layer < 8; layer++)
    for (let y = 0; y < 16; y++)
      for (let x = 0; x < 32; x++) {
        const z = 12 + layer * 1.1;
        candidates.push({
          x: (((x + 0.5) / 32) * 2 - 1) * z * grid.halfWidth * 0.92,
          y: (((y + 0.5) / 16) * 2 - 1) * z * grid.halfHeight * 0.92,
          z,
          radius: 0.16,
        });
      }
  return { grid, occluders, candidates };
}
