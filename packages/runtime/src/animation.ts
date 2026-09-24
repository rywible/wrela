import type { Joint, Motion, Quat, Vec3 } from "@wrela/model";

export type JointPose = { translation: Vec3; rotation: Quat };
export type Pose = Map<string, JointPose>;
export const quatIdentity = (): Quat => [0, 0, 0, 1];
export function quatMultiply(a: Quat, b: Quat): Quat {
  return [
    a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
    a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
    a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
    a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
  ];
}
export function quatFromEuler(v: Vec3): Quat {
  const [x, y, z] = v.map((a) => a / 2),
    sx = Math.sin(x),
    cx = Math.cos(x),
    sy = Math.sin(y),
    cy = Math.cos(y),
    sz = Math.sin(z),
    cz = Math.cos(z);
  return [
    sx * cy * cz + cx * sy * sz,
    cx * sy * cz - sx * cy * sz,
    cx * cy * sz + sx * sy * cz,
    cx * cy * cz - sx * sy * sz,
  ];
}
export function quatSlerp(a: Quat, b: Quat, alpha: number): Quat {
  let cosine = a.reduce((sum, v, i) => sum + v * b[i], 0);
  const target = b.map((v) => (cosine < 0 ? -v : v)) as Quat;
  cosine = Math.abs(cosine);
  if (cosine > 0.9995) {
    const q = a.map((v, i) => v + (target[i] - v) * alpha);
    const n = Math.hypot(...q);
    return q.map((v) => v / n) as Quat;
  }
  const angle = Math.acos(Math.min(1, cosine)),
    sin = Math.sin(angle);
  return a.map(
    (v, i) => (v * Math.sin((1 - alpha) * angle) + target[i] * Math.sin(alpha * angle)) / sin,
  ) as Quat;
}
export function rotateVector(q: Quat, v: Vec3): Vec3 {
  // Expanded q * (v,0) * conjugate(q), preserving non-unit quaternion
  // behavior without allocating two intermediate quaternions per vector.
  const [x, y, z, w] = q,
    [vx, vy, vz] = v;
  const tx = 2 * (y * vz - z * vy),
    ty = 2 * (z * vx - x * vz),
    tz = 2 * (x * vy - y * vx);
  const norm = x * x + y * y + z * z + w * w;
  return [
    norm * vx + w * tx + y * tz - z * ty,
    norm * vy + w * ty + z * tx - x * tz,
    norm * vz + w * tz + x * ty - y * tx,
  ];
}
const restPose = (): JointPose => ({ translation: [0, 0, 0], rotation: quatIdentity() });
export type CompiledMotionTracks = ReadonlyMap<string, readonly Motion["keys"][number][]>;
const compiledTracks = new WeakMap<Motion, CompiledMotionTracks>();
/** Clips are immutable compiled artifacts. Index and sort tracks once, not per joint/frame. */
export function compileMotionTracks(motion: Motion): CompiledMotionTracks {
  const cached = compiledTracks.get(motion);
  if (cached) return cached;
  const tracks = new Map<string, Motion["keys"]>();
  for (const key of motion.keys) {
    const track = tracks.get(key.joint) ?? [];
    track.push(key);
    tracks.set(key.joint, track);
  }
  for (const track of tracks.values()) track.sort((a, b) => a.time - b.time);
  compiledTracks.set(motion, tracks);
  return tracks;
}
export function sampleMotion(
  joints: Joint[],
  motion: Motion | undefined,
  time: number,
  accumulateRoot = false,
): Pose {
  const pose: Pose = new Map(joints.map((j) => [j.id, restPose()]));
  if (!motion) return pose;
  const t = motion.loop
    ? ((time % motion.duration) + motion.duration) % motion.duration
    : Math.max(0, Math.min(time, motion.duration));
  for (const joint of joints) {
    const keys = compileMotionTracks(motion).get(joint.id) ?? [];
    if (!keys.length) continue;
    let a = keys[0],
      b = keys[keys.length - 1],
      ta = a.time,
      tb = b.time;
    const next = keys.findIndex((k) => k.time >= t);
    if (next > 0) {
      a = keys[next - 1];
      b = keys[next];
      ta = a.time;
      tb = b.time;
    } else if (next === 0) {
      if (motion.loop && keys.length > 1 && t < keys[0].time) {
        a = keys[keys.length - 1];
        b = keys[0];
        ta = a.time - motion.duration;
        tb = b.time;
      } else {
        a = b = keys[0];
        ta = tb = a.time;
      }
    } else if (motion.loop && keys.length > 1) {
      a = keys[keys.length - 1];
      b = keys[0];
      ta = a.time;
      tb = b.time + motion.duration;
    } else {
      a = b = keys[keys.length - 1];
      ta = tb = a.time;
    }
    const alpha = tb === ta ? 0 : Math.max(0, Math.min(1, (t - ta) / (tb - ta)));
    const clampRotation = (rotation: Vec3): Vec3 =>
      rotation.map((v) => Math.max(joint.minimum, Math.min(joint.maximum, v))) as Vec3;
    pose.set(joint.id, {
      translation: a.translation.map(
        (v, i) =>
          v +
          (b.translation[i] - v) * alpha +
          (accumulateRoot && !joint.parent && motion.loop
            ? Math.floor(time / motion.duration) *
              (keys[keys.length - 1].translation[i] - keys[0].translation[i])
            : 0),
      ) as Vec3,
      rotation: quatSlerp(
        quatFromEuler(clampRotation(a.rotation)),
        quatFromEuler(clampRotation(b.rotation)),
        alpha,
      ),
    });
  }
  return pose;
}
export function blendPoses(a: Pose, b: Pose, weight: number): Pose {
  const result: Pose = new Map();
  const t = Math.max(0, Math.min(1, weight));
  for (const id of new Set([...a.keys(), ...b.keys()])) {
    const pa = a.get(id) ?? restPose(),
      pb = b.get(id) ?? restPose();
    result.set(id, {
      rotation: quatSlerp(pa.rotation, pb.rotation, t),
      translation: pa.translation.map((v, i) => v + (pb.translation[i] - v) * t) as Vec3,
    });
  }
  return result;
}
export function poseMatrices(joints: Joint[], pose: Pose): Float32Array {
  const output = new Float32Array(joints.length * 16);
  const jointMap = new Map(joints.map((j) => [j.id, j]));
  const global = new Map<string, { rotation: Quat; position: Vec3; restRotation: Quat }>();
  const visiting = new Set<string>();
  const solve = (joint: Joint): { rotation: Quat; position: Vec3; restRotation: Quat } => {
    const ready = global.get(joint.id);
    if (ready) return ready;
    if (visiting.has(joint.id)) throw new Error("Cyclic skeleton");
    visiting.add(joint.id);
    const local = pose.get(joint.id) ?? restPose(),
      parentJoint = joint.parent ? jointMap.get(joint.parent) : undefined;
    const parent = parentJoint ? solve(parentJoint) : undefined;
    const restRotation = parent
      ? quatMultiply(parent.restRotation, quatFromEuler(joint.rotation))
      : quatFromEuler(joint.rotation);
    const animatedRotation = quatMultiply(restRotation, local.rotation);
    const parentDeform = parent
      ? quatMultiply(parent.rotation, [
          -parent.restRotation[0],
          -parent.restRotation[1],
          -parent.restRotation[2],
          parent.restRotation[3],
        ])
      : quatIdentity();
    const offset: Vec3 = joint.position.map(
      (v, i) => v - (parentJoint?.position[i] ?? 0) + local.translation[i],
    ) as Vec3;
    const moved = rotateVector(parentDeform, offset);
    const position = moved.map((v, i) => v + (parent?.position[i] ?? 0)) as Vec3;
    const value = { rotation: quatMultiply(parentDeform, animatedRotation), position, restRotation };
    visiting.delete(joint.id);
    global.set(joint.id, value);
    return value;
  };
  joints.forEach((joint, index) => {
    const g = solve(joint),
      q = quatMultiply(g.rotation, [
        -g.restRotation[0],
        -g.restRotation[1],
        -g.restRotation[2],
        g.restRotation[3],
      ]),
      [x, y, z, w] = q;
    const anchor = rotateVector(q, joint.position),
      p = g.position.map((v, i) => v - anchor[i]);
    output.set(
      [
        1 - 2 * (y * y + z * z),
        2 * (x * y + z * w),
        2 * (x * z - y * w),
        0,
        2 * (x * y - z * w),
        1 - 2 * (x * x + z * z),
        2 * (y * z + x * w),
        0,
        2 * (x * z + y * w),
        2 * (y * z - x * w),
        1 - 2 * (x * x + y * y),
        0,
        p[0],
        p[1],
        p[2],
        1,
      ],
      index * 16,
    );
  });
  return output;
}
