import { expect, test } from "bun:test";
import {
  bernsteinCoefficients,
  blendPolynomial,
  evaluatePolynomial,
  isolatePolynomial,
} from "./blend-polynomial";
import { smoothMin } from "./local-program";

test("the compiled quartic equals -4k times the field throughout the active blend", () => {
  const a = [0.06, 0.18, 0.3],
    b = [0.1, -0.2, 0.25],
    k = 0.4,
    p = blendPolynomial(a, b, k);
  expect(p.length).toBe(5);
  for (let i = 0; i <= 1000; i++) {
    const t = i / 1000 - 0.5,
      x = evaluatePolynomial(a, t),
      y = evaluatePolynomial(b, t);
    expect(Math.abs(x - y)).toBeLessThan(k);
    expect(Math.abs(evaluatePolynomial(p, t) + 4 * k * smoothMin(x, y, k))).toBeLessThan(1e-14);
  }
  const roots = isolatePolynomial(p, -0.5, 0.5);
  expect(roots.intervals.length).toBeGreaterThan(0);
  for (const [lo, hi] of roots.intervals) {
    const t = (lo + hi) / 2;
    expect(Math.abs(smoothMin(evaluatePolynomial(a, t), evaluatePolynomial(b, t), k))).toBeLessThan(1e-7);
  }
});
test("Bernstein subdivision retains simple and tangent roots and rejects empty intervals", () => {
  for (const p of [
    [-1, 0, 1],
    [1, -2, 1],
    [0.0625, 0, -0.5, 0, 1],
  ]) {
    const { intervals } = isolatePolynomial(p, -2, 2);
    expect(intervals.length).toBeGreaterThan(0);
    for (const [lo, hi] of intervals)
      expect(Math.abs(evaluatePolynomial(p, (lo + hi) / 2))).toBeLessThan(1e-6);
  }
  expect(isolatePolynomial([1, 0, 1], -2, 2).intervals.length).toBe(0);
  const p = [0.1, -0.3, 0.7, 0.2, -0.4],
    b = bernsteinCoefficients(p, -1.2, 0.8);
  for (let i = 0; i <= 1000; i++) {
    const v = evaluatePolynomial(p, -1.2 + i * 0.002);
    expect(v).toBeGreaterThanOrEqual(Math.min(...b) - 1e-12);
    expect(v).toBeLessThanOrEqual(Math.max(...b) + 1e-12);
  }
});
