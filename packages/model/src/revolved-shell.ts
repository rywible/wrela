import { z } from "zod";

/** A closed meridian in radius/height coordinates produces a hollow, watertight shell.
 * Profile corners are intentional creases; angular tessellation does not change the source. */
export const revolvedShellSchema = z
  .object({
    profile: z
      .array(z.tuple([z.number().finite().min(0.0001).max(500), z.number().finite().min(-1000).max(1000)]))
      .min(3)
      .max(64),
    segments: z.number().int().min(12).max(96).default(48),
  })
  .superRefine(({ profile }, ctx) => {
    const cross = (a: number[], b: number[], c: number[]) =>
      (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    let area = 0;
    for (let i = 0; i < profile.length; i++) {
      const a = profile[i],
        b = profile[(i + 1) % profile.length];
      area += a[0] * b[1] - a[1] * b[0];
      if (Math.hypot(a[0] - b[0], a[1] - b[1]) < 1e-6)
        ctx.addIssue({ code: "custom", message: "Shell profile contains a zero-length edge" });
      for (let j = i + 2; j < profile.length; j++) {
        if (i === 0 && j === profile.length - 1) continue;
        const c = profile[j],
          d = profile[(j + 1) % profile.length];
        const side = (p: number[], q: number[], r: number[]) =>
          Math.abs(cross(p, q, r)) < 1e-10 &&
          r[0] >= Math.min(p[0], q[0]) - 1e-10 &&
          r[0] <= Math.max(p[0], q[0]) + 1e-10 &&
          r[1] >= Math.min(p[1], q[1]) - 1e-10 &&
          r[1] <= Math.max(p[1], q[1]) + 1e-10;
        if (
          (cross(a, b, c) * cross(a, b, d) < 0 && cross(c, d, a) * cross(c, d, b) < 0) ||
          side(a, b, c) ||
          side(a, b, d) ||
          side(c, d, a) ||
          side(c, d, b)
        )
          ctx.addIssue({ code: "custom", message: "Shell meridian must be a simple closed polygon" });
      }
    }
    if (Math.abs(area) < 1e-9)
      ctx.addIssue({ code: "custom", message: "Shell meridian must enclose nonzero material thickness" });
  });
export type RevolvedShell = z.infer<typeof revolvedShellSchema>;

export function revolvedShellRings(input: RevolvedShell): [number, number, number][][] {
  const shell = revolvedShellSchema.parse(input),
    profile = [...shell.profile];
  const area = profile.reduce((n, a, i) => {
    const b = profile[(i + 1) % profile.length];
    return n + a[0] * b[1] - a[1] * b[0];
  }, 0);
  if (area < 0) profile.reverse();
  return profile.map(([r, y]) =>
    Array.from({ length: shell.segments }, (_, i) => {
      const angle = (-i * Math.PI * 2) / shell.segments;
      return [r * Math.cos(angle), y, r * Math.sin(angle)];
    }),
  );
}
