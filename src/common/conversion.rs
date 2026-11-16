use std::num::NonZeroU64;

/// Coq `positive`  →  `NonZeroU64`
/// Coq `N`         →  `u64`
/// Coq `nat`       →  `usize`
/// Coq `int` / `Z` →  `i64`
/// pos2N : positive → N
///   Npos p  (the positive is already a natural number > 0)
pub fn pos2n(p: NonZeroU64) -> u64 {
    p.get()
}

/// nat2N : nat → N
///   N.of_nat i
pub fn nat2n(i: usize) -> u64 {
    i as u64
}

/// i2p : int → positive
///   Negative or zero inputs are clamped to 1 (smallest positive).
pub fn i2p(i: i64) -> NonZeroU64 {
    let clamped = if i <= 0 { 1u64 } else { i as u64 };
    // SAFETY: clamped is always >= 1
    unsafe { NonZeroU64::new_unchecked(clamped) }
}

/// p2i : positive → int
///   Values above i64::MAX are saturated.
pub fn p2i(p: NonZeroU64) -> i64 {
    p.get().min(i64::MAX as u64) as i64
}

/// i2z : int → Z
///   int and Z are both represented as i64; this is the identity.
pub fn i2z(i: i64) -> i64 {
    i
}

/// z2i : Z → int
///   Identity in the Rust representation.
pub fn z2i(z: i64) -> i64 {
    z
}

/// i2n : int → N
///   Negative values map to 0 (no natural number below zero).
pub fn i2n(i: i64) -> u64 {
    if i < 0 { 0 } else { i as u64 }
}

/// n2i : N → int
///   Values above i64::MAX are saturated.
pub fn n2i(n: u64) -> i64 {
    n.min(i64::MAX as u64) as i64
}

/// i2s : int → string
pub fn i2s(i: i64) -> String {
    i.to_string()
}
