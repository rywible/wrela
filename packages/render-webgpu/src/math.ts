import type { Camera, Vec3 } from "@wrela/model";
import { cross, dot, normalize, sub } from "@wrela/model";

export function multiply(a: Float32Array, b: Float32Array): Float32Array {
  const out = new Float32Array(16);
  for (let c = 0; c < 4; c++)
    for (let r = 0; r < 4; r++) for (let k = 0; k < 4; k++) out[c * 4 + r] += a[k * 4 + r] * b[c * 4 + k];
  return out;
}
export function lookAt(eye: Vec3, target: Vec3): Float32Array {
  const z = normalize(sub(eye, target));
  const x = normalize(cross(Math.abs(z[1]) > 0.999 ? [0, 0, 1] : [0, 1, 0], z));
  const y = cross(z, x);
  return new Float32Array([
    x[0],
    y[0],
    z[0],
    0,
    x[1],
    y[1],
    z[1],
    0,
    x[2],
    y[2],
    z[2],
    0,
    -dot(x, eye),
    -dot(y, eye),
    -dot(z, eye),
    1,
  ]);
}
/** WebGPU right-handed perspective, depth range [0,1]. */
export function perspective(fov: number, aspect: number, near = 0.05, far = 2500): Float32Array {
  const f = 1 / Math.tan((fov * Math.PI) / 360);
  return new Float32Array([
    f / aspect,
    0,
    0,
    0,
    0,
    f,
    0,
    0,
    0,
    0,
    far / (near - far),
    -1,
    0,
    0,
    (near * far) / (near - far),
    0,
  ]);
}
export function orthographic(radius: number, near: number, far: number): Float32Array {
  return new Float32Array([
    1 / radius,
    0,
    0,
    0,
    0,
    1 / radius,
    0,
    0,
    0,
    0,
    1 / (near - far),
    0,
    0,
    0,
    near / (near - far),
    1,
  ]);
}
export function cameraBasis(camera: Camera): { forward: Vec3; right: Vec3; up: Vec3 } {
  const forward = normalize(sub(camera.target, camera.position));
  const right = normalize(cross(forward, Math.abs(forward[1]) > 0.999 ? [0, 0, 1] : [0, 1, 0]));
  return { forward, right, up: cross(right, forward) };
}
export function cameraRay(
  camera: Camera,
  x: number,
  y: number,
  aspect: number,
): { origin: Vec3; direction: Vec3 } {
  const { forward, right, up } = cameraBasis(camera);
  const t = Math.tan((camera.fov * Math.PI) / 360);
  return {
    origin: [...camera.position],
    direction: normalize(forward.map((v, i) => v + right[i] * x * aspect * t + up[i] * y * t) as Vec3),
  };
}
