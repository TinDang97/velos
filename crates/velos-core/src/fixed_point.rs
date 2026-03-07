//! Fixed-point arithmetic types for cross-GPU determinism.
//!
//! Three types model position, speed, and lateral offset using integer
//! arithmetic to guarantee bit-identical results across GPU vendors:
//!
//! - [`FixPos`] (Q16.16): 16 integer bits, 16 fractional bits. Range: \[-32768, 32767\] m.
//! - [`FixSpd`] (Q12.20): 12 integer bits, 20 fractional bits. Range: \[-2048, 2047\] m/s.
//! - [`FixLat`] (Q8.8): 8 integer bits, 8 fractional bits. Range: \[-128, 127\] m.
//!
//! All types wrap `i32` and use `#[repr(transparent)]` for zero-cost GPU
//! buffer interop via bytemuck.

use std::ops::{Add, Neg, Sub};

// -- Constants ---------------------------------------------------------------

const POS_FRAC_BITS: u32 = 16;
const SPD_FRAC_BITS: u32 = 20;
const LAT_FRAC_BITS: u32 = 8;

const POS_SCALE: f64 = (1u32 << POS_FRAC_BITS) as f64; // 65536.0
const SPD_SCALE: f64 = (1u32 << SPD_FRAC_BITS) as f64; // 1048576.0
const LAT_SCALE: f64 = (1u32 << LAT_FRAC_BITS) as f64; // 256.0

const POS_SCALE_F32: f32 = (1u32 << POS_FRAC_BITS) as f32;
const SPD_SCALE_F32: f32 = (1u32 << SPD_FRAC_BITS) as f32;

// -- FixPos (Q16.16) ---------------------------------------------------------

/// Position in Q16.16 fixed-point (16 integer bits, 16 fractional bits).
///
/// Range: \[-32768, 32767\] metres. Resolution: ~0.015 mm.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(transparent)]
pub struct FixPos(i32);

impl FixPos {
    /// Convert from `f64` to Q16.16.
    #[inline]
    pub fn from_f64(v: f64) -> Self {
        Self((v * POS_SCALE).round() as i32)
    }

    /// Convert to `f64`.
    #[inline]
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / POS_SCALE
    }

    /// Convert from `f32` to Q16.16.
    #[inline]
    pub fn from_f32(v: f32) -> Self {
        Self((v * POS_SCALE_F32).round() as i32)
    }

    /// Convert to `f32`.
    #[inline]
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / POS_SCALE_F32
    }

    /// Access the raw `i32` representation.
    #[inline]
    pub fn raw(self) -> i32 {
        self.0
    }

    /// Construct from a raw `i32` value.
    #[inline]
    pub fn from_raw(raw: i32) -> Self {
        Self(raw)
    }
}

impl Add for FixPos {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0.wrapping_add(rhs.0))
    }
}

impl Sub for FixPos {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.wrapping_sub(rhs.0))
    }
}

impl Neg for FixPos {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self(self.0.wrapping_neg())
    }
}

// -- FixSpd (Q12.20) ---------------------------------------------------------

/// Speed in Q12.20 fixed-point (12 integer bits, 20 fractional bits).
///
/// Range: \[-2048, 2047\] m/s. Resolution: ~0.001 mm/s.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(transparent)]
pub struct FixSpd(i32);

impl FixSpd {
    /// Convert from `f64` to Q12.20.
    #[inline]
    pub fn from_f64(v: f64) -> Self {
        Self((v * SPD_SCALE).round() as i32)
    }

    /// Convert to `f64`.
    #[inline]
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / SPD_SCALE
    }

    /// Convert from `f32` to Q12.20.
    #[inline]
    pub fn from_f32(v: f32) -> Self {
        Self((v * SPD_SCALE_F32).round() as i32)
    }

    /// Convert to `f32`.
    #[inline]
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / SPD_SCALE_F32
    }

    /// Access the raw `i32` representation.
    #[inline]
    pub fn raw(self) -> i32 {
        self.0
    }

    /// Construct from a raw `i32` value.
    #[inline]
    pub fn from_raw(raw: i32) -> Self {
        Self(raw)
    }
}

