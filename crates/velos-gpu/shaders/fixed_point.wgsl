// Fixed-point arithmetic functions for WGSL compute shaders.
// Matches CPU Rust types in velos-core/src/fixed_point.rs.
//
// Type aliases:
//   FixPos = i32 (Q16.16) -- position along edge in metres
//   FixSpd = i32 (Q12.20) -- speed in m/s
//   FixLat = i32 (Q8.8)   -- lateral offset in metres

alias FixPos = i32;
alias FixSpd = i32;
alias FixLat = i32;

const POS_FRAC: u32 = 16u;
const SPD_FRAC: u32 = 20u;
const POS_SCALE: f32 = 65536.0;
const SPD_SCALE: f32 = 1048576.0;

/// Multiply two Q16.16 values using 16-bit half-splitting.
/// Handles sign separately to avoid signed overflow in WGSL.
fn fix_mul_q16(a: i32, b: i32) -> i32 {
    if a == 0 || b == 0 {
        return 0;
    }

    // Determine result sign
    let neg_a = a < 0;
    let neg_b = b < 0;
    let result_neg = neg_a != neg_b;

    // Work with absolute values as u32
    var ua: u32;
    var ub: u32;
    if neg_a {
        ua = u32(-a);
    } else {
        ua = u32(a);
    }
    if neg_b {
        ub = u32(-b);
    } else {
        ub = u32(b);
    }

    let ah = ua >> 16u;
    let al = ua & 0xFFFFu;
    let bh = ub >> 16u;
    let bl = ub & 0xFFFFu;

    // Partial products reassembled with shift:
    //   result = (ah*bh) << 16 + ah*bl + al*bh + (al*bl) >> 16
    // Each partial fits in u32 (16-bit * 16-bit = 32-bit max).
    let hh = ah * bh;
    let hl = ah * bl;
    let lh = al * bh;
    let ll = al * bl;

    let result = (hh << 16u) + hl + lh + (ll >> 16u);

    if result_neg {
        return -i32(result);
    }
    return i32(result);
}

/// Convert f32 to Q16.16 fixed-point.
fn f32_to_fixpos(v: f32) -> FixPos {
    return i32(round(v * POS_SCALE));
}

/// Convert Q16.16 fixed-point to f32.
fn fixpos_to_f32(v: FixPos) -> f32 {
    return f32(v) / POS_SCALE;
}

/// Convert f32 to Q12.20 fixed-point.
fn f32_to_fixspd(v: f32) -> FixSpd {
    return i32(round(v * SPD_SCALE));
}

/// Convert Q12.20 fixed-point to f32.
fn fixspd_to_f32(v: FixSpd) -> f32 {
    return f32(v) / SPD_SCALE;
}

/// Multiply speed (Q12.20) by dt (Q16.16) to produce displacement (Q16.16).
/// Cross-format: Q12.20 * Q16.16 = Q28.36; shift right by 20 to get Q16.16.
/// Uses half-splitting to stay within u32 arithmetic.
fn fix_speed_dt_to_pos(speed: FixSpd, dt_pos: FixPos) -> FixPos {
    if speed == 0 || dt_pos == 0 {
        return 0;
    }

    let neg_s = speed < 0;
    let neg_d = dt_pos < 0;
    let result_neg = neg_s != neg_d;

    var us: u32;
    var ud: u32;
    if neg_s {
        us = u32(-speed);
    } else {
        us = u32(speed);
    }
    if neg_d {
        ud = u32(-dt_pos);
    } else {
        ud = u32(dt_pos);
    }

    // Split speed into 16-bit halves
    let sh = us >> 16u;
    let sl = us & 0xFFFFu;
    // Split dt into 16-bit halves
    let dh = ud >> 16u;
    let dl = ud & 0xFFFFu;

    // Full product needs shift right by SPD_FRAC (20).
    // Partial products:
    //   sh*dh has implicit shift of 32
    //   sh*dl and sl*dh have implicit shift of 16
    //   sl*dl has implicit shift of 0
    // After shifting all right by 20:
    let hh = sh * dh; // shift 32, after >>20 = <<12
    let hl = sh * dl; // shift 16, after >>20 = >>4
    let lh = sl * dh; // shift 16, after >>20 = >>4
    let ll = sl * dl; // shift 0, after >>20 = >>20

    let result = (hh << 12u) + (hl >> 4u) + (lh >> 4u) + (ll >> 20u);

    if result_neg {
        return -i32(result);
    }
    return i32(result);
}
