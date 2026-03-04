/// Conservative f16 conversion — the ONLY code path for bound serialization.
/// Direction-aware: upper bounds round toward +inf, lower bounds toward -inf.

/// Direction for conservative rounding.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BoundDirection {
    Upper,
    Lower,
}

/// Convert f32 to f16 using round-to-nearest-even (IEEE 754 compliant).
pub fn f16_from_f32_rne(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = (bits >> 31) & 1;
    let exp = ((bits >> 23) & 0xFF) as i32;
    let frac = bits & 0x007F_FFFF;

    // Handle special cases
    if exp == 0xFF {
        // Inf or NaN
        if frac == 0 {
            // Infinity
            return (sign << 15 | 0x7C00) as u16;
        } else {
            // NaN — preserve sign, set quiet NaN
            return (sign << 15 | 0x7E00) as u16;
        }
    }

    // Rebias exponent: f32 bias=127, f16 bias=15
    let new_exp = exp - 127 + 15;

    if new_exp >= 31 {
        // Overflow to infinity
        return (sign << 15 | 0x7C00) as u16;
    }

    if new_exp <= 0 {
        // Subnormal or zero in f16
        if new_exp < -10 {
            // Too small, flush to zero
            return (sign << 15) as u16;
        }
        // Subnormal: shift mantissa right, add implicit 1
        let shift = (1 - new_exp) as u32;
        let full_frac = frac | 0x0080_0000; // add implicit 1 bit
        let shifted = full_frac >> (13 + shift);

        // Round-to-nearest-even
        let round_bit = 1u32 << (12 + shift);
        let sticky = full_frac & (round_bit - 1);
        let halfway = (full_frac & round_bit) != 0;

        let mut result = (sign << 15 | shifted) as u16;
        if halfway && (sticky != 0 || (shifted & 1) != 0) {
            result += 1;
        }
        return result;
    }

    // Normal number
    let shifted_frac = frac >> 13;
    let round_bit = frac & 0x1000;
    let sticky = frac & 0x0FFF;

    let mut result = (sign << 15 | (new_exp as u32) << 10 | shifted_frac) as u16;
    // Round-to-nearest-even
    if round_bit != 0 && (sticky != 0 || (shifted_frac & 1) != 0) {
        result += 1;
    }
    result
}

/// Convert f16 to f32.
pub fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1F) as u32;
    let frac = (h & 0x03FF) as u32;

    if exp == 0x1F {
        // Inf or NaN
        if frac == 0 {
            return f32::from_bits(sign << 31 | 0x7F80_0000);
        } else {
            return f32::from_bits(sign << 31 | 0x7FC0_0000 | (frac << 13));
        }
    }

    if exp == 0 {
        if frac == 0 {
            // Zero (signed)
            return f32::from_bits(sign << 31);
        }
        // Subnormal: normalize
        let mut f = frac;
        let mut e = 1i32;
        while (f & 0x0400) == 0 {
            f <<= 1;
            e -= 1;
        }
        f &= 0x03FF; // remove implicit 1
        let f32_exp = (127 - 15 + e) as u32;
        return f32::from_bits(sign << 31 | f32_exp << 23 | f << 13);
    }

    // Normal
    let f32_exp = (exp + 127 - 15) as u32;
    f32::from_bits(sign << 31 | f32_exp << 23 | frac << 13)
}

/// IEEE numeric successor for f16 (handles sign boundary correctly).
pub fn f16_next_up(h: u16) -> u16 {
    let sign = (h >> 15) & 1;
    let abs_bits = h & 0x7FFF;

    // NaN stays NaN
    if abs_bits > 0x7C00 {
        return h;
    }

    if abs_bits == 0 {
        // ±0 → smallest positive subnormal
        return 0x0001;
    }

    if sign == 0 {
        // Positive: increment
        if abs_bits == 0x7C00 {
            return h; // +Inf stays +Inf
        }
        h + 1
    } else {
        // Negative: decrement magnitude (toward zero)
        (1u16 << 15) | (abs_bits - 1)
    }
}

/// IEEE numeric predecessor for f16 (handles sign boundary correctly).
pub fn f16_next_down(h: u16) -> u16 {
    let sign = (h >> 15) & 1;
    let abs_bits = h & 0x7FFF;

    // NaN stays NaN
    if abs_bits > 0x7C00 {
        return h;
    }

    if abs_bits == 0 {
        // ±0 → smallest negative subnormal (i.e., -min_subnormal)
        return 0x8001;
    }

    if sign == 0 {
        // Positive: decrement
        h - 1
    } else {
        // Negative: increment magnitude (away from zero)
        if abs_bits == 0x7C00 {
            return h; // -Inf stays -Inf
        }
        h + 1
    }
}

