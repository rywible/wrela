import { type Camera, cross, dot, normalize, sub, type Vec3 } from "@wrela/model";

export type Overlay = "rig" | "colliders";
export type OverlaySegment = { a: Vec3; b: Vec3; kind: "rig" | "collider" };
export function projectSegment(segment: OverlaySegment, camera: Camera, width: number, height: number) {
  const forward = normalize(sub(camera.target, camera.position));
  const right = normalize(cross(forward, Math.abs(forward[1]) > 0.999 ? [0, 0, 1] : [0, 1, 0]));
  const up = cross(right, forward);
  const transform = (point: Vec3): Vec3 => {
    const delta = sub(point, camera.position);
    return [dot(delta, right), dot(delta, up), dot(delta, forward)];
  };
  let a = transform(segment.a),
    b = transform(segment.b);
  const near = 0.05;
  if (a[2] < near && b[2] < near) return null;
  if (a[2] < near || b[2] < near) {
    const fraction = (near - a[2]) / (b[2] - a[2]);
    const intersection = a.map((v, i) => v + (b[i] - v) * fraction) as Vec3;
    if (a[2] < near) a = intersection;
    else b = intersection;
  }
  const scale = height / (2 * Math.tan((camera.fov * Math.PI) / 360));
  const pixel = (point: Vec3) =>
    [width / 2 + (point[0] * scale) / point[2], height / 2 - (point[1] * scale) / point[2]] as const;
  return [pixel(a), pixel(b)] as const;
}
export function drawOverlays(
  context: CanvasRenderingContext2D,
  segments: OverlaySegment[],
  camera: Camera,
  overlays: readonly Overlay[],
) {
  context.save();
  context.lineWidth = Math.max(1.5, context.canvas.width / 900);
  context.lineCap = "round";
  for (const segment of segments) {
    if (!overlays.includes(segment.kind === "rig" ? "rig" : "colliders")) continue;
    const line = projectSegment(segment, camera, context.canvas.width, context.canvas.height);
    if (!line) continue;
    context.strokeStyle = segment.kind === "rig" ? "#ffd078" : "#64edcf";
    context.beginPath();
    context.moveTo(...line[0]);
    context.lineTo(...line[1]);
    context.stroke();
  }
  context.restore();
}
export async function overlayCapture(
  blob: Blob,
  segments: OverlaySegment[],
  camera: Camera,
  overlays: readonly Overlay[],
) {
  const bitmap = await createImageBitmap(blob);
  const canvas = document.createElement("canvas");
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  const context = canvas.getContext("2d");
  if (!context) {
    bitmap.close();
    throw Error("Diagnostic overlay canvas is unavailable");
  }
  context.drawImage(bitmap, 0, 0);
  bitmap.close();
  drawOverlays(context, segments, camera, overlays);
  return new Promise<Blob>((resolve, reject) =>
    canvas.toBlob(
      (image) => (image ? resolve(image) : reject(Error("Diagnostic capture encoding failed"))),
      "image/png",
    ),
  );
}
