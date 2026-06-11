//! Password hashing (argon2) and opaque session tokens (random + sha256).

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use sha2::{Digest, Sha256};

/// Entropy of a session token before hex-encoding (256 bits).
const TOKEN_BYTES: usize = 32;

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Hash a password for storage (argon2id with a random salt). Infallible in
/// practice (default params + generated salt); a failure is a programming bug.
pub fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("argon2 hashing")
        .to_string()
}

/// Verify a password against a stored hash. `false` on any mismatch or bad hash.
pub fn verify_password(stored_hash: &str, password: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// SHA-256 hex of a token — what we store/look up (never the raw token).
pub fn hash_token(token: &str) -> String {
    to_hex(&Sha256::digest(token.as_bytes()))
}

/// Generate a fresh session token and its stored hash: `(token, token_hash)`.
pub fn new_token() -> (String, String) {
    let raw: [u8; TOKEN_BYTES] = rand::random();
    let token = to_hex(&raw);
    let token_hash = hash_token(&token);
    (token, token_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_verifies_passwords() {
        let hash = hash_password("s3cret");
        assert!(verify_password(&hash, "s3cret"));
        assert!(!verify_password(&hash, "wrong"));
        assert!(!verify_password("not-a-valid-hash", "s3cret")); // bad hash -> false
    }

    #[test]
    fn tokens_are_unique_and_hash_consistently() {
        let (token, token_hash) = new_token();
        assert_eq!(hash_token(&token), token_hash);
        assert_ne!(token, token_hash); // stored hash != raw token
        let (other, _) = new_token();
        assert_ne!(token, other);
    }
}
