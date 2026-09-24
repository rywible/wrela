import { contentKey } from "@wrela/model";

import type { PhasePolynomial, PhaseTerm } from "./phase";

/** Supported subset: finite sums/products of integer phase cosines. Unsupported
 * nonlinear operations are not approximated and no terms are silently dropped. */
export type Expression =
  | { kind: "constant"; value: number }
  | { kind: "cosine"; mode: number[]; phase: number }
  | { kind: "add" | "multiply"; left: Expression; right: Expression };
type Spectrum = Map<string, PhaseTerm>;
export function compileFourier(expression: Expression, dimensions: number): PhasePolynomial {
  if (!Number.isInteger(dimensions) || dimensions < 1 || dimensions > 8)
    throw Error("Invalid phase dimensions");
  const add = (map: Spectrum, term: PhaseTerm) => {
    const key = term.mode.join(",");
    const old = map.get(key);
    const value = old
      ? { mode: term.mode, real: old.real + term.real, imaginary: old.imaginary + term.imaginary }
      : term;
    if (
      !Number.isFinite(value.real) ||
      !Number.isFinite(value.imaginary) ||
      !value.mode.every(Number.isSafeInteger)
    )
      throw Error("Nonfinite phase expression");
    if (value.real === 0 && value.imaginary === 0) map.delete(key);
    else map.set(key, value);
    if (map.size > 4096) throw Error("Symbolic expansion too large; use the original expression");
  };
  const active = new Set<Expression>();
  let nodes = 0,
    operations = 0;
  const visit = (node: Expression, depth = 0): Spectrum => {
    if (++nodes > 1024 || depth > 64 || active.has(node))
      throw Error("Invalid or excessive phase expression");
    active.add(node);
    const result: Spectrum = new Map();
    if (node.kind === "constant")
      add(result, { mode: Array(dimensions).fill(0), real: node.value, imaginary: 0 });
    else if (node.kind === "cosine") {
      if (
        node.mode.length !== dimensions ||
        !node.mode.every(Number.isSafeInteger) ||
        !Number.isFinite(node.phase)
      )
        throw Error("Invalid phase carrier");
      add(result, {
        mode: node.mode,
        real: 0.5 * Math.cos(node.phase),
        imaginary: 0.5 * Math.sin(node.phase),
      });
      add(result, {
        mode: node.mode.map((v) => -v),
        real: 0.5 * Math.cos(node.phase),
        imaginary: -0.5 * Math.sin(node.phase),
      });
    } else {
      if (node.kind !== "add" && node.kind !== "multiply") throw Error("Unsupported phase expression");
      const left = visit(node.left, depth + 1),
        right = visit(node.right, depth + 1);
      operations += left.size * right.size;
      if (operations > 1_000_000) throw Error("Symbolic expansion too large; use the original expression");
      if (node.kind === "add") for (const term of [...left.values(), ...right.values()]) add(result, term);
      else
        for (const a of left.values())
          for (const b of right.values())
            add(result, {
              mode: a.mode.map((v, i) => v + b.mode[i]),
              real: a.real * b.real - a.imaginary * b.imaginary,
              imaginary: a.real * b.imaginary + a.imaginary * b.real,
            });
    }
    active.delete(node);
    return result;
  };
  const spectrum = visit(expression);
  const constant = spectrum.get(Array(dimensions).fill(0).join(","))?.real ?? 0;
  const terms = [...spectrum.values()].filter((term) => (term.mode.find((k) => k !== 0) ?? 0) > 0);
  return {
    constant,
    terms,
    sourceKey: contentKey(expression),
    parameterKey: `finite-fourier-1-${dimensions}`,
    sourceFit: {
      kind: "unknown",
      reason:
        "Algebraic expansion of a finite expression; no fitted approximation, floating-point error not certified",
    },
  };
}
const constant = (value: number): Expression => ({ kind: "constant", value });
const cosine = (mode: number[]): Expression => ({ kind: "cosine", mode, phase: 0 });
const multiply = (left: Expression, right: Expression): Expression => ({ kind: "multiply", left, right });
const add = (left: Expression, right: Expression): Expression => ({ kind: "add", left, right });
export const correlatedExpression = multiply(
  add(constant(0.6), multiply(constant(0.35), cosine([1, 0]))),
  add(constant(0.5), multiply(constant(0.45), cosine([1, 1]))),
);

