import type {
  CompiledRenderProduct,
  EvaluatedScene,
  RenderCostObservation,
  RenderSurface,
} from "@wrela/model";

import { cameraBasis } from "./math";
import type { RenderQuality } from "./quality";
import type { AtmosphereRuntimeTable } from "./transport";

export const RENDER_KERNEL_VERSION = "render-products-8";
export type RenderCompilerOptions = {
  /** Auto is the production default; explicit representations are reproducible benchmark controls. */
  geometry?: "direct" | "parametric" | "analytic" | "auto";
  water?: "auto" | "direct" | "regular" | "reference";
  shutterSeconds?: number;
  visibility?: boolean;
  finiteSun?: { angularRadius: number };
  atmosphere?: AtmosphereRuntimeTable;
  maxGeometryErrorPixels?: number;
  costs?: readonly RenderCostObservation[];
};
export type RealizationDecision = {
  id: string;
  kind: CompiledRenderProduct["kind"];
  key: string;
  reason: string;
  rejections: { key: string; reason: string }[];
};

type GeometryProjection = {
  scale: number;
  nearest: number;
  lateral: number;
  focal: number;
  radius: number;
  height: number;
};
function projectionForSurface(
  scene: EvaluatedScene,
  surface: RenderSurface,
  height: number,
  basis = cameraBasis(scene.camera),
): GeometryProjection {
  const m = surface.matrix,
    bounds = surface.mesh.bounds;
  let squaredScale = 0;
  for (let column = 0; column < 3; column++) {
    let rowSum = 0;
    for (let other = 0; other < 3; other++)
      rowSum += Math.abs(
        m[column * 4] * m[other * 4] +
          m[column * 4 + 1] * m[other * 4 + 1] +
          m[column * 4 + 2] * m[other * 4 + 2],
      );
    squaredScale = Math.max(squaredScale, rowSum);
  }
  let nearest = Infinity,
    lateral = 0;
  for (let corner = 0; corner < 8; corner++) {
    const x = bounds[corner & 1 ? "max" : "min"][0],
      y = bounds[corner & 2 ? "max" : "min"][1],
      z = bounds[corner & 4 ? "max" : "min"][2];
    const px = m[0] * x + m[4] * y + m[8] * z + m[12] - scene.camera.position[0],
      py = m[1] * x + m[5] * y + m[9] * z + m[13] - scene.camera.position[1],
      pz = m[2] * x + m[6] * y + m[10] * z + m[14] - scene.camera.position[2];
    nearest = Math.min(nearest, px * basis.forward[0] + py * basis.forward[1] + pz * basis.forward[2]);
    lateral = Math.max(
      lateral,
      Math.hypot(
        px * basis.right[0] + py * basis.right[1] + pz * basis.right[2],
        px * basis.up[0] + py * basis.up[1] + pz * basis.up[2],
      ),
    );
  }
  const ex = (bounds.max[0] - bounds.min[0]) * 0.5,
    ey = (bounds.max[1] - bounds.min[1]) * 0.5,
    ez = (bounds.max[2] - bounds.min[2]) * 0.5;
  const radius = Math.hypot(
    Math.abs(m[0]) * ex + Math.abs(m[4]) * ey + Math.abs(m[8]) * ez,
    Math.abs(m[1]) * ex + Math.abs(m[5]) * ey + Math.abs(m[9]) * ez,
    Math.abs(m[2]) * ex + Math.abs(m[6]) * ey + Math.abs(m[10]) * ez,
  );
  return {
    scale: Math.sqrt(squaredScale),
    nearest,
    lateral,
    radius,
    focal: height / (2 * Math.tan((scene.camera.fov * Math.PI) / 360)),
    height,
  };
}
function projectedError(projection: GeometryProjection, error: number): number {
  if (!Number.isFinite(error) || error < 0 || !Number.isFinite(projection.height) || projection.height <= 0)
    return Infinity;
  if (error === 0) return 0;
  const displacement = error * projection.scale;
  if (
    projection.nearest - displacement <= 0.05 ||
    !Number.isFinite(projection.nearest + projection.lateral + displacement)
  )
    return Infinity;
  return (
    (projection.focal * displacement * (1 + projection.lateral / projection.nearest)) /
    (projection.nearest - displacement)
  );
}
/** Conservative projection Lipschitz bound. The shared per-instance projection is reused by every candidate. */
export function projectedGeometryError(
  scene: EvaluatedScene,
  surface: RenderSurface,
  error: number,
  viewportHeight: number,
): number {
  return projectedError(projectionForSurface(scene, surface, viewportHeight), error);
}
function linearCondition(m: Float32Array): number {
  if (m.length !== 16 || !m.every(Number.isFinite) || m[3] !== 0 || m[7] !== 0 || m[11] !== 0 || m[15] !== 1)
    return Infinity;
  const a = m[0],
    b = m[4],
    c = m[8],
    d = m[1],
    e = m[5],
    f = m[9],
    g = m[2],
    h = m[6],
    i = m[10];
  const c0 = e * i - f * h,
    c1 = f * g - d * i,
    c2 = d * h - e * g,
    c3 = c * h - b * i,
    c4 = a * i - c * g,
    c5 = b * g - a * h,
    c6 = b * f - c * e,
    c7 = c * d - a * f,
    c8 = a * e - b * d;
  const determinant = a * c0 + b * c1 + c * c2;
  return determinant === 0
    ? Infinity
    : (Math.hypot(a, b, c, d, e, f, g, h, i) * Math.hypot(c0, c1, c2, c3, c4, c5, c6, c7, c8)) /
        Math.abs(determinant);
}

