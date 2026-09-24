import { z } from "zod";
import { add, cross, dot, normalize, scale, sub, type Vec3 } from "./math";
import { revolvedShellSchema } from "./revolved-shell";

/** Fits cooked mesh's 250,000-vertex budget even with flat triangle normals. */
export const ASSEMBLY_MAX_TRIANGLES = 80_000;

const finite = z.number().finite();
const id = z
  .string()
  .min(1)
  .max(100)
  .regex(/^[a-zA-Z0-9_-]+$/);
const coordinate = finite.min(-1_000_000).max(1_000_000);
const vector = z.tuple([coordinate, coordinate, coordinate]);
const point = z.tuple([coordinate, coordinate]);
export const assemblyProfileSchema = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("rectangle"),
    width: finite.positive().max(1000),
    height: finite.positive().max(1000),
  }),
  z.object({
    kind: z.literal("circle"),
    radius: finite.positive().max(500),
    segments: z.number().int().min(6).max(64),
  }),
  z.object({ kind: z.literal("polygon"), points: z.array(point).min(3).max(64) }),
]);
const joint = z
  .object({
    kind: z.enum(["hinge", "slider"]),
    axis: vector.refine((v) => Math.hypot(...v) > 1e-6, "Joint axis must be nonzero"),
    pivot: vector,
    minimum: finite,
    maximum: finite,
    value: finite,
    drive: z.object({ period: finite.min(0.1).max(3600), phase: finite.min(0).max(1) }).optional(),
    link: z.object({ part: id, ratio: finite.min(-1000).max(1000), offset: finite }).optional(),
  })
  .refine(
    (j) => j.minimum <= j.maximum && j.value >= j.minimum && j.value <= j.maximum,
    "Joint value must lie inside ordered limits",
  );
