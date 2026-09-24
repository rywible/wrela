import type { RenderEnvironment } from "@wrela/model";

type Formation = NonNullable<NonNullable<RenderEnvironment["cloudscape"]>["formations"]>[number];
export const CLOUD_FORMATION_CAPACITY = 8;
export const CLOUD_LOBES_PER_FORMATION = 6;
export const CLOUD_FORMATION_HEADER_FLOATS = 8;
export const CLOUD_FORMATION_STRIDE = 16;
export const CLOUD_LOBE_OFFSET =
  CLOUD_FORMATION_HEADER_FLOATS + CLOUD_FORMATION_CAPACITY * CLOUD_FORMATION_STRIDE;
export const CLOUD_FORMATION_FLOATS =
  CLOUD_LOBE_OFFSET + CLOUD_FORMATION_CAPACITY * CLOUD_LOBES_PER_FORMATION * 8;
export const CLOUD_FORMATION_GLOBAL_OFFSET = 228;
export const CLOUD_FORMATION_FRAME_OFFSET = 60;
export const CLOUD_FRAME_FLOATS = CLOUD_FORMATION_FRAME_OFFSET + CLOUD_FORMATION_FLOATS;

type Lobe = { center: [number, number, number]; radius: [number, number, number]; activity: number };
/** Compile macro growth once per upload, not random branches at every ray sample.
 * All six lobes fit the same conservative box used by views, light and shadows.
 * The seed changes branch count/position/width as well as boundary turbulence. */
export function compileCloudGrowth(form: Formation): Lobe[] {
  let bits = form.seed + 0x9e3779b9;
  const random = () => {
    bits = (Math.imul(bits ^ (bits >>> 16), 0x21f0aaad) + 0x735a2d97) | 0;
    return (bits >>> 0) / 4294967296;
  };
  const maturity = form.maturity ?? 0.45;
  const shear = form.shear ?? 0;
  const side = random() < 0.5 ? -1 : 1;
  const lobes: Lobe[] = [];
  const add = (x: number, y: number, z: number, rx: number, ry: number, rz: number, activity: number) => {
    x += shear * y * y * 0.28;
    const radius: Lobe["radius"] = [rx, ry, rz];
    const center: Lobe["center"] = [x, y, z];
    // Keep the entire envelope inside support, including sheared crowns.
    for (const axis of [0, 2])
      center[axis] = Math.max(-0.97 + radius[axis], Math.min(0.97 - radius[axis], center[axis]));
    center[1] = Math.max(radius[1] * 0.7, Math.min(0.98 - radius[1], center[1]));
    lobes.push({ center, radius, activity });
  };
  if (form.kind === "wisp") {
    for (let i = 0; i < CLOUD_LOBES_PER_FORMATION; i++) {
      add(
        -0.65 + i * 0.25,
        0.25 + random() * 0.5,
        (random() - 0.5) * 0.48,
        0.12 + random() * 0.16,
        0.06 + random() * 0.1,
        0.18 + random() * 0.15,
        0.1,
      );
    }
  } else if (form.kind === "tower") {
    const stem = side * (0.1 + random() * 0.23);
    const width = 0.25 + random() * 0.13;
    add(0, 0.16, 0, 0.68 + random() * 0.18, 0.23, 0.58 + random() * 0.14, 0.25);
    add(stem * 0.65, 0.39, (random() - 0.5) * 0.22, width + 0.08, 0.32, 0.32 + random() * 0.1, 0.65);
    add(
      stem,
      0.73 + random() * 0.08,
      (random() - 0.5) * 0.26,
      width + maturity * 0.09,
      0.2 + random() * 0.05,
      0.27 + random() * 0.14,
      1,
    );
    add(
      -side * (0.22 + random() * 0.24),
      0.34 + random() * 0.18,
      (random() - 0.5) * 0.42,
      0.24 + random() * 0.13,
      0.22 + random() * 0.1,
      0.24 + random() * 0.1,
      0.7,
    );
    // A seed selects a secondary updraft or a detached, evaporating shoulder.
    const secondary = random() > 0.45;
    add(
      -side * (0.3 + random() * 0.18),
      secondary ? 0.7 : 0.38,
      (random() - 0.5) * 0.46,
      secondary ? 0.22 : 0.15,
      secondary ? 0.23 : 0.12,
      0.2 + random() * 0.09,
      secondary ? 0.9 : 0.12,
    );
    add(
      side * (0.48 + random() * 0.17),
      0.34 + random() * 0.18,
      (random() - 0.5) * 0.5,
      0.13 + maturity * 0.07,
      0.1 + random() * 0.06,
      0.16,
      0.08,
    );
  } else {
    // A bank is several unequal cells with openings, not a scaled tower.
    add(-0.08, 0.25, 0, 0.62 + random() * 0.18, 0.28, 0.53 + random() * 0.2, 0.3);
    for (let i = 1; i < CLOUD_LOBES_PER_FORMATION; i++) {
      const x = -0.7 + (i - 1) * 0.32 + (random() - 0.5) * 0.17;
      const top = 0.32 + random() * 0.4;
      add(
        x,
        top,
        (random() - 0.5) * 0.55,
        0.17 + random() * 0.17,
        0.16 + random() * 0.19,
        0.25 + random() * 0.19,
        i === 5 ? 0.1 : 0.4 + random() * 0.45,
      );
    }
  }
  return lobes;
}

/** Fixed upload shared by the view, light field, shadows and history. */
export function packCloudFormations(cloudscape: RenderEnvironment["cloudscape"]): Float32Array {
  const data = new Float32Array(CLOUD_FORMATION_FLOATS);
  const forms = cloudscape?.formations ?? [];
  if (forms.length > CLOUD_FORMATION_CAPACITY) throw new RangeError("Too many runtime cloud formations");
  data.set([
    forms.length,
    cloudscape?.background ?? 1,
    0,
    cloudscape?.highCloudYaw ?? 0.4324078,
    cloudscape?.midCloudCover ?? 0,
    cloudscape?.midCloudHeight ?? 6800,
    cloudscape?.midCloudYaw ?? -0.35,
    cloudscape?.highCloudHeight ?? 9800,
  ]);
  forms.forEach((f, i) => {
    if (f.density > 0) data[2] = Math.max(data[2], f.base + f.size[1]);
    data.set(
      [
        f.center[0],
        f.base,
        f.center[1],
        ["tower", "bank", "wisp"].indexOf(f.kind),
        ...f.size,
        f.density,
        Math.cos(f.yaw),
        Math.sin(f.yaw),
        f.erosion,
        f.seed,
        f.maturity ?? 0.45,
        f.shear ?? 0,
        0,
        0,
      ],
      CLOUD_FORMATION_HEADER_FLOATS + i * CLOUD_FORMATION_STRIDE,
    );
    compileCloudGrowth(f).forEach((lobe, j) => {
      data.set(
        [...lobe.center, lobe.activity, ...lobe.radius, 0],
        CLOUD_LOBE_OFFSET + (i * CLOUD_LOBES_PER_FORMATION + j) * 8,
      );
    });
  });
  return data;
}