function assumptionRejection(
  surface: RenderSurface,
  product: CompiledRenderProduct,
  condition: number,
): string | undefined {
  if (surface.skin || surface.deformation || surface.wind || surface.water) return "deformed surface";
  if (!Number.isFinite(condition)) return "singular or nonfinite transform";
  if (
    surface.drawRange &&
    (surface.drawRange.start !== 0 || surface.drawRange.count !== surface.mesh.indices.length)
  )
    return "partial material range";
  for (const assumption of product.assumptions) {
    switch (assumption.kind) {
      case "opaque":
      case "rigid":
        break;
      case "matrix-condition": {
        if (product.kind !== "analytic-quadric") return "unsupported matrix condition domain";
        const radii = product.primitive.radii;
        const primitiveCondition =
          Math.hypot(...radii) * Math.hypot(1 / radii[0], 1 / radii[1], 1 / radii[2]);
        if (condition * primitiveCondition > assumption.maximum) return "ill-conditioned transform";
        break;
      }
      case "roughness-range":
        if (
          surface.material.roughness < assumption.minimum ||
          surface.material.roughness > assumption.maximum
        )
          return "roughness outside product domain";
        break;
      default:
        return `unsupported ${assumption.kind} assumption`;
    }
  }
  return;
}

/** Cold-adapter scheduling estimates work, rather than inventing timing evidence.
 * Both raster passes transform mesh vertices; quadric ray tests scale with proxy
 * area. Current adapter measurements replace this estimate when comparable. */
export function geometryWorkEstimate(
  scene: EvaluatedScene,
  surface: RenderSurface,
  product: CompiledRenderProduct,
  viewportHeight: number,
  projection = projectionForSurface(scene, surface, viewportHeight),
  rasterSamples = 1,
): number {
  if (product.kind !== "analytic-quadric") {
    const mesh = product.kind === "parametric-mesh" ? product.mesh : surface.mesh;
    return 2 * (mesh.positions.length / 3 + mesh.indices.length / 3);
  }
  const { radius, nearest, focal } = projection;
  const cameraPixels =
    nearest > 0.05
      ? Math.min(viewportHeight ** 2 * 2, 4 * ((focal * radius) / nearest) ** 2)
      : viewportHeight ** 2 * 2;
  // Coefficient units are work, not milliseconds. The fixed term includes CPU
  // matrix/proxy preparation and storage traffic; the pixel term covers ray tests.
  const shadowPixels = (radius * 16) ** 2;
  return 1200 + 3 * (cameraPixels * rasterSamples + shadowPixels);
}

