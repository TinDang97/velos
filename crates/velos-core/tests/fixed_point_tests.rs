//! Tests for fixed-point arithmetic types (Q16.16, Q12.20, Q8.8).
//!
//! These types ensure cross-GPU determinism for 280K traffic agents.
//! All position and speed calculations use integer arithmetic to guarantee
//! bit-identical results across different GPU vendors.

use velos_core::fixed_point::{FixLat, FixPos, FixSpd, fix_mul_mixed, fix_mul_q16};

// ---------------------------------------------------------------------------
// FixPos (Q16.16) conversion tests
// ---------------------------------------------------------------------------

#[test]
fn fixpos_roundtrip_f64_within_tolerance() {
    let values = [0.0, 1.0, -1.0, 100.5, -32768.0, 32767.0, 0.00001525878];
    let tolerance = 1.0 / 65536.0; // Q16.16 resolution
    for &v in &values {
        let fp = FixPos::from_f64(v);
        let back = fp.to_f64();
        assert!(
            (back - v).abs() <= tolerance,
            "FixPos roundtrip failed for {v}: got {back}, tolerance {tolerance}"
        );
    }
}

#[test]
fn fixpos_roundtrip_f32_within_tolerance() {
    let values = [0.0f32, 1.0, -1.0, 100.5, 0.5];
    let tolerance = 1.0 / 65536.0;
    for &v in &values {
        let fp = FixPos::from_f32(v);
        let back = fp.to_f32();
        assert!(
            (back - v).abs() <= tolerance,
            "FixPos f32 roundtrip failed for {v}: got {back}"
        );
    }
}

#[test]
fn fixpos_zero() {
    let fp = FixPos::from_f64(0.0);
    assert_eq!(fp.raw(), 0);
    assert_eq!(fp.to_f64(), 0.0);
}

// ---------------------------------------------------------------------------
// FixSpd (Q12.20) conversion tests
// ---------------------------------------------------------------------------

#[test]
fn fixspd_roundtrip_f64_within_tolerance() {
    let values = [0.0, 1.0, -1.0, 13.89, -2048.0, 2047.0];
    let tolerance = 1.0 / 1_048_576.0; // Q12.20 resolution
    for &v in &values {
        let fs = FixSpd::from_f64(v);
        let back = fs.to_f64();
        assert!(
            (back - v).abs() <= tolerance,
            "FixSpd roundtrip failed for {v}: got {back}, tolerance {tolerance}"
        );
    }
}

#[test]
fn fixspd_roundtrip_f32_within_tolerance() {
    let tolerance = 1.0 / 1_048_576.0;
    let fs = FixSpd::from_f32(13.89);
    let back = fs.to_f32();
    assert!(
        (back - 13.89).abs() <= tolerance,
        "FixSpd f32 roundtrip failed: got {back}"
    );
}

// ---------------------------------------------------------------------------
// FixLat (Q8.8) conversion tests
// ---------------------------------------------------------------------------

#[test]
fn fixlat_roundtrip_f64_within_tolerance() {
    let values = [0.0, 1.0, -1.0, 3.5, -128.0, 127.0];
    let tolerance = 1.0 / 256.0; // Q8.8 resolution
    for &v in &values {
        let fl = FixLat::from_f64(v);
        let back = fl.to_f64();
        assert!(
            (back - v).abs() <= tolerance,
            "FixLat roundtrip failed for {v}: got {back}, tolerance {tolerance}"
        );
    }
}

// ---------------------------------------------------------------------------
// fix_mul_q16 tests
// ---------------------------------------------------------------------------

#[test]
fn fix_mul_q16_one_times_two() {
    let one = FixPos::from_f64(1.0);
    let two = FixPos::from_f64(2.0);
    let result = fix_mul_q16(one.raw(), two.raw());
    let expected = FixPos::from_f64(2.0).raw();
    assert_eq!(result, expected, "1.0 * 2.0 should equal 2.0 in Q16.16");
}

