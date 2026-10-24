//! Small pure helpers: opaque tokens, hashing, invite codes, slugs, validation.
use base64::Engine;
use rand::Rng;
use sha2::{Digest, Sha256};

/// 256-bit random opaque token (URL-safe base64, no padding).
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Hash a token for storage: hex(sha256(secret ":" token)). Only the hash is stored in the DB.
pub fn hash_token(secret: &str, token: &str) -> String {
    let mut h = Sha256::new();
    h.update(secret.as_bytes());
    h.update(b":");
    h.update(token.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Alphabet without ambiguous characters (no 0/O/1/I/L).
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

/// Human-friendly invite code like `K7QD-X2MP`.
pub fn new_invite_code() -> String {
    let mut rng = rand::rng();
    let mut s = String::with_capacity(9);
    for i in 0..8 {
        if i == 4 {
            s.push('-');
        }
        s.push(CODE_ALPHABET[rng.random_range(0..CODE_ALPHABET.len())] as char);
    }
    s
}

pub fn normalize_invite_code(code: &str) -> String {
    code.trim().to_uppercase()
}

pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("group");
    }
    out.truncate(48);
    out
}

pub fn valid_username(u: &str) -> bool {
    (3..=24).contains(&u.len())
        && u.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub fn valid_email(e: &str) -> bool {
    let e = e.trim();
    match e.split_once('@') {
        Some((l, d)) => {
            !l.is_empty()
                && d.contains('.')
                && !d.starts_with('.')
                && !d.ends_with('.')
                && !e.contains(char::is_whitespace)
                && e.len() <= 254
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hash_is_stable_and_salted() {
        let a = hash_token("s1", "tok");
        assert_eq!(a, hash_token("s1", "tok"));
        assert_ne!(a, hash_token("s2", "tok"));
        assert_ne!(a, hash_token("s1", "tok2"));
        assert_eq!(a.len(), 64);
        assert_ne!(a, "tok");
    }

    #[test]
    fn tokens_are_unique_and_long() {
        let a = new_token();
        assert!(a.len() >= 43);
        assert_ne!(a, new_token());
    }

    #[test]
    fn invite_codes_look_right() {
        for _ in 0..50 {
            let c = new_invite_code();
            assert_eq!(c.len(), 9);
            assert_eq!(&c[4..5], "-");
            assert!(c
                .chars()
                .all(|ch| ch == '-' || CODE_ALPHABET.contains(&(ch as u8))));
        }
        assert_eq!(normalize_invite_code(" ab12-cd34 "), "AB12-CD34");
    }

    #[test]
    fn slugs() {
        assert_eq!(slugify("  Eastern Front!! Club "), "eastern-front-club");
        assert_eq!(slugify("***"), "group");
    }

    #[test]
    fn validation() {
        assert!(valid_username("good_name-1"));
        assert!(!valid_username("no spaces"));
        assert!(!valid_username("ab"));
        assert!(valid_email("a@b.co"));
        assert!(!valid_email("a@b"));
        assert!(!valid_email("nope"));
    }
}
