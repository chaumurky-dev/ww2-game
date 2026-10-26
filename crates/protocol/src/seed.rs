//! Deterministic seed derivation shared by the server and the wasm client (replays).
use crate::game::Power;

/// FNV-1a (64 bit) over `bytes`, finished with a splitmix64 avalanche so that nearby inputs
/// (consecutive seqs) give unrelated seeds. Stable forever: stored games replay from it.
fn stable_hash(domain: &[u8], parts: &[u64]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    feed(domain);
    for p in parts {
        feed(&p.to_le_bytes());
    }
    let mut z = h.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// RNG seed for action number `seq` of a game: `FNV1a("wartide/action" ++ le(game_seed) ++ le(seq))`
/// run through a splitmix64 finaliser. Replays use the same function.
pub fn action_seed(game_seed: u64, seq: i64) -> u64 {
    stable_hash(b"wartide/action", &[game_seed, seq as u64])
}

/// Seed of the computer player for `power` in a game.
pub fn bot_seed(game_seed: u64, power: Power) -> u64 {
    stable_hash(b"wartide/bot", &[game_seed, power as u64])
}

/// Seed of the random-policy fallback for one decision (`seq` = last applied action) and attempt.
pub fn fallback_seed(game_seed: u64, seq: i64, attempt: u32) -> u64 {
    stable_hash(
        b"wartide/fallback",
        &[game_seed, seq as u64, u64::from(attempt)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_are_stable_and_distinct() {
        assert_eq!(action_seed(5, 1), action_seed(5, 1));
        assert_ne!(action_seed(5, 1), action_seed(5, 2));
        assert_ne!(action_seed(5, 1), action_seed(6, 1));
        assert_ne!(action_seed(-1i64 as u64, 1), action_seed(1, 1));
        assert_ne!(bot_seed(5, Power::Ussr), bot_seed(5, Power::Germany));
        assert_ne!(action_seed(5, 1), bot_seed(5, Power::Germany));
        assert_ne!(fallback_seed(5, 1, 0), fallback_seed(5, 1, 1));
    }

    #[test]
    fn seed_value_is_pinned() {
        // computed with an independent implementation; guards against accidental changes
        assert_eq!(action_seed(42, 7), 4_232_985_104_476_163_571);
    }

    #[test]
    fn action_seed_is_byte_exact() {
        // Pinned from the original server implementation before it moved here: changing the
        // hash would silently break replays of stored games.
        assert_eq!(action_seed(5, 1), 17_431_658_355_517_745_560);
    }
}
