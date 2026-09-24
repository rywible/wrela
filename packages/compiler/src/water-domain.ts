import {
  type CompiledWaterDomain,
  contentKey,
  type MeshData,
  type Vec3,
  type WaterDefinition,
} from "@wrela/model";

const cache = new Map<string, CompiledWaterDomain>();
const smooth = (t: number) => {
  const x = Math.max(0, Math.min(1, t));
  return x * x * (3 - 2 * x);
};
function bedNoise(x: number, z: number) {
  const ix = Math.floor(x),
    iz = Math.floor(z),
    fx = smooth(x - ix),
    fz = smooth(z - iz);
  const hash = (x: number, z: number) => {
    let h = Math.imul(x, 374761393) ^ Math.imul(z, 668265263);
    h = Math.imul(h ^ (h >>> 13), 1274126177);
    return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
  };
  const a = hash(ix, iz) * (1 - fx) + hash(ix + 1, iz) * fx,
    b = hash(ix, iz + 1) * (1 - fx) + hash(ix + 1, iz + 1) * fx;
  return a * (1 - fz) + b * fz;
}
/** One bed source supplies optical depth, collision, wet banks and solver geometry. */
export function waterBedAt(water: WaterDefinition, x: number, z: number) {
  const domain = water.domain;
  if (!domain) throw new Error("Water has no bounded bed");
  let bed = water.level + domain.bankHeight,
    level = water.level,
    u = 0,
    v = 0;
  const include = (surface: number, depth: number, edge: number, vx: number, vz: number) => {
    const candidate =
      edge > 0 ? surface + domain.bankHeight * smooth(edge / domain.bankWidth) : surface - depth;
    if (candidate < bed) {
      bed = candidate;
      level = surface;
      u = vx;
      v = vz;
    }
  };
  for (const basin of domain.basins) {
    const px = (x - basin.center[0]) / basin.radii[0],
      pz = (z - basin.center[1]) / basin.radii[1];
    const angle = Math.atan2(pz, px);
    const radius = Math.hypot(px, pz) / (1 + 0.045 * Math.sin(angle * 3 + 0.7) + 0.025 * Math.sin(angle * 7));
    include(
      basin.level ?? water.level,
      basin.depth * Math.max(0, 1 - radius * radius),
      (radius - 1) * Math.min(...basin.radii),
      0,
      0,
    );
  }
  const points = water.flow?.river?.points ?? [];
  const speed = Math.hypot(...(water.flow?.velocity ?? [0, 0]));
  for (let i = 1; i < points.length; i++) {
    const a = points[i - 1],
      b = points[i],
      dx = b.position[0] - a.position[0],
      dz = b.position[1] - a.position[1];
    const length = Math.hypot(dx, dz);
    const t = Math.max(
      0,
      Math.min(1, ((x - a.position[0]) * dx + (z - a.position[1]) * dz) / (length * length)),
    );
    const distance = Math.hypot(x - a.position[0] - t * dx, z - a.position[1] - t * dz);
    const half = (a.width + (b.width - a.width) * t) * 0.5;
    const cross = Math.max(0, 1 - (distance / half) ** 2);
    const surface = (a.level ?? water.level) * (1 - t) + (b.level ?? water.level) * t;
    include(
      surface,
      (a.depth * (1 - t) + b.depth * t) * cross,
      distance - half,
      (dx / length) * speed * cross,
      (dz / length) * speed * cross,
    );
  }
  for (const obstacle of domain.obstacles) {
    const px = x - obstacle.center[0],
      pz = z - obstacle.center[1],
      yaw = obstacle.yaw ?? 0;
    const a = (px * Math.cos(yaw) + pz * Math.sin(yaw)) / (obstacle.aspect ?? 1);
    const b = -px * Math.sin(yaw) + pz * Math.cos(yaw);
    const angle = Math.atan2(b, a);
    const d =
      Math.hypot(a, b) /
      (obstacle.radius * (1 + 0.12 * Math.sin(angle * 3 + 0.8) + 0.06 * Math.cos(angle * 5)));
    bed += obstacle.height * Math.exp(-2.4 * d ** 4);
  }
  // Bounded relief shared by collision, render contact and the fluid's sampled bed.
  const detail = domain.bedDetail ?? 0;
  bed += detail * (1.3 * bedNoise(x * 7.3, z * 7.3) + 0.5 * bedNoise(x * 2.1, z * 2.1) - 0.9);
  return { bed, level, u, v };
}
export function compileWaterDomain(water: WaterDefinition): CompiledWaterDomain | undefined {
  const source = water.domain;
  if (!source) return;
  if (!source.basins.length && !water.flow?.river)
    throw new Error("A bounded water domain needs a basin or river");
  const key = contentKey({
    version: 3,
    domain: source,
    river: water.flow?.river,
    velocity: water.flow?.velocity,
    level: water.level,
  });
  const previous = cache.get(key);
  if (previous) return previous;
  const n = source.resolution,
    spacing: [number, number] = [source.size[0] / (n - 1), source.size[1] / (n - 1)];
  const cells = new Float32Array(n * n * 4);
  for (let row = 0; row < n; row++)
    for (let col = 0; col < n; col++) {
      const p = waterBedAt(water, source.min[0] + col * spacing[0], source.min[1] + row * spacing[1]);
      cells.set([p.bed, p.level, p.u, p.v], (row * n + col) * 4);
    }
  const rn = source.renderResolution ?? n;
  const sx = source.size[0] / (rn - 1),
    sz = source.size[1] / (rn - 1);
  const contact = new Float32Array(rn * rn * 4);
  const bedPositions = new Float32Array(rn * rn * 3),
    positions = new Float32Array(rn * rn * 3);
  const colors = new Float32Array(rn * rn * 3),
    normals = new Float32Array(rn * rn * 3),
    bedNormals = new Float32Array(rn * rn * 3);
  const indices = new Uint32Array((rn - 1) ** 2 * 6);
  let lo = Infinity,
    hi = -Infinity,
    index = 0;
  for (let row = 0; row < rn; row++)
    for (let col = 0; col < rn; col++) {
      const i = row * rn + col,
        x = source.min[0] + col * sx,
        z = source.min[1] + row * sz;
      const sample = waterBedAt(water, x, z);
      const depth = sample.level - sample.bed;
      contact.set(
        [
          sample.bed,
          sample.level,
          depth,
          Math.max(0, Math.hypot(sample.u, sample.v) / Math.sqrt(9.81 * Math.max(0.03, depth)) - 0.55),
        ],
        i * 4,
      );
      bedPositions.set([col * sx, sample.bed, row * sz], i * 3);
      positions.set([col * sx, sample.level, row * sz], i * 3);
      normals[i * 3 + 1] = 1;
      const wet = smooth((depth + 0.16) / 0.45);
      const grain = 0.86 + 0.14 * Math.sin(x * 2.7 + Math.sin(z * 3.3)) * Math.cos(z * 2.2);
      const moss = smooth((-depth - 0.1) / 0.65) * smooth((bedNoise(x * 0.7, z * 0.7) - 0.22) / 0.4);
      colors.set(
        [
          (1 - wet * 0.3) * grain * (1 - moss * 0.35),
          (1 - wet * 0.25) * grain * (1 + moss * 0.25),
          (1 - wet * 0.15) * grain * (1 - moss * 0.48),
        ],
        i * 3,
      );
      lo = Math.min(lo, sample.bed);
      hi = Math.max(hi, sample.bed, sample.level);
      if (row < rn - 1 && col < rn - 1) {
        indices.set([i, i + rn, i + 1, i + 1, i + rn, i + rn + 1], index);
        index += 6;
      }
    }
  for (let row = 0; row < rn; row++)
    for (let col = 0; col < rn; col++) {
      const x0 = Math.max(0, col - 1),
        x1 = Math.min(rn - 1, col + 1);
      const z0 = Math.max(0, row - 1),
        z1 = Math.min(rn - 1, row + 1);
      const dx = (contact[(row * rn + x1) * 4] - contact[(row * rn + x0) * 4]) / ((x1 - x0) * sx);
      const dz = (contact[(z1 * rn + col) * 4] - contact[(z0 * rn + col) * 4]) / ((z1 - z0) * sz);
      const length = Math.hypot(dx, 1, dz);
      bedNormals.set([-dx / length, 1 / length, -dz / length], (row * rn + col) * 3);
      contact[(row * rn + col) * 4 + 2] /= Math.max(0.08, Math.hypot(dx, dz));
    }
  const tileN = Math.ceil(n / 8),
    tiles = new Float32Array(tileN * tileN * 4);
  for (let tz = 0; tz < tileN; tz++)
    for (let tx = 0; tx < tileN; tx++) {
      let low = Infinity,
        high = -Infinity,
        minDepth = Infinity,
        maxDepth = -Infinity;
      for (let z = tz * 8; z < Math.min(n, (tz + 1) * 8 + 1); z++)
        for (let x = tx * 8; x < Math.min(n, (tx + 1) * 8 + 1); x++) {
          const i = (z * n + x) * 4,
            depth = cells[i + 1] - cells[i];
          low = Math.min(low, cells[i]);
          high = Math.max(high, cells[i]);
          minDepth = Math.min(minDepth, depth);
          maxDepth = Math.max(maxDepth, depth);
        }
      tiles.set([low, high, minDepth, maxDepth], (tz * tileN + tx) * 4);
    }
  const bounds = { min: [0, lo, 0] as Vec3, max: [source.size[0], hi, source.size[1]] as Vec3 };
  // Fluid elevation is piecewise linear on this coarse grid. Fine shoreline
  // silhouettes come from the contact field in the fragment stage, so bank
  // detail must not multiply the expensive wave solve at every water vertex.
  let surface: MeshData = { positions, normals, indices, bounds };
  if (rn !== n) {
    const p = new Float32Array(n * n * 3),
      normal = new Float32Array(n * n * 3),
      faces = new Uint32Array((n - 1) ** 2 * 6);
    let at = 0;
    for (let row = 0; row < n; row++)
      for (let col = 0; col < n; col++) {
        const i = row * n + col;
        p.set([col * spacing[0], cells[i * 4 + 1], row * spacing[1]], i * 3);
        normal[i * 3 + 1] = 1;
        if (row < n - 1 && col < n - 1) {
          faces.set([i, i + n, i + 1, i + 1, i + n, i + n + 1], at);
          at += 6;
        }
      }
    surface = { positions: p, normals: normal, indices: faces, bounds };
  }
  const result: CompiledWaterDomain = {
    key,
    min: [...source.min],
    size: [...source.size],
    resolution: n,
    spacing,
    cells,
    contact,
    renderResolution: rn,
    tiles,
    surface,
    bed: { positions: bedPositions, normals: bedNormals, colors, indices, bounds },
  };
  if (cache.size >= 16) cache.delete(cache.keys().next().value as string);
  cache.set(key, result);
  return result;
}
/** Piecewise-linear sampling matches the bed and water triangle diagonals on the GPU. */
export function sampleWaterGrid(
  domain: CompiledWaterDomain,
  data: Float32Array,
  x: number,
  z: number,
): [number, number, number, number] | undefined {
  const n = domain.resolution,
    px = (x - domain.min[0]) / domain.spacing[0],
    pz = (z - domain.min[1]) / domain.spacing[1];
  if (px < 0 || pz < 0 || px > n - 1 || pz > n - 1) return;
  const ix = Math.min(n - 2, Math.floor(px)),
    iz = Math.min(n - 2, Math.floor(pz)),
    fx = px - ix,
    fz = pz - iz;
  const result: [number, number, number, number] = [0, 0, 0, 0];
  for (let j = 0; j < 2; j++)
    for (let i = 0; i < 2; i++) {
      const weight =
          fx + fz <= 1
            ? j
              ? i
                ? 0
                : fz
              : i
                ? fx
                : 1 - fx - fz
            : j
              ? i
                ? fx + fz - 1
                : 1 - fx
              : i
                ? 1 - fz
                : 0,
        offset = ((iz + j) * n + ix + i) * 4;
      for (let k = 0; k < 4; k++) result[k] += data[offset + k] * weight;
    }
  return result;
}
/** Optical contact uses the same fine triangles as the bed, independent of fluid resolution. */
export function sampleWaterContact(domain: CompiledWaterDomain, x: number, z: number) {
  return sampleWaterGrid(
    {
      ...domain,
      resolution: domain.renderResolution,
      spacing: [
        domain.size[0] / (domain.renderResolution - 1),
        domain.size[1] / (domain.renderResolution - 1),
      ],
    },
    domain.contact,
    x,
    z,
  );
}
/** Nested annuli give the ocean a dense near field and bounded horizon geometry. */
export function oceanWaterMesh(): MeshData {
  const radii = [0.05];
  for (let r = 0.5; r <= 16; r += 0.5) radii.push(r);
  for (let r = 18; r <= 64; r += 2) radii.push(r);
  for (let r = 72; r <= 256; r += 8) radii.push(r);
  for (let r = 288; r <= 1024; r += 32) radii.push(r);
  for (let r = 1152; r <= 8192; r *= 1.25) radii.push(r);
  const count = 192,
    positions: number[] = [],
    normals: number[] = [],
    colors: number[] = [],
    indices: number[] = [];
  for (let ring = 0; ring < radii.length; ring++)
    for (let i = 0; i < count; i++) {
      const angle = (i * Math.PI * 2) / count,
        r = radii[ring];
      positions.push(Math.cos(angle) * r, 0, Math.sin(angle) * r);
      normals.push(0, 1, 0);
      const spacing = Math.max((r * Math.PI * 2) / count, r - (radii[ring - 1] ?? 0));
      colors.push(spacing, 0, 0);
      if (ring) {
        const a = (ring - 1) * count + i,
          b = (ring - 1) * count + ((i + 1) % count),
          c = ring * count + i,
          d = ring * count + ((i + 1) % count);
        indices.push(a, b, c, b, d, c);
      }
    }
  return {
    positions: new Float32Array(positions),
    normals: new Float32Array(normals),
    colors: new Float32Array(colors),
    indices: new Uint32Array(indices),
    bounds: { min: [-8192, 0, -8192], max: [8192, 0, 8192] },
  };
}