export const assemblyPartSchema = z.object({
  id,
  name: z.string().min(1).max(120),
  parent: id.optional(),
  profile: assemblyProfileSchema,
  /** Optional explicit shell realization. Transforms, materials, sockets and joints remain shared. */
  shell: revolvedShellSchema.optional(),
  path: z.array(vector).min(2).max(64),
  bevel: finite.min(0).max(100),
  endBevel: finite.min(0).max(100).optional(),
  /** Local corner erosion on straight rectangular members; zero preserves the original mesh. */
  edgeWear: finite.min(0).max(1).optional(),
  /** Timber origin in material coordinates; independent of geometry, joints and broad colour. */
  grainOffset: vector.optional(),
  position: vector,
  rotation: vector,
  material: id.optional(),
  collision: z.boolean().optional(),
  repeat: z.object({ count: z.number().int().min(1).max(128), offset: vector }),
  sockets: z
    .array(
      z.object({
        id,
        position: vector,
        rotation: vector.optional(),
        anchor: z
          .object({
            point: z.union([z.literal("start"), z.literal("end"), z.number().int().min(0).max(63)]),
            profileOffset: z.tuple([finite.min(-1).max(1), finite.min(-1).max(1)]),
          })
          .optional(),
      }),
    )
    .max(32),
  mate: z
    .object({ part: id, socket: id, ownSocket: id, module: z.number().int().min(0).max(127).optional() })
    .optional(),
  joint: joint.optional(),
  wear: z.object({ amount: finite.min(0).max(1), scale: finite.positive().max(100), seed: z.number().int() }),
});
export const assemblySchema = z
  .object({
    parts: z.array(assemblyPartSchema).min(1).max(128),
    grid: finite.positive().max(100),
    clearances: z.array(z.object({ id, name: z.string().min(1).max(120), min: vector, max: vector })).max(32),
  })
  .superRefine((assembly, context) => {
    const ids = new Set<string>();
    let instances = 0,
      triangles = 0;
    assembly.parts.forEach((part, i) => {
      const issue = (message: string) => context.addIssue({ code: "custom", path: ["parts", i], message });
      if (ids.has(part.id)) issue(`Duplicate part ${part.id}`);
      ids.add(part.id);
      instances += part.repeat.count;
      const sides =
        part.profile.kind === "circle"
          ? part.profile.segments
          : (part.profile.kind === "rectangle" ? 4 : part.profile.points.length) * (part.bevel > 0 ? 2 : 1);
      const wearRings =
        part.edgeWear && part.bevel > 0 && part.profile.kind === "rectangle" && part.path.length === 2
          ? Math.min(64, Math.max(2, Math.ceil(Math.hypot(...sub(part.path[1], part.path[0])) / 0.08) + 1))
          : part.path.length;
      triangles += part.shell
        ? part.shell.profile.length * part.shell.segments * 2 * part.repeat.count
        : sides * (wearRings + (part.endBevel ? 2 : 0)) * 2 * part.repeat.count;
      if (part.shell && (part.bevel || part.endBevel || part.edgeWear))
        issue("Shell edges are defined by the meridian; sweep bevel and edge wear do not apply");
      if (part.edgeWear && (part.bevel <= 0 || part.profile.kind !== "rectangle" || part.path.length !== 2))
        issue("Edge wear requires a straight rectangular member with a positive bevel");
      if (new Set(part.sockets.map((s) => s.id)).size !== part.sockets.length)
        issue("Socket IDs must be unique within a part");
      if (
        part.path.some(
          (_p, n) =>
            n > 0 &&
            n + 1 < part.path.length &&
            Math.hypot(...sub(part.path[n + 1], part.path[n - 1])) < 1e-6,
        )
      )
        issue("Sweep path cannot reverse directly onto itself");
      if (part.path.some((p, n) => n > 0 && Math.hypot(...sub(p, part.path[n - 1])) < 1e-6))
        issue("Sweep path contains a zero-length segment");
      if (part.profile.kind === "polygon") {
        const points = part.profile.points;
        let sign = 0;
        for (let n = 0; n < points.length; n++) {
          const a = points[n],
            b = points[(n + 1) % points.length],
            c = points[(n + 2) % points.length];
          const turn = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
          if (Math.abs(turn) < 1e-8 || (sign && sign * turn < 0)) {
            issue("Sweep polygons must be strictly convex and consistently wound");
            break;
          }
          sign = turn;
          if (
            points.some((p) => ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])) * sign < -1e-8)
          ) {
            issue("Sweep polygons cannot self-intersect");
            break;
          }
        }
      }
    });
    if (triangles > ASSEMBLY_MAX_TRIANGLES)
      context.addIssue({
        code: "custom",
        path: ["parts"],
        message: "Assembly exceeds 80,000 triangle budget; reduce repeats or sweep subdivisions",
      });
    if (instances > 1024)
      context.addIssue({
        code: "custom",
        path: ["parts"],
        message: "Assembly exceeds 1024 realized modules",
      });
    const byId = new Map(assembly.parts.map((part) => [part.id, part]));
    const visiting = new Set<string>(),
      complete = new Set<string>();
    const visit = (part: (typeof assembly.parts)[number]) => {
      if (complete.has(part.id)) return;
      const index = assembly.parts.indexOf(part);
      const issue = (message: string) =>
        context.addIssue({ code: "custom", path: ["parts", index], message });
      if (visiting.has(part.id)) {
        issue("Parent and socket mate dependencies must be acyclic");
        return;
      }
      visiting.add(part.id);
      if (part.parent && part.mate)
        issue("A fixed socket mate replaces the parent transform; select one attachment strategy");
      if (part.joint && part.mate)
        issue("A fixed socket mate cannot also own a joint; articulate a child part");
      for (const socket of part.sockets)
        if (typeof socket.anchor?.point === "number" && socket.anchor.point >= part.path.length)
          issue(`Socket ${socket.id} references a missing path point`);
      const dependency = part.mate?.part ?? part.parent;
      if (dependency) {
        const target = byId.get(dependency);
        if (!target) issue(`Missing attachment part ${dependency}`);
        else {
          if (part.parent && target.repeat.count > 1)
            issue("A repeated module cannot be a parent; use a socket mate with a module index");
          if (part.mate) {
            if (!part.sockets.some((socket) => socket.id === part.mate?.ownSocket))
              issue("Mate references a missing own socket");
            if (!target.sockets.some((socket) => socket.id === part.mate?.socket))
              issue("Mate references a missing target socket");
            if ((part.mate.module ?? 0) >= target.repeat.count)
              issue("Mate references a missing repeated module");
          }
          visit(target);
        }
      }
      visiting.delete(part.id);
      complete.add(part.id);
    };
    for (const part of assembly.parts) visit(part);
    // Mechanical transmission is independent of the transform hierarchy: two
    // sibling leaves or a rack and pinion can share motion without sharing space.
    const resolving = new Set<string>(),
      resolved = new Set<string>();
    const resolveJoint = (part: (typeof assembly.parts)[number]) => {
      if (resolved.has(part.id)) return;
      const issue = (message: string) =>
        context.addIssue({
          code: "custom",
          path: ["parts", assembly.parts.indexOf(part), "joint"],
          message,
        });
      if (resolving.has(part.id)) {
        issue("Linked joint dependencies must be acyclic");
        return;
      }
      resolving.add(part.id);
      if (part.joint?.link) {
        if (part.joint.drive) issue("A linked joint follows its source and cannot own an independent drive");
        const target = byId.get(part.joint.link.part);
        if (!target?.joint) issue(`Linked joint references a missing joint ${part.joint.link.part}`);
        else resolveJoint(target);
      }
      resolving.delete(part.id);
      resolved.add(part.id);
    };
    for (const part of assembly.parts) resolveJoint(part);
    if (new Set(assembly.clearances.map((c) => c.id)).size !== assembly.clearances.length)
      context.addIssue({ code: "custom", path: ["clearances"], message: "Clearance IDs must be unique" });
    assembly.clearances.forEach((c, i) => {
      if (c.min.some((v, axis) => v >= c.max[axis]))
        context.addIssue({
          code: "custom",
          path: ["clearances", i],
          message: "Clearance must have positive extent",
        });
    });
  });
