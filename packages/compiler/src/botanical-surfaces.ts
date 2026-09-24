import { add, cross, normalize, scale, sub, type Vec3 } from "@wrela/model";

import { type BotanicalBranch, botanicalBranchPoint } from "./botanical-branch";
import { type Builder, vertex } from "./botanical-primitives";

/** Shared rings remove internal caps and keep continuous normals along bent wood.
 * Flare/collars overlap supporting wood; this is not a claim of a watertight union. */
export function botanicalWood(
  mesh: Builder,
  branch: BotanicalBranch,
  sides: number,
  segments: number,
  source: string,
  species: "pine" | "birch",
  age = 1,
) {
  const base = mesh.positions.length / 3;
  const length = Math.max(0.0001, Math.hypot(...sub(branch.end, branch.start)));
  const tip = branch.tipRadius ?? Math.max(0.0006, branch.radius * 0.08);
  let across: Vec3 | undefined;
  for (let row = 0; row <= segments; row++) {
    const t = (row / segments) ** (branch.level < 2 ? 1.5 : 1);
    const center = botanicalBranchPoint(branch, t);
    const tangent = normalize(
      sub(
        botanicalBranchPoint(branch, Math.min(1, t + 0.02)),
        botanicalBranchPoint(branch, Math.max(0, t - 0.02)),
      ),
    );
    // Project the previous ring frame into the new normal plane (parallel transport).
    across = across
      ? normalize(
          sub(
            across,
            scale(tangent, across[0] * tangent[0] + across[1] * tangent[1] + across[2] * tangent[2]),
          ),
        )
      : normalize(cross(tangent, Math.abs(tangent[1]) > 0.9 ? [1, 0, 0] : [0, 1, 0]));
    const around = cross(tangent, across);
    const collarScale = branch.parent
      ? Math.max(0.015, branch.radius * 3.5)
      : Math.max(0.12, branch.radius * 4);
    const collar = (branch.parent ? 0.5 : 0.7) * Math.exp((-t * length) / collarScale);
    const radius = (branch.radius * (1 - t) + tip * t) * (1 + collar);
    for (let i = 0; i <= sides; i++) {
      const angle = (i / sides) * Math.PI * 2;
      const radial = add(scale(across, Math.cos(angle)), scale(around, Math.sin(angle)));
      const ridge =
        species === "pine"
          ? Math.min(0.0015, radius * 0.04) * age * Math.sin(angle * 5 + t * 2)
          : Math.min(0.0004, radius * 0.02) * Math.sin(angle * 7 + t * 3);
      const slope =
        ((branch.radius - tip) / length) * (1 + collar) +
        ((branch.radius * (1 - t) + tip * t) * collar) / collarScale;
      const normal = normalize(add(radial, scale(tangent, slope)));
      const shade = 0.92 + 0.08 * Math.sin(angle * 3 + t * 4) ** 2;
      // Young birch twigs retain brown bark. Diameter is a provisional surface
      // maturity cue, not a calibrated mapping from developmental steps to years.
      const mature = species === "birch" ? Math.min(1, Math.max(0, (radius - 0.006) / 0.02)) : 1;
      const color: Vec3 = [0.47 + mature * 0.53, 0.27 + mature * 0.73, 0.15 + mature * 0.85].map(
        (value) => value * shade,
      ) as Vec3;
      vertex(mesh, add(center, scale(radial, radius + ridge)), normal, color, source);
      mesh.materialCoordinates ??= [];
      mesh.materialCoordinates.push(angle * branch.radius, t * length, 0);
    }
  }
  for (let row = 0; row < segments; row++)
    for (let i = 0; i < sides; i++) {
      const a = base + row * (sides + 1) + i,
        b = a + sides + 1;
      mesh.indices.push(a, a + 1, b, a + 1, b + 1, b);
    }
  // Only the exposed ends are capped; an internal attachment is concealed by its parent.
  if (!branch.parent || branch.broken) {
    const end = branch.broken ? segments : 0;
    const center = vertex(
      mesh,
      botanicalBranchPoint(branch, end / segments),
      scale(normalize(sub(branch.end, branch.start)), end ? 1 : -1),
      [0.75, 0.7, 0.6],
      source,
    );
    if (!mesh.materialCoordinates) throw Error("Missing wood surface coordinates");
    mesh.materialCoordinates.push(0, end ? length : 0, 0);
    for (let i = 0; i < sides; i++) {
      const a = base + end * (sides + 1) + i;
      mesh.indices.push(center, end ? a : a + 1, end ? a + 1 : a);
    }
  }
}