impl Add for FixSpd {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0.wrapping_add(rhs.0))
    }
}

impl Sub for FixSpd {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.wrapping_sub(rhs.0))
    }
}

impl Neg for FixSpd {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self(self.0.wrapping_neg())
    }
}

// -- FixLat (Q8.8) -----------------------------------------------------------

/// Lateral offset in Q8.8 fixed-point (8 integer bits, 8 fractional bits).
///
/// Range: \[-128, 127\] metres. Resolution: ~3.9 mm.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(transparent)]
pub struct FixLat(i32);

impl FixLat {
    /// Convert from `f64` to Q8.8 (stored in i32 for alignment).
    #[inline]
    pub fn from_f64(v: f64) -> Self {
        Self((v * LAT_SCALE).round() as i32)
    }

    /// Convert to `f64`.
    #[inline]
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / LAT_SCALE
    }

    /// Access the raw `i32` representation.
    #[inline]
    pub fn raw(self) -> i32 {
        self.0
    }

    /// Construct from a raw `i32` value.
    #[inline]
    pub fn from_raw(raw: i32) -> Self {
        Self(raw)
    }
}

impl Add for FixLat {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0.wrapping_add(rhs.0))
    }
}

impl Sub for FixLat {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.wrapping_sub(rhs.0))
    }
}

impl Neg for FixLat {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self(self.0.wrapping_neg())
    }
}

// -- Multiplication ----------------------------------------------------------

/// Multiply two Q16.16 fixed-point values without i64 overflow.
///
/// Uses the 16-bit half-splitting technique: split each operand into
/// high (integer) and low (fractional) 16-bit halves, compute partial
/// products in `u32`, then reassemble. Handles sign separately.
///
/// This mirrors the WGSL `fix_mul_q16` shader function exactly, so
/// CPU and GPU produce identical results.
#[inline]
pub fn fix_mul_q16(a: i32, b: i32) -> i32 {
    if a == 0 || b == 0 {
        return 0;
    }

    let sign = if (a < 0) != (b < 0) { -1i32 } else { 1i32 };

    let ua = (a as i64).unsigned_abs() as u32;
    let ub = (b as i64).unsigned_abs() as u32;

    let ah = ua >> 16;
    let al = ua & 0xFFFF;
    let bh = ub >> 16;
    let bl = ub & 0xFFFF;

    // Full product in Q32.32 terms, but we need to shift right by 16
    // to get back to Q16.16:
    //   result = (ah*bh) << 16 + ah*bl + al*bh + (al*bl) >> 16
    //
    // Using u64 for the accumulation to avoid overflow in intermediate sums.
    let result = ((ah as u64 * bh as u64) << 16)
        + (ah as u64 * bl as u64)
        + (al as u64 * bh as u64)
        + ((al as u64 * bl as u64) >> 16);

    (result as i32) * sign
}

/// Multiply speed (Q12.20) by timestep (Q16.16) to produce displacement (Q16.16).
///
/// Cross-format multiplication: the product of Q12.20 * Q16.16 is Q28.36.
/// We need to shift right by 20 (the speed fractional bits) to get Q16.16.
///
/// Uses i64 intermediate to handle the cross-format shift correctly.
/// The CPU reference uses i64; the GPU shader version uses the
/// half-splitting technique with adjusted shifts.
#[inline]
pub fn fix_mul_mixed(spd: FixSpd, dt: FixPos) -> FixPos {
    let product = spd.raw() as i64 * dt.raw() as i64;
    // Q12.20 * Q16.16 = Q28.36; shift right by 20 to get Q16.16
    FixPos::from_raw((product >> SPD_FRAC_BITS) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fix_mul_q16_large_values() {
        // 1000.0 * 30.0 = 30000.0
        let a = FixPos::from_f64(1000.0);
        let b = FixPos::from_f64(30.0);
        let result = fix_mul_q16(a.raw(), b.raw());
        let expected = FixPos::from_f64(30000.0).raw();
        assert_eq!(result, expected);
    }
}