export type AssemblyDefinition = z.infer<typeof assemblySchema>;
export type AssemblyPart = z.infer<typeof assemblyPartSchema>;
export type AssemblyProfile = z.infer<typeof assemblyProfileSchema>;

/** Joint coordinates are radians for hinges and metres for sliders. */
export function evaluateAssemblyJoint(part: AssemblyPart, value = part.joint?.value ?? 0): number {
  if (!Number.isFinite(value)) throw new Error("Joint value must be finite");
  return part.joint ? Math.max(part.joint.minimum, Math.min(part.joint.maximum, value)) : 0;
}
function linkedJointValue(
  assembly: AssemblyDefinition,
  part: AssemblyPart,
  values: Readonly<Record<string, number>>,
  visiting = new Set<string>(),
): number {
  if (!part.joint) return 0;
  if (Object.hasOwn(values, part.id)) return evaluateAssemblyJoint(part, values[part.id]);
  const link = part.joint.link;
  if (!link) return part.joint.value;
  if (visiting.has(part.id)) throw new Error("Linked joint dependencies must be acyclic");
  visiting.add(part.id);
  const target = assembly.parts.find((candidate) => candidate.id === link.part);
  if (!target?.joint) throw new Error(`Missing linked joint ${link.part}`);
  return evaluateAssemblyJoint(
    part,
    linkedJointValue(assembly, target, values, visiting) * link.ratio + link.offset,
  );
}
export function rotateAssemblyAxis(p: Vec3, axis: Vec3, angle: number): Vec3 {
  const n = normalize(axis),
    c = Math.cos(angle),
    s = Math.sin(angle);
  return add(add(scale(p, c), scale(cross(n, p), s)), scale(n, dot(n, p) * (1 - c)));
}
export function assemblyJointValues(
  assembly: AssemblyDefinition,
  time: number,
  commands: Readonly<Record<string, number>> = {},
): Record<string, number> {
  if (!Number.isFinite(time)) throw new Error("Assembly time must be finite");
  const values: Record<string, number> = Object.create(null);
  for (const part of assembly.parts)
    if (part.joint) {
      if (part.joint.link) continue;
      const drive = part.joint.drive;
      const phase = drive ? (((time / drive.period + drive.phase) % 1) + 1) % 1 : 0;
      const driven = drive
        ? part.joint.minimum +
          ((part.joint.maximum - part.joint.minimum) * (1 - Math.cos(phase * Math.PI * 2))) / 2
        : part.joint.value;
      values[part.id] = evaluateAssemblyJoint(
        part,
        Object.hasOwn(commands, part.id) ? commands[part.id] : driven,
      );
    }
  for (const part of assembly.parts)
    if (part.joint && Object.hasOwn(commands, part.id))
      values[part.id] = evaluateAssemblyJoint(part, commands[part.id]);
  for (const part of assembly.parts)
    if (part.joint?.link) values[part.id] = linkedJointValue(assembly, part, values);
  return values;
}
export function rotateAssemblyPoint(point: Vec3, angles: Vec3, inverse = false): Vec3 {
  let result = point;
  for (const axis of inverse ? [2, 1, 0] : [0, 1, 2]) {
    const direction: Vec3 = [0, 0, 0];
    direction[axis] = 1;
    result = rotateAssemblyAxis(result, direction, angles[axis] * (inverse ? -1 : 1));
  }
  return result;
}
export type AssemblySocket = AssemblyPart["sockets"][number];
export type AssemblyFrame = { origin: Vec3; axes: [Vec3, Vec3, Vec3] };
/** Section frames are shared by sweep geometry and dimension-bound socket anchors. */
export function assemblyPathFrames(part: AssemblyPart): AssemblyFrame[] {
  let previousX: Vec3 | undefined;
  return part.path.map((point, i) => {
    const tangent = normalize(
      sub(part.path[Math.min(part.path.length - 1, i + 1)], part.path[Math.max(0, i - 1)]),
    );
    let x = previousX
      ? sub(previousX, scale(tangent, dot(previousX, tangent)))
      : cross(Math.abs(tangent[1]) > 0.95 ? [0, 0, 1] : [0, 1, 0], tangent);
    if (Math.hypot(...x) < 1e-6) x = cross(Math.abs(tangent[1]) > 0.95 ? [0, 0, 1] : [0, 1, 0], tangent);
    x = normalize(x);
    previousX = x;
    return { origin: point, axes: [x, normalize(cross(tangent, x)), tangent] };
  });
}
function framePoint(frame: AssemblyFrame, point: Vec3): Vec3 {
  return add(
    frame.origin,
    add(add(scale(frame.axes[0], point[0]), scale(frame.axes[1], point[1])), scale(frame.axes[2], point[2])),
  );
}
export function assemblySocketFrame(part: AssemblyPart, socket: AssemblySocket): AssemblyFrame {
  let origin = socket.position;
  let axes: AssemblyFrame["axes"] = [
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
  ];
  if (socket.anchor) {
    const point =
      socket.anchor.point === "start"
        ? 0
        : socket.anchor.point === "end"
          ? part.path.length - 1
          : socket.anchor.point;
    const pathFrame = assemblyPathFrames(part)[point];
    if (!pathFrame) throw new Error(`Missing socket anchor point ${point}`);
    const profile = part.profile;
    const width =
      profile.kind === "rectangle"
        ? profile.width / 2
        : profile.kind === "circle"
          ? profile.radius
          : Math.max(...profile.points.map((p) => Math.abs(p[0])));
    const height =
      profile.kind === "rectangle"
        ? profile.height / 2
        : profile.kind === "circle"
          ? profile.radius
          : Math.max(...profile.points.map((p) => Math.abs(p[1])));
    origin = framePoint(
      pathFrame,
      add(socket.position, [
        socket.anchor.profileOffset[0] * width,
        socket.anchor.profileOffset[1] * height,
        0,
      ]),
    );
    axes = pathFrame.axes;
  }
  const rotated = ([0, 1, 2] as const).map((axis) => {
    const basis: Vec3 = [0, 0, 0];
    basis[axis] = 1;
    const local = rotateAssemblyPoint(basis, socket.rotation ?? [0, 0, 0]);
    return add(add(scale(axes[0], local[0]), scale(axes[1], local[1])), scale(axes[2], local[2]));
  }) as AssemblyFrame["axes"];
  return { origin, axes: rotated };
}
/** Transform a part-local point through authored articulation and its parent chain. */
export function assemblyPartPoint(
  assembly: AssemblyDefinition,
  part: AssemblyPart,
  point: Vec3,
  values: Readonly<Record<string, number>> = {},
  repeat = 0,
): Vec3 {
  let result = point;
  if (part.mate) {
    const target = assembly.parts.find((p) => p.id === part.mate?.part),
      own = part.sockets.find((socket) => socket.id === part.mate?.ownSocket),
      socket = target?.sockets.find((socket) => socket.id === part.mate?.socket);
    if (!target || !own || !socket) throw new Error(`Unresolved socket mate on ${part.id}`);
    const movingFrame = assemblySocketFrame(part, own),
      targetFrame = assemblySocketFrame(target, socket);
    const relative = sub(add(point, scale(part.repeat.offset, repeat)), movingFrame.origin);
    const socketPoint = movingFrame.axes.map((axis) => dot(axis, relative)) as Vec3;
    return assemblyPartPoint(
      assembly,
      target,
      framePoint(targetFrame, socketPoint),
      values,
      part.mate.module ?? 0,
    );
  }
  if (part.joint) {
    const value = linkedJointValue(assembly, part, values);
    result =
      part.joint.kind === "slider"
        ? add(result, scale(normalize(part.joint.axis), value))
        : add(part.joint.pivot, rotateAssemblyAxis(sub(result, part.joint.pivot), part.joint.axis, value));
  }
  result = add(
    rotateAssemblyPoint(add(result, scale(part.repeat.offset, repeat)), part.rotation),
    part.position,
  );
  if (part.parent) {
    const parent = assembly.parts.find((p) => p.id === part.parent);
    if (!parent) throw new Error(`Missing assembly parent ${part.parent}`);
    result = assemblyPartPoint(assembly, parent, result, values);
  }
  return result;
}
