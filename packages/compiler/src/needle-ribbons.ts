import { add, cross, normalize, scale, sub } from "@wrela/model";

import { type BotanicalBranch, botanicalBranchPoint } from "./botanical-branch";
import { type Builder, vertex } from "./botanical-primitives";

/** Three oriented curved ribbons retain source-shoot volume. Their actual leaf
 * boundary is a filtered semantic coverage field, shared at every detail level. */
export function needleRibbons(
  mesh: Builder,
  uv: number[],
  branch: BotanicalBranch,
  width: number,
  source: string,
  tint: number,
  segments = 3,
) {
  const sides = 3;
  const direction = normalize(sub(branch.end, branch.start));
  const lateral = normalize(cross(direction, Math.abs(direction[1]) > 0.95 ? [1, 0, 0] : [0, 1, 0]));
  const vertical = cross(direction, lateral);
  for (let side = 0; side < sides; side++) {
    const angle = (side * Math.PI) / sides;
    const across = add(scale(lateral, Math.cos(angle)), scale(vertical, Math.sin(angle)));
    const normal = normalize(cross(direction, across));
    const start = mesh.positions.length / 3;
    for (let row = 0; row <= segments; row++) {
      const t = row / segments;
      const center = botanicalBranchPoint(branch, t);
      for (let edge = 0; edge < 2; edge++) {
        vertex(mesh, add(center, scale(across, (edge ? 1 : -1) * width)), normal, [tint, tint, tint], source);
        uv.push(edge, t);
      }
    }
    for (let row = 0; row < segments; row++) {
      const a = start + row * 2;
      mesh.indices.push(a, a + 2, a + 1, a + 1, a + 2, a + 3);
    }
  }
}