#[test]
fn fix_mul_q16_handles_max_position_without_overflow() {
    // 65535m * 1.0 should not overflow
    let max_pos = FixPos::from_f64(32767.0); // safe max (half-range to avoid sign issues)
    let one = FixPos::from_f64(1.0);
    let result = fix_mul_q16(max_pos.raw(), one.raw());
    assert_eq!(result, max_pos.raw(), "max_pos * 1.0 should equal max_pos");
}

#[test]
fn fix_mul_q16_negative_values() {
    let neg_one = FixPos::from_f64(-1.0);
    let two = FixPos::from_f64(2.0);
    let result = fix_mul_q16(neg_one.raw(), two.raw());
    let expected = FixPos::from_f64(-2.0).raw();
    assert_eq!(result, expected, "-1.0 * 2.0 should equal -2.0");
}

#[test]
fn fix_mul_q16_negative_times_negative() {
    let neg_three = FixPos::from_f64(-3.0);
    let neg_two = FixPos::from_f64(-2.0);
    let result = fix_mul_q16(neg_three.raw(), neg_two.raw());
    let expected = FixPos::from_f64(6.0).raw();
    assert_eq!(result, expected, "-3.0 * -2.0 should equal 6.0");
}

#[test]
fn fix_mul_q16_zero() {
    let zero = FixPos::from_f64(0.0);
    let five = FixPos::from_f64(5.0);
    assert_eq!(fix_mul_q16(zero.raw(), five.raw()), 0);
}

#[test]
fn fix_mul_q16_fractional() {
    let half = FixPos::from_f64(0.5);
    let four = FixPos::from_f64(4.0);
    let result = fix_mul_q16(half.raw(), four.raw());
    let expected = FixPos::from_f64(2.0).raw();
    assert_eq!(result, expected, "0.5 * 4.0 should equal 2.0");
}

// ---------------------------------------------------------------------------
// Addition and subtraction tests
// ---------------------------------------------------------------------------

#[test]
fn fixpos_addition() {
    let a = FixPos::from_f64(1.5);
    let b = FixPos::from_f64(2.25);
    let sum = a + b;
    let expected = FixPos::from_f64(3.75);
    assert_eq!(sum, expected, "1.5 + 2.25 should equal 3.75");
}

#[test]
fn fixpos_subtraction() {
    let a = FixPos::from_f64(5.0);
    let b = FixPos::from_f64(3.0);
    let diff = a - b;
    let expected = FixPos::from_f64(2.0);
    assert_eq!(diff, expected, "5.0 - 3.0 should equal 2.0");
}

#[test]
fn fixpos_negation() {
    let a = FixPos::from_f64(3.0);
    let neg = -a;
    let expected = FixPos::from_f64(-3.0);
    assert_eq!(neg, expected, "neg(3.0) should equal -3.0");
}

// ---------------------------------------------------------------------------
// Ordering tests
// ---------------------------------------------------------------------------

#[test]
fn fixpos_ordering() {
    let a = FixPos::from_f64(1.0);
    let b = FixPos::from_f64(2.0);
    assert!(a < b);
    assert!(b > a);

    let c = FixPos::from_f64(-1.0);
    assert!(c < a);
}

// ---------------------------------------------------------------------------
// Mixed-format multiplication: speed * dt -> displacement
// ---------------------------------------------------------------------------

#[test]
fn fix_mul_mixed_speed_times_dt() {
    // speed = 13.89 m/s (50 km/h), dt = 0.1s -> displacement ~1.389m
    let spd = FixSpd::from_f64(13.89);
    let dt = FixPos::from_f64(0.1);
    let result = fix_mul_mixed(spd, dt);
    let tolerance = 0.001; // 1mm tolerance
    let result_f64 = result.to_f64();
    let expected = 13.89 * 0.1;
    assert!(
        (result_f64 - expected).abs() < tolerance,
        "speed({}) * dt(0.1) = {result_f64}, expected ~{expected}",
        spd.to_f64()
    );
}

#[test]
fn fix_mul_mixed_zero_speed() {
    let spd = FixSpd::from_f64(0.0);
    let dt = FixPos::from_f64(0.1);
    let result = fix_mul_mixed(spd, dt);
    assert_eq!(result.raw(), 0, "zero speed * dt should be zero displacement");
}
