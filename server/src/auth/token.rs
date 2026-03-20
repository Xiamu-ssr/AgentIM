use rand::Rng;
use sha2::{Digest, Sha256};

use crate::consts;

/// Generate a new agent token: `aim_` + 48 random hex characters.
pub fn generate_token() -> String {
    let mut rng = rand::rng();
    let random_bytes: Vec<u8> = (0..consts::TOKEN_RANDOM_BYTES)
        .map(|_| rng.random::<u8>())
        .collect();
    format!("{}{}", consts::TOKEN_PREFIX, hex::encode(random_bytes))
}

/// Compute the SHA-256 hash of a token, returned as lowercase hex string.
pub fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_has_correct_prefix() {
        let token = generate_token();
        assert!(token.starts_with(consts::TOKEN_PREFIX));
    }

    #[test]
    fn token_has_correct_length() {
        let token = generate_token();
        // "aim_" (4) + 48 bytes * 2 hex chars = 100
        assert_eq!(token.len(), 4 + consts::TOKEN_RANDOM_BYTES * 2);
    }

    #[test]
    fn hash_is_deterministic() {
        let token = "aim_abc123";
        assert_eq!(hash_token(token), hash_token(token));
    }

    #[test]
    fn hash_differs_from_raw() {
        let token = generate_token();
        assert_ne!(hash_token(&token), token);
    }

    #[test]
    fn different_tokens_have_different_hashes() {
        let t1 = generate_token();
        let t2 = generate_token();
        assert_ne!(hash_token(&t1), hash_token(&t2));
    }
}
