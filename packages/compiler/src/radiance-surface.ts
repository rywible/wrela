import type { RadianceLightingField, Vec3 } from "@wrela/model";
import { type IndirectGeometry, indirectDot } from "./indirect-query";
import { traceRadianceSampleSteps } from "./radiance-transport";

/** Research surface quadrature: one fixed normal removes the nine-coefficient
 * output projection. Runtime integration/placement is deliberately separate. */
export function* traceDiffuseReceiverSteps(
  geometry: IndirectGeometry,
  position: Vec3,
  normal: Vec3,
  rays: number,
  lights: RadianceLightingField["lights"],
): Generator<void, { transfer: Float64Array; directEmission: Vec3; rays: number }> {
  if (!Number.isInteger(rays) || rays < 1 || rays > 1048576)
    throw Error("Invalid surface radiance sample count");
  const transfer = new Float64Array(27 * 4);
  const directEmission: Vec3 = [0, 0, 0];
  const count = yield* traceRadianceSampleSteps(
    geometry,
    position,
    0,
    rays,
    lights,
    new Float32Array(0),
    new Float32Array(0),
    undefined,
    undefined,
    undefined,
    {
      normal,
      project: false,
      observe: (_index, path) => {
        for (let i = 0; i < transfer.length; i++) transfer[i] += path.incident[i] / rays;
        if (path.directHit) for (let c = 0; c < 3; c++) directEmission[c] += path.directHit[c] / rays;
        if (path.emitter) {
          const cosine = Math.max(0, indirectDot(normal, path.emitter.direction)) / Math.PI;
          for (let c = 0; c < 3; c++) {
            const value = path.emitter.energy[c] * cosine;
            transfer[26 * 4 + c] += value;
            directEmission[c] += value;
          }
        }
      },
    },
  );
  return { transfer, directEmission, rays: count };
}