export function selectRenderProducts(
  scene: EvaluatedScene,
  viewportHeight: number,
  options: RenderCompilerOptions = {},
  adapter = "",
  browser = "",
  rasterSamples = 1,
): { scene: EvaluatedScene; decisions: RealizationDecision[] } {
  const mode = options.geometry ?? "auto";
  const requestedBudget = options.maxGeometryErrorPixels ?? 0.25;
  const budget = Number.isFinite(requestedBudget) && requestedBudget >= 0 ? requestedBudget : -1;
  const decisions: RealizationDecision[] = [];
  const basis = cameraBasis(scene.camera);
  const surfaces = scene.surfaces.map((surface): RenderSurface => {
    const products = surface.renderProducts;
    if (!products?.length) return surface;
    const direct = products.find((product) => product.kind === "direct-mesh");
    if (!direct) return surface; // Malformed optional metadata cannot remove the existing geometry.
    const rejections: RealizationDecision["rejections"] = [];
    let selected: CompiledRenderProduct = direct;
    let reason = "primary geometry";
    const eligible: CompiledRenderProduct[] = [];
    const condition = linearCondition(surface.matrix);
    const projection = projectionForSurface(scene, surface, viewportHeight, basis);
    for (const product of products) {
      if (product.kind === "direct-mesh" && mode !== "auto") continue;
      let rejection = assumptionRejection(surface, product, condition);
      if (
        product.sourceKey !== direct.sourceKey ||
        (product.kind !== "direct-mesh" && product.fallbackKey !== direct.key)
      )
        rejection = "stale source or fallback identity";
      if (mode === "direct") rejection = "direct control requested";
      if (mode === "analytic" && product.kind !== "analytic-quadric")
        rejection = "analytic control requested";
      if (mode === "parametric" && product.kind !== "parametric-mesh")
        rejection = "parametric control requested";
      const evidence = product.errors.find(
        (error) =>
          error.kind !== "unknown" &&
          (product.kind === "analytic-quadric" ? error.metric === "depth" : error.metric === "silhouette"),
      );
      if (
        !rejection &&
        (!evidence || evidence.kind === "unknown" || projectedError(projection, evidence.maximum) > budget)
      )
        rejection = "geometry error exceeds budget or lacks required evidence";
      if (rejection) rejections.push({ key: product.key, reason: rejection });
      else eligible.push(product);
    }
    if (mode === "auto" && eligible.length) {
      const costs = options.costs?.filter(
        (sample) =>
          sample.adapter === adapter &&
          sample.browser === browser &&
          sample.kernelVersion === RENDER_KERNEL_VERSION &&
          Number.isFinite(sample.gpuP95Ms) &&
          sample.gpuP95Ms >= 0 &&
          Number.isFinite(sample.preparationMs) &&
          sample.preparationMs >= 0,
      );
      const baseline = costs?.find((sample) => sample.productKey === direct.key);
      const matched = baseline?.fixture
        ? eligible.map((product) => ({
            product,
            sample: costs?.find(
              (sample) => sample.productKey === product.key && sample.fixture === baseline.fixture,
            ),
          }))
        : [];
      // A measured comparison is meaningful only when every admissible competitor
      // was observed on the same fixture, adapter, browser and kernel revision.
      if (matched.length === eligible.length && matched.every(({ sample }) => sample !== undefined)) {
        matched.sort(
          (a, b) =>
            (a.sample?.gpuP95Ms ?? Infinity) +
            (a.sample?.preparationMs ?? Infinity) -
            ((b.sample?.gpuP95Ms ?? Infinity) + (b.sample?.preparationMs ?? Infinity)),
        );
        selected = matched[0].product;
        reason = "lowest measured pass and preparation cost within projected geometric budget";
      } else {
        eligible.sort(
          (a, b) =>
            geometryWorkEstimate(scene, surface, a, viewportHeight, projection, rasterSamples) -
            geometryWorkEstimate(scene, surface, b, viewportHeight, projection, rasterSamples),
        );
        selected = eligible[0];
        reason = "projected geometry work model; real-arithmetic error bound, numeric roundoff unmeasured";
      }
    } else if (eligible.length) {
      eligible.sort((a, b) => a.byteLength - b.byteLength);
      selected = eligible[0];
      reason = "explicit representation control within geometric budget";
    }
    decisions.push({ id: surface.id, kind: selected.kind, key: selected.key, reason, rejections });
    return {
      ...surface,
      selectedRenderProduct: selected,
      ...(selected.kind === "parametric-mesh"
        ? {
            mesh: selected.mesh,
            drawRange: undefined,
            details: undefined,
            ...(surface.reliefAppearance && selected.algorithmVersion.startsWith("surface-relief-coarse")
              ? {
                  reliefAppearance: {
                    ...surface.reliefAppearance,
                    geometryWeights: [0, 0, 0] as [number, number, number],
                    residualWeights: [1, 1, 1] as [number, number, number],
                  },
                }
              : {}),
          }
        : {}),
    };
  });
  return { scene: { ...scene, surfaces }, decisions };
}

export function selectWaterAppearance(
  scene: EvaluatedScene,
  options: RenderCompilerOptions = {},
  quality: RenderQuality = "balanced",
) {
  const eligible = (surface: RenderSurface) =>
    surface.material.pattern === 0 &&
    !surface.material.layers?.length &&
    surface.material.normalStrength === 0;
  const requestedMode = options.water ?? "auto";
  const requestedShutter = options.shutterSeconds ?? 1 / 60;
  const shutterSeconds =
    Number.isFinite(requestedShutter) && requestedShutter >= 0 ? Math.min(4, requestedShutter) : 1 / 60;
  const reason = {
    auto: "automatic per-pixel footprint integration with conditioning guards",
    regular: "fixed footprint and shutter quadrature control",
    direct: "single complete response control",
    reference: "high-sample correlated response reference",
  };
  return {
    scene: {
      ...scene,
      surfaces: scene.surfaces.map((surface) =>
        surface.water
          ? {
              ...surface,
              waterAppearance: {
                mode: eligible(surface) ? requestedMode : "direct",
                shutterSeconds,
                quality,
              },
            }
          : surface,
      ),
    },
    decisions: scene.surfaces
      .filter((surface) => surface.water)
      .map((surface) => ({
        id: surface.id,
        kind: surface.waterState ? ("spectrum" as const) : eligible(surface) ? requestedMode : "direct",
        reason: surface.waterState
          ? "compiled directional carriers with geometric filtering and residual slope variance"
          : eligible(surface)
            ? reason[requestedMode]
            : "layered or procedural source uses its complete direct response",
      })),
  };
}
