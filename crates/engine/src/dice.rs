//! Deterministic dice.
pub use rand_chacha::rand_core::{RngCore, SeedableRng};
pub use rand_chacha::ChaCha8Rng;

/// The RNG type used throughout the engine.
pub type Rng = ChaCha8Rng;

/// Build the per-action RNG from a seed.
pub fn rng_from_seed(seed: u64) -> Rng {
    ChaCha8Rng::seed_from_u64(seed)
}

/// Roll one six-sided die without modulo bias.
pub fn d6(rng: &mut Rng) -> u8 {
    loop {
        let x = rng.next_u32();
        if x < 4_294_967_292 {
            return (x % 6 + 1) as u8;
        }
    }
}

/// Roll `n` dice.
pub fn roll(rng: &mut Rng, n: u32) -> Vec<u8> {
    (0..n).map(|_| d6(rng)).collect()
}
