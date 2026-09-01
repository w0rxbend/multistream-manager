//! Spreading repeated work out in time so that many clients, or many chats in
//! one client, do not all act on the same tick.
//!
//! Every retry ladder in this program is exponential, and an exponential
//! ladder without jitter synchronises: two chats that drop at the same moment
//! retry at the same moment, back off by the same factor, and retry together
//! again — hammering whatever just failed in step. Adding a small random
//! spread to each delay breaks that up.

/// A cheap linear congruential generator for jitter.
///
/// Jitter needs speed and spread, not unpredictability, so no crypto-grade
/// randomness is involved — and deliberately so: a seeded, reproducible
/// sequence is what lets the delays be tested.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Lcg(u64);

impl Lcg {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    /// Seed from a string — a channel name, a video id — so that two chats
    /// never share a jitter sequence, which is the whole point of jittering.
    pub(crate) fn seeded_by(text: &str) -> Self {
        // FNV-1a: short, dependency-free, and well spread for short keys.
        let seed = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |acc, byte| {
            (acc ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
        Self::new(seed)
    }

    /// A uniform draw in [0, 1).
    pub(crate) fn next_f64(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }

    /// Spread `delay` by up to `fraction` either side of itself.
    ///
    /// With `fraction` of 0.1 the result is uniform in `delay ± 10%`. Never
    /// returns zero for a non-zero delay, so a jittered retry can never
    /// become a busy loop.
    pub(crate) fn spread(
        &mut self,
        delay: std::time::Duration,
        fraction: f64,
    ) -> std::time::Duration {
        if delay.is_zero() {
            return delay;
        }
        let spread = delay.mul_f64(fraction);
        (delay + spread.mul_f64(2.0 * self.next_f64())).saturating_sub(spread)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn two_different_keys_produce_two_different_sequences() {
        let mut one = Lcg::seeded_by("#alice");
        let mut other = Lcg::seeded_by("#bob");
        assert_ne!(one.next_f64(), other.next_f64());
    }

    #[test]
    fn the_same_key_always_produces_the_same_sequence() {
        assert_eq!(
            Lcg::seeded_by("#alice").next_f64(),
            Lcg::seeded_by("#alice").next_f64()
        );
    }

    #[test]
    fn a_draw_is_always_inside_the_unit_interval() {
        let mut lcg = Lcg::new(1);
        for _ in 0..1000 {
            let draw = lcg.next_f64();
            assert!((0.0..1.0).contains(&draw), "out of range: {draw}");
        }
    }

    #[test]
    fn spreading_stays_within_the_requested_fraction_and_never_reaches_zero() {
        let mut lcg = Lcg::seeded_by("#chan");
        let base = Duration::from_secs(10);
        for _ in 0..1000 {
            let jittered = lcg.spread(base, 0.1);
            assert!(
                jittered >= Duration::from_secs(9) && jittered <= Duration::from_secs(11),
                "outside ±10%: {jittered:?}"
            );
            assert!(!jittered.is_zero());
        }
    }
}
