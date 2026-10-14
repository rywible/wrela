#!/usr/bin/env python3
"""Compare independent and relational bounds for affine float32 branches.

This is a compiler-analysis experiment, not an optimizer or a renderer. It
counts branches retained under min; it makes no runtime or frame-time claim.
Only multiply by constants and addition are modeled, with separate float32
round-to-nearest operations. There is no WGSL error, FMA, or overflow model.
Inputs and constants must be finite float32 values of magnitude at most 2**16.
Exact rational analysis includes a conservative error for every operation.
"""

from fractions import Fraction
import itertools
import json
import random
import struct


SEED = 0x5EED
UNIT_ROUNDOFF = Fraction(1, 2**24)
TINY = Fraction(1, 2**149)


def f32(value):
    return struct.unpack("<f", struct.pack("<f", float(value)))[0]


def interval(coefficients, bias, lo, hi):
    """Exact real-arithmetic range of an affine expression over a box."""
    return (
        bias + sum(min(a * l, a * h) for a, l, h in zip(coefficients, lo, hi)),
        bias + sum(max(a * l, a * h) for a, l, h in zip(coefficients, lo, hi)),
    )


def rounding_error(coefficients, bias, lo, hi):
    """Bound the absolute error of ((a*x + b*y) + c*z) + bias."""
    magnitudes = [
        abs(a) * max(abs(l), abs(h)) for a, l, h in zip(coefficients, lo, hi)
    ]
    errors = [UNIT_ROUNDOFF * magnitude + TINY for magnitude in magnitudes]
    magnitude, error = magnitudes[0], errors[0]
    for next_magnitude, next_error in [
        *zip(magnitudes[1:], errors[1:]), (abs(bias), Fraction(0))
    ]:
        error += next_error + UNIT_ROUNDOFF * (
            magnitude + next_magnitude + error + next_error
        ) + TINY
        magnitude += next_magnitude
    return error


def value(branch, point):
    """Evaluate separate float32 operations; coefficients and inputs are f32."""
    coefficients, bias = branch
    terms = [f32(f32(a) * f32(x)) for a, x in zip(coefficients, point)]
    return f32(f32(f32(terms[0] + terms[1]) + terms[2]) + f32(bias))


def live_branches(branches, lo, hi):
    """Return independent survivors, relational survivors, and comparison count."""
    if not branches or len(lo) != 3 or len(hi) != 3:
        raise ValueError("expected affine branches and a three-dimensional box")
    if any(l > h for l, h in zip(lo, hi)):
        raise ValueError("box bounds are reversed")
    constants = list(lo) + list(hi)
    for coefficients, bias in branches:
        if len(coefficients) != 3:
            raise ValueError("expected three coefficients per branch")
        constants.extend((*coefficients, bias))
    if any(abs(x) > 2**16 or Fraction(f32(x)) != x for x in constants):
        raise ValueError("expected float32 constants and bounds within +/-2**16")

    ranges = [interval(c, b, lo, hi) for c, b in branches]
    errors = [rounding_error(c, b, lo, hi) for c, b in branches]
    best_hi = min(h + error for (_, h), error in zip(ranges, errors))
    independent = [
        i for i, ((l, _), error) in enumerate(zip(ranges, errors))
        if l - error <= best_hi
    ]
    relational = []
    comparisons = 0
    for i, (ci, bi) in enumerate(branches):
        for j, (cj, bj) in enumerate(branches):
            if i == j:
                continue
            comparisons += 1
            # Subtract exact coefficients in the analysis. Do not reassociate
            # the program's floating-point expression. Strict dominance keeps
            # ties, including their signed-zero and derivative conventions.
            _, upper = interval(tuple(a - b for a, b in zip(cj, ci)), bj - bi, lo, hi)
            if upper + errors[i] + errors[j] < 0:
                break
        else:
            relational.append(i)
    return independent, relational, comparisons


def probe(name, branches, boxes, random_samples=127, seed=SEED):
    """Count work and verify result bits at corners, centers and seeded points."""
    rng = random.Random(seed)
    total_independent = total_relational = total_comparisons = checked = 0
    min_keep, max_keep = len(branches), 0
    for lo, hi in boxes:
        independent, live, comparisons = live_branches(branches, lo, hi)
        assert live and set(live).issubset(independent)
        total_independent += len(independent)
        total_relational += len(live)
        total_comparisons += comparisons
        min_keep, max_keep = min(min_keep, len(live)), max(max_keep, len(live))
        points = list(itertools.product(*zip(lo, hi)))
        points.append(tuple((l + h) / 2 for l, h in zip(lo, hi)))
        points.extend(
            tuple(l + (h - l) * Fraction(rng.randrange(65537), 65536)
                  for l, h in zip(lo, hi))
            for _ in range(random_samples)
        )
        for point in points:
            # Generated points in run() are exact f32 dyadics. Reject other
            # inputs instead of testing a rounded point outside its box.
            if any(Fraction(f32(x)) != x for x in point):
                raise ValueError("probe points must be exactly representable as float32")
            original = min(value(branch, point) for branch in branches)
            reduced = min(value(branches[i], point) for i in live)
            assert struct.pack("<f", original) == struct.pack("<f", reduced), (
                name, lo, point, original, reduced
            )
            checked += 1
    return {
        "case": name,
        "boxes": len(boxes),
        "branches_before": len(branches),
        "point_checks": checked,
        "bitwise_mismatches": 0,
        "independent_interval_live_branches_sum": total_independent,
        "relational_live_branches_sum": total_relational,
        "relational_live_branches_min": min_keep,
        "relational_live_branches_max": max_keep,
        "branch_evaluation_reduction_vs_plain": round(total_independent / total_relational, 3),
        "pairwise_analysis_comparisons": total_comparisons,
        "semantics": "float32 round-to-nearest; rational forward-error enclosure",
        "measurement": "branch work counts only, not runtime or frame time",
    }


def run():
    rng = random.Random(SEED)
    # Large shared slopes make independent ranges overlap; small differing
    # slopes prevent factoring one identical expression out of every branch.
    correlated = [
        (tuple(Fraction(16) + Fraction(rng.randrange(-8, 9), 64) for _ in range(3)),
         Fraction(i, 8))
        for i in range(64)
    ]
    # Opposing planes intersect: many branches remain relevant. This control
    # exposes both weaker work elimination and higher analysis cost.
    unrelated = [
        (tuple(Fraction(rng.randrange(-1024, 1025), 64) for _ in range(3)), Fraction(0))
        for _ in range(64)
    ]
    boxes = [
        (tuple(map(Fraction, lower)), tuple(Fraction(x + 2) for x in lower))
        for lower in itertools.product(range(-4, 4, 2), repeat=3)
    ]
    return {
        "seed": hex(SEED),
        "results": [
            probe("correlated_affine_min64", correlated, boxes),
            probe("unrelated_affine_min64", unrelated, boxes),
        ],
    }


if __name__ == "__main__":
    print(json.dumps(run(), indent=2))
