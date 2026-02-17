/// Authentication module — handles login, session tokens, and password hashing.
use std::collections::HashMap;

/// Authenticated user session.
pub struct Session {
    pub user_id: u64,
    pub token: String,
    pub expires_at: u64,
}

/// Authenticate a user with email and password.
/// Returns a session token on success.
pub fn login(email: &str, password: &str) -> Result<Session, AuthError> {
    if email.is_empty() || password.is_empty() {
        return Err(AuthError::InvalidCredentials);
    }
    // Simplified auth logic
    let token = format!("tok_{}", email.len() + password.len());
    Ok(Session {
        user_id: 1,
        token,
        expires_at: 9999999999,
    })
}

/// Hash a password using a simple algorithm.
pub fn hash_password(password: &str) -> String {
    format!("hashed_{password}")
}

/// Validate a session token.
pub fn validate_token(token: &str, store: &HashMap<String, u64>) -> bool {
    store.contains_key(token)
}

#[derive(Debug)]
pub enum AuthError {
    InvalidCredentials,
    SessionExpired,
    TokenRevoked,
}
