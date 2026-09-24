import type { ReviewTriangle } from "./vegetation-review";

/** The small illustrative canvas uses the compiled scalar field too: proxy
 * rectangles must never appear as opaque foliage in the authoring review. */
export function paintVegetationReview(context: CanvasRenderingContext2D, triangles: ReviewTriangle[]) {
  const tiles = new Map<string, HTMLCanvasElement>();
  for (const triangle of triangles) {
    context.save();
    context.beginPath();
    context.moveTo(...triangle.points[0]);
    context.lineTo(...triangle.points[1]);
    context.lineTo(...triangle.points[2]);
    context.closePath();
    const coverage = triangle.coverage,
      uv = triangle.uv;
    if (!coverage || !uv) {
      context.fillStyle = triangle.color;
      context.fill();
      context.restore();
      continue;
    }
    const key = `${coverage.key}/${triangle.color}`;
    let tile = tiles.get(key);
    if (!tile) {
      tile = document.createElement("canvas");
      tile.width = coverage.width;
      tile.height = coverage.height;
      const target = tile.getContext("2d");
      if (!target) throw Error("Could not create vegetation coverage preview");
      target.fillStyle = triangle.color;
      target.fillRect(0, 0, tile.width, tile.height);
      const image = target.getImageData(0, 0, tile.width, tile.height);
      for (let i = 0; i < coverage.levels[0].length; i++) image.data[i * 4 + 3] = coverage.levels[0][i];
      target.putImageData(image, 0, 0);
      tiles.set(key, tile);
    }
    const [a, b, c] = uv.map(([u, v]) => [u * coverage.width, v * coverage.height]);
    const [p, q, r] = triangle.points;
    const ux = b[0] - a[0],
      uy = b[1] - a[1],
      vx = c[0] - a[0],
      vy = c[1] - a[1];
    const determinant = ux * vy - uy * vx;
    if (Math.abs(determinant) > 1e-8) {
      const sx = ((q[0] - p[0]) * vy - (r[0] - p[0]) * uy) / determinant;
      const sy = ((q[1] - p[1]) * vy - (r[1] - p[1]) * uy) / determinant;
      const tx = ((r[0] - p[0]) * ux - (q[0] - p[0]) * vx) / determinant;
      const ty = ((r[1] - p[1]) * ux - (q[1] - p[1]) * vx) / determinant;
      context.clip();
      context.transform(sx, sy, tx, ty, p[0] - sx * a[0] - tx * a[1], p[1] - sy * a[0] - ty * a[1]);
      context.drawImage(tile, 0, 0);
    }
    context.restore();
  }
}
