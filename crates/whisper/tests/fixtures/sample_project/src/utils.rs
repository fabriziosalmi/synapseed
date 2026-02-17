/// Utility functions — string helpers, validation, formatting.

/// Validate an email address (simple check).
pub fn is_valid_email(email: &str) -> bool {
    email.contains('@') && email.contains('.')
}

/// Truncate a string to max length, appending "..." if truncated.
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

/// Generate a unique request ID.
pub fn request_id() -> String {
    format!("req_{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_email() {
        assert!(is_valid_email("user@example.com"));
        assert!(!is_valid_email("invalid"));
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hello...");
    }
}
