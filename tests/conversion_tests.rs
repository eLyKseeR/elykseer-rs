use elykseer_lib::common::conversion::{i2n, i2p, i2s, i2z, n2i, nat2n, p2i, pos2n, z2i};
use std::num::NonZeroU64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pos2n_returns_inner_value() {
        let p = NonZeroU64::new(42).unwrap();
        assert_eq!(pos2n(p), 42);
    }

    #[test]
    fn nat2n_casts_usize() {
        assert_eq!(nat2n(0), 0u64);
        assert_eq!(nat2n(100), 100u64);
    }

    #[test]
    fn i2p_positive_input() {
        assert_eq!(i2p(5).get(), 5);
    }

    #[test]
    fn i2p_clamps_zero_and_negative() {
        assert_eq!(i2p(0).get(), 1);
        assert_eq!(i2p(-99).get(), 1);
    }

    #[test]
    fn p2i_round_trips() {
        let p = NonZeroU64::new(7).unwrap();
        assert_eq!(p2i(p), 7);
    }

    #[test]
    fn p2i_saturates_at_i64_max() {
        let p = NonZeroU64::new(u64::MAX).unwrap();
        assert_eq!(p2i(p), i64::MAX);
    }

    #[test]
    fn i2z_z2i_identity() {
        assert_eq!(i2z(42), 42);
        assert_eq!(z2i(-1), -1);
    }

    #[test]
    fn i2n_negative_becomes_zero() {
        assert_eq!(i2n(-5), 0);
        assert_eq!(i2n(0), 0);
        assert_eq!(i2n(10), 10);
    }

    #[test]
    fn n2i_saturates_at_i64_max() {
        assert_eq!(n2i(u64::MAX), i64::MAX);
        assert_eq!(n2i(100), 100);
    }

    #[test]
    fn i2s_formats_correctly() {
        assert_eq!(i2s(0), "0");
        assert_eq!(i2s(-3), "-3");
        assert_eq!(i2s(42), "42");
    }
}
