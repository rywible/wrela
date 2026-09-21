/** In an active polynomial smooth-min branch, two quadratic primitive fields
 * compile to a quartic zero equation. Linear/quadratic metric gauges increase
 * the degree to six/eight. Roots must still pass |a-b| <= k and region checks. */
export function evaluatePolynomial(p: number[], t: number): number {
  let value = 0;
  for (let i = p.length - 1; i >= 0; i--) value = value * t + p[i];
  return value;
}
export function blendPolynomial(a: number[], b: number[], k: number): number[] {
  if (!(k > 0)) throw new RangeError("A smooth blend needs positive width");
  const n = Math.max(a.length, b.length),
    difference = Array.from({ length: n }, (_, i) => (a[i] ?? 0) - (b[i] ?? 0));
  const p = Array<number>(2 * n - 1).fill(0);
  for (let i = 0; i < n; i++) {
    p[i] -= 2 * k * ((a[i] ?? 0) + (b[i] ?? 0));
    for (let j = 0; j < n; j++) p[i + j] += difference[i] * difference[j];
  }
  p[0] += k * k;
  return p;
}
const choose = (n: number, k: number) => {
  let value = 1;
  for (let i = 1; i <= k; i++) value = (value * (n - i + 1)) / i;
  return value;
};
export function bernsteinCoefficients(p: number[], lo: number, hi: number): number[] {
  const n = p.length - 1;
  const power = p.map((_, j) =>
    p.reduce((sum, v, i) => (i < j ? sum : sum + v * choose(i, j) * lo ** (i - j) * (hi - lo) ** j), 0),
  );
  return p.map((_, k) =>
    power.reduce((sum, v, j) => (j > k ? sum : sum + (v * choose(k, j)) / choose(n, j)), 0),
  );
}
/** Bernstein convex hull subdivision returns candidate root intervals, including
 * tangencies. This exploratory f64 implementation uses padding, not a formal
 * floating-point proof. It reports exhaustion instead of silently missing roots. */
export function isolatePolynomial(p: number[], lo: number, hi: number, tolerance = 1e-7) {
  if (!(hi > lo) || !(tolerance > 0)) throw new RangeError("Invalid root isolation interval");
  const initial = bernsteinCoefficients(p, lo, hi);
  if (initial.every((v) => v === 0)) return { intervals: [[lo, hi]], visited: 1 };
  const padding = 1e-13 * Math.max(...initial.map(Math.abs), 1e-10);
  const stack = [{ b: initial, lo, hi }],
    intervals: number[][] = [];
  let visited = 0;
  while (stack.length) {
    const cell = stack.pop();
    if (!cell) break;
    if (++visited > 100000) throw new Error("Polynomial isolation budget exhausted");
    if (Math.min(...cell.b) > padding || Math.max(...cell.b) < -padding) continue;
    if (cell.hi - cell.lo <= tolerance) {
      const previous = intervals.at(-1);
      if (previous && cell.lo <= previous[1] + tolerance) previous[1] = cell.hi;
      else intervals.push([cell.lo, cell.hi]);
      continue;
    }
    const temp = [...cell.b],
      left = [temp[0]],
      right = [temp.at(-1) ?? 0];
    for (let level = temp.length - 1; level > 0; level--) {
      for (let i = 0; i < level; i++) temp[i] = (temp[i] + temp[i + 1]) / 2;
      left.push(temp[0]);
      right.unshift(temp[level - 1]);
    }
    const mid = (cell.lo + cell.hi) / 2;
    stack.push({ b: right, lo: mid, hi: cell.hi }, { b: left, lo: cell.lo, hi: mid });
  }
  return { intervals, visited };
}
