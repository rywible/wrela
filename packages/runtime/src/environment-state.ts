import type {
  Cloudscape,
  DayCycle,
  EnvironmentGrade,
  EnvironmentSequence,
  EnvironmentState,
  LightingZone,
  Vec3,
} from "@wrela/model";

const mix = (a: number, b: number, t: number) => a + (b - a) * t;
const mixVector = (a: Vec3, b: Vec3, t: number): Vec3 => a.map((v, i) => mix(v, b[i], t)) as Vec3;
const defaultCloudscape: Cloudscape = { development: 0.45, storminess: 0, highCloudCover: 0.15 };
/** Match identities; appearing/disappearing or changing kinds crossfade their
 * density. At most four authored forms per key become eight runtime envelopes. */
function blendCloudFormations(a: Cloudscape, b: Cloudscape, t: number): Cloudscape["formations"] {
  const left = a.formations ?? [],
    right = b.formations ?? [];
  if (!left.length && !right.length) return undefined;
  const result: NonNullable<Cloudscape["formations"]> = [];
  for (const form of left) {
    const next = right.find((f) => f.id === form.id && f.kind === form.kind);
    if (!next) {
      if (t < 1) result.push({ ...form, density: form.density * (1 - t) });
      continue;
    }
    const yaw = Math.atan2(Math.sin(next.yaw - form.yaw), Math.cos(next.yaw - form.yaw));
    result.push({
      ...form,
      center: [mix(form.center[0], next.center[0], t), mix(form.center[1], next.center[1], t)],
      base: mix(form.base, next.base, t),
      size: mixVector(form.size, next.size, t),
      yaw: form.yaw + yaw * t,
      density: mix(form.density, next.density, t),
      erosion: mix(form.erosion, next.erosion, t),
      maturity: mix(form.maturity ?? 0.45, next.maturity ?? 0.45, t),
      shear: mix(form.shear ?? 0, next.shear ?? 0, t),
      // A seed change changes topology, so treat it as an explicit cut at the key.
      seed: t < 1 ? form.seed : next.seed,
    });
  }
  for (const form of right) {
    if (t > 0 && !left.some((f) => f.id === form.id && f.kind === form.kind))
      result.push({ ...form, density: form.density * t });
  }
  return result;
}
function blendCloudscape(
  a: Cloudscape | undefined,
  b: Cloudscape | undefined,
  t: number,
): Cloudscape | undefined {
  if (!a && !b) return undefined;
  const left = a ?? defaultCloudscape;
  const right = b ?? defaultCloudscape;
  const frontA = left.front ?? (right.front ? { ...right.front, strength: 0 } : undefined);
  const frontB = right.front ?? (left.front ? { ...left.front, strength: 0 } : undefined);
  const direction =
    frontA && frontB
      ? [mix(frontA.direction[0], frontB.direction[0], t), mix(frontA.direction[1], frontB.direction[1], t)]
      : undefined;
  const directionLength = direction ? Math.hypot(...direction) : 0;
  return {
    development: mix(left.development, right.development, t),
    storminess: mix(left.storminess, right.storminess, t),
    highCloudCover: mix(left.highCloudCover, right.highCloudCover, t),
    highCloudHeight: mix(left.highCloudHeight ?? 9800, right.highCloudHeight ?? 9800, t),
    midCloudCover: mix(left.midCloudCover ?? 0, right.midCloudCover ?? 0, t),
    midCloudHeight: mix(left.midCloudHeight ?? 6800, right.midCloudHeight ?? 6800, t),
    midCloudYaw:
      (left.midCloudYaw ?? -0.35) +
      Math.atan2(
        Math.sin((right.midCloudYaw ?? -0.35) - (left.midCloudYaw ?? -0.35)),
        Math.cos((right.midCloudYaw ?? -0.35) - (left.midCloudYaw ?? -0.35)),
      ) *
        t,
    highCloudYaw:
      (left.highCloudYaw ?? 0.4324078) +
      Math.atan2(
        Math.sin((right.highCloudYaw ?? 0.4324078) - (left.highCloudYaw ?? 0.4324078)),
        Math.cos((right.highCloudYaw ?? 0.4324078) - (left.highCloudYaw ?? 0.4324078)),
      ) *
        t,
    background: mix(left.background ?? 1, right.background ?? 1, t),
    formations: blendCloudFormations(left, right, t),
    front:
      frontA && frontB
        ? {
            origin: [mix(frontA.origin[0], frontB.origin[0], t), mix(frontA.origin[1], frontB.origin[1], t)],
            direction:
              direction && directionLength > 0.0001
                ? [direction[0] / directionLength, direction[1] / directionLength]
                : frontA.direction,
            width: mix(frontA.width, frontB.width, t),
            strength: mix(frontA.strength, frontB.strength, t),
          }
        : undefined,
  };
}
/** Local solar time: noon, sunset, and night remain continuous across loops. */
export function sampleDayCycle(
  cycle: DayCycle,
  seconds: number,
): { elevation: number; azimuth: number; hour: number } {
  if (!Number.isFinite(seconds)) throw new Error("Environment time must be finite");
  const hour = (((cycle.startHour + (seconds / cycle.duration) * 24) % 24) + 24) % 24;
  const angle = ((hour - 12) * Math.PI) / 12;
  const latitude = (cycle.latitude * Math.PI) / 180;
  const declination = (cycle.declination * Math.PI) / 180;
  const vertical =
    Math.sin(latitude) * Math.sin(declination) + Math.cos(latitude) * Math.cos(declination) * Math.cos(angle);
  const east = Math.cos(declination) * Math.sin(angle);
  const north =
    Math.cos(latitude) * Math.sin(declination) - Math.sin(latitude) * Math.cos(declination) * Math.cos(angle);
  const c = Math.cos(cycle.northOffset),
    s = Math.sin(cycle.northOffset);
  return {
    elevation: Math.asin(Math.max(-1, Math.min(1, vertical))),
    azimuth: Math.atan2(east * c + north * s, north * c - east * s),
    hour,
  };
}
export function blendEnvironmentGrade(
  a: EnvironmentGrade,
  b: EnvironmentGrade,
  weight: number,
): EnvironmentGrade {
  return {
    exposureCompensation: mix(a.exposureCompensation, b.exposureCompensation, weight),
    tint: mixVector(a.tint, b.tint, weight),
  };
}
/** Stateless sampling makes playback, seeking, capture, and simulation agree. */
export function sampleEnvironmentSequence(sequence: EnvironmentSequence, seconds: number): EnvironmentState {
  if (!Number.isFinite(seconds)) throw new Error("Environment time must be finite");
  const time = sequence.loop
    ? ((seconds % sequence.duration) + sequence.duration) % sequence.duration
    : Math.max(0, Math.min(sequence.duration, seconds));
  const keys = sequence.keyframes;
  let left = keys[0],
    right = keys[keys.length - 1];
  let span: number, offset: number;
  if (time < left.time || time >= right.time) {
    if (!sequence.loop) return structuredClone(time < left.time ? left.state : right.state);
    const first = left;
    left = right;
    right = first;
    span = sequence.duration - left.time + right.time;
    offset = time >= left.time ? time - left.time : sequence.duration - left.time + time;
  } else {
    for (let i = 1; i < keys.length; i++) {
      if (time < keys[i].time) {
        left = keys[i - 1];
        right = keys[i];
        break;
      }
    }
    span = right.time - left.time;
    offset = time - left.time;
  }
  let t = span > 0 ? Math.max(0, Math.min(1, offset / span)) : 0;
  if (sequence.interpolation === "smooth") t = t * t * (3 - 2 * t);
  const a = left.state,
    b = right.state;
  const azimuthDelta = Math.atan2(
    Math.sin(b.sunAzimuth - a.sunAzimuth),
    Math.cos(b.sunAzimuth - a.sunAzimuth),
  );
  return {
    sunElevation: mix(a.sunElevation, b.sunElevation, t),
    sunAzimuth: a.sunAzimuth + azimuthDelta * t,
    turbidity: mix(a.turbidity, b.turbidity, t),
    fogDensity: mix(a.fogDensity, b.fogDensity, t),
    cloudCover: mix(a.cloudCover ?? 0, b.cloudCover ?? 0, t),
    cloudscape: blendCloudscape(a.cloudscape, b.cloudscape, t),
    wind: mixVector(a.wind, b.wind, t),
    ambient: mix(a.ambient, b.ambient, t),
    sunIntensity: mix(a.sunIntensity, b.sunIntensity, t),
    wetness: mix(a.wetness, b.wetness, t),
    grade: blendEnvironmentGrade(a.grade, b.grade, t),
  };
}
/** A zone describes the viewed environment; it is not a substitute for local lights. */
export function lightingZoneWeight(zone: LightingZone, position: Vec3): number {
  const offset = position.map((v, i) => v - zone.center[i]) as Vec3;
  let inward: number;
  const box = zone.box;
  if (box) {
    const c = Math.cos(box.yaw),
      s = Math.sin(box.yaw);
    const local: Vec3 = [c * offset[0] - s * offset[2], offset[1], s * offset[0] + c * offset[2]];
    inward = Math.min(...local.map((v, i) => box.halfExtents[i] - Math.abs(v)));
  } else inward = zone.radius - Math.hypot(...offset);
  if (inward <= 0) return 0;
  if (zone.blendDistance === 0) return 1;
  const t = Math.min(1, inward / zone.blendDistance);
  return t * t * (3 - 2 * t);
}
export function applyLightingZones(
  state: EnvironmentState,
  zones: readonly LightingZone[],
  position: Vec3,
): EnvironmentState {
  const result = structuredClone(state);
  for (const zone of [...zones].sort((a, b) => a.priority - b.priority || a.id.localeCompare(b.id))) {
    const weight = lightingZoneWeight(zone, position);
    result.ambient = mix(result.ambient, zone.ambient, weight);
    result.sunIntensity = mix(result.sunIntensity, zone.sunIntensity, weight);
    result.fogDensity = Math.min(0.1, result.fogDensity * mix(1, zone.fogMultiplier, weight));
    result.grade = blendEnvironmentGrade(result.grade, zone.grade, weight);
  }
  return result;
}
