import { expect, test } from "bun:test";
import { compileFourier, type Expression, wovenExpression } from "./periodic-material";
import { evaluatePhasePolynomial } from "./phase";

test("woven coverage retains the joint average in correlated and oblique footprints", () => {
  const program = compileFourier(wovenExpression, 2);
  expect(program.terms).toHaveLength(4);
  const cases = [
    { origin: [0.5, 1.2], dx: [0, 0], dy: [0, 0], shutter: [0, 0] },
    { origin: [0, 0], dx: [Math.PI * 8, Math.PI * 8], dy: [0, 0], shutter: [0, 0] },
    { origin: [1.7, 0.2], dx: [6, 9], dy: [3, -2], shutter: [0, 0] },
  ];
  for (const f of cases) {
    let sum = 0;
    const count = 192;
    for (let y = 0; y < count; y++)
      for (let x = 0; x < count; x++) {
        const p = f.origin.map(
          (v, i) => v + ((x + 0.5) / count - 0.5) * f.dx[i] + ((y + 0.5) / count - 0.5) * f.dy[i],
        );
        sum += (0.5 + 0.5 * Math.cos(p[0])) * (0.5 + 0.5 * Math.cos(p[1]));
      }
    expect(Math.abs(evaluatePhasePolynomial(program, f).value - sum / count ** 2)).toBeLessThan(2e-5);
  }
  expect(evaluatePhasePolynomial(program, cases[1]).value).toBeCloseTo(0.375, 12);
});
test("symbolic material compiler rejects cyclic, excessive and nonfinite sources", () => {
  const cycle: Expression = {
    kind: "add",
    left: { kind: "constant", value: 1 },
    right: { kind: "constant", value: 2 },
  };
  cycle.left = cycle;
  expect(() => compileFourier(cycle, 2)).toThrow();
  expect(() => compileFourier({ kind: "constant", value: Infinity }, 2)).toThrow();
  let deep: Expression = { kind: "constant", value: 1 };
  for (let i = 0; i < 70; i++) deep = { kind: "add", left: deep, right: { kind: "constant", value: 1 } };
  expect(() => compileFourier(deep, 2)).toThrow();
});