/** Woven color coverage. Both carriers are spatial, and their product is integrated
 * jointly. Their sum/difference modes preserve covariance in oblique footprints. */
export const wovenExpression = multiply(
  add(constant(0.5), multiply(constant(0.5), cosine([1, 0]))),
  add(constant(0.5), multiply(constant(0.5), cosine([0, 1]))),
);

export function emitWovenWGSL(): string {
  const program = compileFourier(wovenExpression, 2);
  const number = (value: number) => (Number.isInteger(value) ? `${value}.0` : String(value));
  const terms = program.terms
    .map(({ mode, real, imaginary }) => {
      if (imaginary !== 0) throw Error("Woven recipe unexpectedly contains sine coefficients");
      return `  value+=${number(2 * real)}*wovenMode(vec2f(${mode.map(number).join(",")}),phase,dx,dy);`;
    })
    .join("\n");
  return `// Generated by tools/compiler-frontiers/build-material.ts. Source ${program.sourceKey}.
// Albedo coverage only. Exact for affine phase footprints in real arithmetic.
const WOVEN_MODE:u32=0u;
fn wovenSinc(x:f32)->f32 {if(abs(x)<0.0001){return 1.0-x*x/6.0;}return sin(x)/x;}
fn wovenMode(k:vec2f,phase:vec2f,dx:vec2f,dy:vec2f)->f32 {
 return cos(dot(k,phase))*wovenSinc(dot(k,dx)*0.5)*wovenSinc(dot(k,dy)*0.5);
}
fn wovenCoverage(phase:vec2f,dx:vec2f,dy:vec2f)->f32 {
 if(WOVEN_MODE==1u){return (0.5+0.5*cos(phase.x))*(0.5+0.5*cos(phase.y));}
 if(WOVEN_MODE>=2u){
   var sum=0.0;let count=select(32u,64u,WOVEN_MODE==3u);
   for(var y=0u;y<count;y++){for(var x=0u;x<count;x++){
     let p=phase+((f32(x)+0.5)/f32(count)-0.5)*dx+((f32(y)+0.5)/f32(count)-0.5)*dy;
     sum+=(0.5+0.5*cos(p.x))*(0.5+0.5*cos(p.y));
   }}return sum/f32(count*count);
 }
 var value=${number(program.constant)};
${terms}
 return clamp(value,0.0,1.0);
}
`;
}

/** Specialization for the probe's two phases, with the second spatially constant.
 * Emits the entire product polynomial, rather than multiplying filtered operands. */
export function emitCorrelatedWGSL(program: PhasePolynomial): string {
  const number = (n: number) => (Number.isInteger(n) ? `${n}.0` : String(n));
  return (
    `value=${number(program.constant)}` +
    program.terms
      .map((term) => {
        if (term.mode.length !== 2) throw Error("Probe emitter needs two phases");
        const [k, shift] = term.mode;
        const argument = `(${number(k)}*p.x+${number(shift)}*p.y)`;
        const oscillation = `(${number(2 * term.real)}*cos${argument}${term.imaginary ? `-${number(2 * term.imaginary)}*sin${argument}` : ""})`;
        const footprint = k
          ? `*sinc(${number(k * 0.5)}*p.z)*sinc(${number(k * 0.5)}*p.w)*sinc(${number(k * 0.5)}*shutter)`
          : "";
        return `+${oscillation}${footprint}`;
      })
      .join("") +
    ";"
  );
}
