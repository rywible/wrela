export type ImageDifference = {
  rms: number;
  maximum: number;
  squared: number;
  samples: number;
  halfFormatBound: number;
};
const halfBound = (value: number) => Math.abs(value) * 2 ** -10 + 2 ** -24;
/** Exact binary16-to-binary32 readback; this extra bound concerns comparison before binary16 quantization. */
export function compareFrontierImages(reference: Float32Array, candidate: Float32Array): ImageDifference {
  if (!reference.length || reference.length !== candidate.length || reference.length % 4)
    throw Error("Matched RGBA images required");
  let squared = 0,
    maximum = 0,
    format = 0;
  for (let index = 0; index < reference.length; index++) {
    if (!Number.isFinite(reference[index]) || !Number.isFinite(candidate[index]))
      throw Error("Nonfinite HDR evidence");
    if (index % 4 === 3) continue;
    const error = Math.abs(candidate[index] - reference[index]);
    squared += error * error;
    maximum = Math.max(maximum, error);
    format = Math.max(format, halfBound(reference[index]) + halfBound(candidate[index]));
  }
  const samples = (reference.length / 4) * 3;
  return { rms: Math.sqrt(squared / samples), maximum, squared, samples, halfFormatBound: format };
}
export function compareScreenErrorDelta(
  previousReference: Float32Array,
  previous: Float32Array,
  reference: Float32Array,
  candidate: Float32Array,
): ImageDifference {
  if (previousReference.length !== reference.length || previous.length !== candidate.length)
    throw Error("Temporal image dimensions differ");
  const a = new Float32Array(reference.length),
    b = new Float32Array(candidate.length);
  for (let index = 0; index < a.length; index++) {
    a[index] = reference[index] - previousReference[index];
    b[index] = candidate[index] - previous[index];
  }
  const value = compareFrontierImages(a, b);
  value.halfFormatBound =
    compareFrontierImages(previousReference, previous).halfFormatBound +
    compareFrontierImages(reference, candidate).halfFormatBound;
  return value;
}
export function aggregateImageDifferences(values: ImageDifference[]) {
  if (!values.length) throw Error("Image evidence is empty");
  const samples = values.reduce((sum, value) => sum + value.samples, 0);
  return {
    rms: Math.sqrt(values.reduce((sum, value) => sum + value.squared, 0) / samples),
    maximum: Math.max(...values.map((value) => value.maximum)),
    samples,
    halfFormatBound: Math.max(...values.map((value) => value.halfFormatBound)),
  };
}
function boundary(image: Float32Array, width: number, height: number): [number, number][] {
  if (
    !Number.isSafeInteger(width) ||
    !Number.isSafeInteger(height) ||
    width < 1 ||
    height < 1 ||
    image.length !== width * height * 4
  )
    throw Error("Silhouette dimensions differ");
  for (let index = 0; index < image.length; index += 4) {
    const value = image[index];
    if ((value !== 0 && value !== 1) || image[index + 1] !== value || image[index + 2] !== value)
      throw Error("Native binary silhouette evidence required");
  }
  const points: [number, number][] = [];
  const occupied = (x: number, y: number) =>
    x >= 0 && y >= 0 && x < width && y < height && image[(y * width + x) * 4] > 0.5;
  for (let y = 0; y < height; y++)
    for (let x = 0; x < width; x++)
      if (
        occupied(x, y) &&
        (!occupied(x - 1, y) || !occupied(x + 1, y) || !occupied(x, y - 1) || !occupied(x, y + 1))
      )
        points.push([x, y]);
  return points;
}
/** Exact symmetric Euclidean boundary distance for the finite native-resolution binary masks. */
export function silhouetteDistance(
  reference: Float32Array,
  candidate: Float32Array,
  width: number,
  height: number,
): number | null {
  const a = boundary(reference, width, height),
    b = boundary(candidate, width, height);
  if (!a.length || !b.length) return a.length === b.length ? 0 : null;
  let maximum = 0;
  for (const [left, right] of [
    [a, b],
    [b, a],
  ])
    for (const [x, y] of left) {
      let nearest = Infinity;
      for (const [u, v] of right) nearest = Math.min(nearest, (x - u) ** 2 + (y - v) ** 2);
      maximum = Math.max(maximum, nearest);
    }
  return Math.sqrt(maximum);
}
export function observedDistribution(values: number[]) {
  if (!values.length || values.some((value) => !Number.isFinite(value) || value < 0))
    throw Error("Finite observed timing samples required");
  const sorted = [...values].sort((a, b) => a - b),
    at = (fraction: number) => sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * fraction) - 1)];
  const p95 = at(0.95),
    minimum = sorted[0],
    maximum = sorted[sorted.length - 1];
  return {
    samples: sorted.length,
    p50: at(0.5),
    p95,
    minimum,
    maximum,
    uncertainty: Math.max(p95 - minimum, maximum - p95),
    scope: "Envelope of actual retained samples; not a confidence bound for future runs",
  };
}
