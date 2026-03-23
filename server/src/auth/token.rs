use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::Utc;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::consts;

// ── Claim code ──

/// Generate a one-time claim code: `clm_` + random hex.
pub fn generate_claim_code() -> String {
    let mut rng = rand::rng();
    let random_bytes: Vec<u8> = (0..consts::CLAIM_CODE_RANDOM_BYTES)
        .map(|_| rng.random::<u8>())
        .collect();
    format!("{}{}", consts::CLAIM_CODE_PREFIX, hex::encode(random_bytes))
}

/// Compute the SHA-256 hash of a claim code, returned as lowercase hex.
pub fn hash_claim_code(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

// ── Challenge nonce ──

/// Generate a random challenge nonce (32 bytes, base64 encoded).
pub fn generate_challenge_nonce() -> String {
    let mut rng = rand::rng();
    let random_bytes: Vec<u8> = (0..32).map(|_| rng.random::<u8>()).collect();
    BASE64.encode(random_bytes)
}

// ── Ed25519 public key fingerprint ──

/// Compute the fingerprint of an Ed25519 public key (SHA-256 first 16 hex chars).
pub fn public_key_fingerprint(public_key_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(public_key_bytes);
    let hash = hex::encode(hasher.finalize());
    hash[..16].to_string()
}

// ── JWT ──

/// JWT claims payload.
#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    /// Subject: agent_id
    pub sub: String,
    /// Credential ID used for authentication
    pub cid: String,
    /// Issued at (unix timestamp)
    pub iat: i64,
    /// Expires at (unix timestamp)
    pub exp: i64,
}

/// Create a JWT access token for an authenticated agent.
pub fn create_jwt(agent_id: &str, credential_id: &str, secret: &str) -> Result<String, String> {
    let now = Utc::now().timestamp();
    let claims = JwtClaims {
        sub: agent_id.to_string(),
        cid: credential_id.to_string(),
        iat: now,
        exp: now + consts::ACCESS_TOKEN_TTL_SECS as i64,
    };

    jsonwebtoken::encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| format!("JWT encoding error: {}", e))
}

/// Verify a JWT access token and return the claims.
pub fn verify_jwt(token: &str, secret: &str) -> Result<JwtClaims, String> {
    let mut validation = Validation::default();
    validation.set_required_spec_claims(&["sub", "exp"]);

    jsonwebtoken::decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|e| format!("JWT verification error: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_code_has_correct_prefix() {
        let code = generate_claim_code();
        assert!(code.starts_with(consts::CLAIM_CODE_PREFIX));
    }

    #[test]
    fn claim_code_has_correct_length() {
        let code = generate_claim_code();
        // "clm_" (4) + 32 bytes * 2 hex chars = 68
        assert_eq!(code.len(), 4 + consts::CLAIM_CODE_RANDOM_BYTES * 2);
    }

    #[test]
    fn claim_code_hash_is_deterministic() {
        let code = "clm_abc123";
        assert_eq!(hash_claim_code(code), hash_claim_code(code));
    }

    #[test]
    fn claim_code_hash_differs_from_raw() {
        let code = generate_claim_code();
        assert_ne!(hash_claim_code(&code), code);
    }

    #[test]
    fn different_claim_codes_have_different_hashes() {
        let c1 = generate_claim_code();
        let c2 = generate_claim_code();
        assert_ne!(hash_claim_code(&c1), hash_claim_code(&c2));
    }

    #[test]
    fn challenge_nonce_is_non_empty() {
        let nonce = generate_challenge_nonce();
        assert!(!nonce.is_empty());
        // Should be valid base64
        assert!(BASE64.decode(&nonce).is_ok());
    }

    #[test]
    fn challenge_nonces_are_unique() {
        let n1 = generate_challenge_nonce();
        let n2 = generate_challenge_nonce();
        assert_ne!(n1, n2);
    }

    #[test]
    fn public_key_fingerprint_is_16_hex() {
        let fp = public_key_fingerprint(b"test-public-key");
        assert_eq!(fp.len(), 16);
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn jwt_create_and_verify() {
        let secret = "test-secret-key-for-jwt";
        let token = create_jwt("my-agent", "cred-1", secret).unwrap();
        assert!(!token.is_empty());

        let claims = verify_jwt(&token, secret).unwrap();
        assert_eq!(claims.sub, "my-agent");
        assert_eq!(claims.cid, "cred-1");
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn jwt_wrong_secret_fails() {
        let token = create_jwt("my-agent", "cred-1", "secret-a").unwrap();
        let result = verify_jwt(&token, "secret-b");
        assert!(result.is_err());
    }

    #[test]
    fn jwt_expired_token_fails() {
        // Create a token that's already expired by manipulating claims directly
        let claims = JwtClaims {
            sub: "my-agent".into(),
            cid: "cred-1".into(),
            iat: 1000,
            exp: 1001, // long past
        };
        let secret = "test-secret";
        let token = jsonwebtoken::encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap();

        let result = verify_jwt(&token, secret);
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("expired") || err_msg.contains("Expired"),
            "Expected 'expired' in error but got: {}",
            err_msg
        );
    }
}