/// Conservative f16 conversion with direction awareness.
/// Upper bounds round toward +inf, lower bounds toward -inf.
pub fn f16_conservative(value: f32, direction: BoundDirection) -> u16 {
    if value.is_nan() {
        return f16_from_f32_rne(value);
    }

    let mut h = f16_from_f32_rne(value);

    match direction {
        BoundDirection::Upper => {
            while f16_to_f32(h) < value {
                h = f16_next_up(h);
            }
        }
        BoundDirection::Lower => {
            while f16_to_f32(h) > value {
                h = f16_next_down(h);
            }
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_zero() {
        assert_eq!(f16_to_f32(f16_from_f32_rne(0.0)), 0.0);
        assert_eq!(f16_to_f32(f16_from_f32_rne(-0.0)), -0.0);
        // Check sign bits
        assert_eq!(f16_from_f32_rne(0.0), 0x0000);
        assert_eq!(f16_from_f32_rne(-0.0), 0x8000);
    }

    #[test]
    fn roundtrip_one() {
        let h = f16_from_f32_rne(1.0);
        assert_eq!(f16_to_f32(h), 1.0);
    }

    #[test]
    fn roundtrip_inf() {
        let h = f16_from_f32_rne(f32::INFINITY);
        assert_eq!(f16_to_f32(h), f32::INFINITY);
        let h = f16_from_f32_rne(f32::NEG_INFINITY);
        assert_eq!(f16_to_f32(h), f32::NEG_INFINITY);
    }

    #[test]
    fn nan_preserved() {
        let h = f16_from_f32_rne(f32::NAN);
        assert!(f16_to_f32(h).is_nan());
    }

    #[test]
    fn subnormal_roundtrip() {
        // Smallest positive f16 subnormal = 2^-24 ≈ 5.96e-8
        let h: u16 = 0x0001;
        let v = f16_to_f32(h);
        assert!(v > 0.0);
        assert!(v < 1e-4);
        // Roundtrip: convert back
        let h2 = f16_from_f32_rne(v);
        assert_eq!(h, h2);
    }

    #[test]
    fn next_up_from_zero() {
        assert_eq!(f16_next_up(0x0000), 0x0001); // +0 → min positive subnormal
        assert_eq!(f16_next_up(0x8000), 0x0001); // -0 → min positive subnormal
    }

    #[test]
    fn next_down_from_zero() {
        assert_eq!(f16_next_down(0x0000), 0x8001); // +0 → -min_subnormal
        assert_eq!(f16_next_down(0x8000), 0x8001); // -0 → -min_subnormal
    }

    #[test]
    fn next_up_positive() {
        let h = f16_from_f32_rne(1.0);
        let h_next = f16_next_up(h);
        assert!(f16_to_f32(h_next) > 1.0);
    }

    #[test]
    fn next_down_positive() {
        let h = f16_from_f32_rne(1.0);
        let h_prev = f16_next_down(h);
        assert!(f16_to_f32(h_prev) < 1.0);
    }

    #[test]
    fn next_up_inf_stays() {
        let h = f16_from_f32_rne(f32::INFINITY);
        assert_eq!(f16_next_up(h), h);
    }

    #[test]
    fn next_down_neg_inf_stays() {
        let h = f16_from_f32_rne(f32::NEG_INFINITY);
        assert_eq!(f16_next_down(h), h);
    }

    #[test]
    fn conservative_lower_invariant() {
        let test_values = [
            0.0, 1.0, -1.0, 0.5, -0.5, 100.0, -100.0, 0.001, -0.001,
            1.0 / 3.0, -1.0 / 3.0, std::f32::consts::PI, -std::f32::consts::PI,
        ];
        for &val in &test_values {
            let h = f16_conservative(val, BoundDirection::Lower);
            let roundtrip = f16_to_f32(h);
            assert!(
                roundtrip <= val,
                "Lower bound violation: f16_to_f32({:#06x}) = {} > {}",
                h, roundtrip, val
            );
        }
    }

    #[test]
    fn conservative_upper_invariant() {
        let test_values = [
            0.0, 1.0, -1.0, 0.5, -0.5, 100.0, -100.0, 0.001, -0.001,
            1.0 / 3.0, -1.0 / 3.0, std::f32::consts::PI, -std::f32::consts::PI,
        ];
        for &val in &test_values {
            let h = f16_conservative(val, BoundDirection::Upper);
            let roundtrip = f16_to_f32(h);
            assert!(
                roundtrip >= val,
                "Upper bound violation: f16_to_f32({:#06x}) = {} < {}",
                h, roundtrip, val
            );
        }
    }

    #[test]
    fn conservative_nan() {
        let h = f16_conservative(f32::NAN, BoundDirection::Lower);
        assert!(f16_to_f32(h).is_nan());
    }

    #[test]
    fn edge_corpus_comprehensive() {
        let corpus: Vec<f32> = vec![
            0.0, -0.0,
            f32::INFINITY, f32::NEG_INFINITY,
            f32::MIN_POSITIVE, -f32::MIN_POSITIVE,
            65504.0, -65504.0,  // max finite f16
            65520.0, -65520.0,  // just above max f16
            6.0e-8, -6.0e-8,   // near f16 subnormal range
            1.0e-10, -1.0e-10,  // below f16 subnormal
        ];
        for &val in &corpus {
            if val.is_nan() {
                continue;
            }
            if val.is_finite() {
                let lower = f16_conservative(val, BoundDirection::Lower);
                let upper = f16_conservative(val, BoundDirection::Upper);
                assert!(
                    f16_to_f32(lower) <= val || val.abs() < 1e-45,
                    "Lower violated for {}",
                    val
                );
                assert!(
                    f16_to_f32(upper) >= val || val.abs() < 1e-45,
                    "Upper violated for {}",
                    val
                );
            }
        }
    }
}
