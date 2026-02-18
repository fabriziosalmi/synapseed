use std::path::{Path, PathBuf};

use thiserror::Error;

/// Domain errors shared across all SYNAPSEED crates.
#[derive(Debug, Error)]
pub enum SynapseedError {
    #[error("Parse error in '{file}': {reason}")]
    Parse { file: String, reason: String },

    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),

    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[error("Path traversal blocked: '{path}' escapes project root")]
    PathTraversal { path: String },

    #[error("Policy denied command: {command}")]
    PolicyDenied { command: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, SynapseedError>;

/// Resolve a potentially relative `user_path` against `root`, then validate
/// that the canonical result still lives inside `root`.
///
/// Returns `Ok(canonical_path)` on success, or `Err(PathTraversal)` when the
/// resolved path escapes the project root (e.g. `../../etc/passwd`).
///
/// Both `root` and the resolved path are canonicalized so that symlinks and
/// `..` segments are fully resolved before the containment check.
pub fn safe_resolve_path(root: &Path, user_path: &str) -> Result<PathBuf> {
    let abs_path = if Path::new(user_path).is_absolute() {
        PathBuf::from(user_path)
    } else {
        root.join(user_path)
    };

    let root_canonical = root
        .canonicalize()
        .map_err(|_| SynapseedError::PathTraversal {
            path: user_path.to_string(),
        })?;

    let canonical = abs_path
        .canonicalize()
        .map_err(|_| SynapseedError::PathTraversal {
            path: user_path.to_string(),
        })?;

    if !canonical.starts_with(&root_canonical) {
        return Err(SynapseedError::PathTraversal {
            path: user_path.to_string(),
        });
    }

    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A valid relative path inside the root should resolve successfully.
    #[test]
    fn safe_resolve_allows_valid_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("hello.txt"), "content").unwrap();

        let result = safe_resolve_path(root, "hello.txt");
        assert!(result.is_ok(), "expected Ok, got {result:?}");
        assert!(result.unwrap().starts_with(root.canonicalize().unwrap()));
    }

    /// A valid absolute path inside the root should resolve successfully.
    #[test]
    fn safe_resolve_allows_valid_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("data.txt");
        fs::write(&file, "content").unwrap();

        let abs_str = file.to_str().unwrap();
        let result = safe_resolve_path(root, abs_str);
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    /// Subdirectory paths should be accepted.
    #[test]
    fn safe_resolve_allows_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("src/lib")).unwrap();
        fs::write(root.join("src/lib/main.rs"), "fn main() {}").unwrap();

        let result = safe_resolve_path(root, "src/lib/main.rs");
        assert!(result.is_ok());
    }

    /// A path with `..` that escapes the root must be rejected.
    #[test]
    fn safe_resolve_blocks_dot_dot_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create a file outside root for the traversal to target
        let result = safe_resolve_path(root, "../../etc/passwd");
        assert!(
            result.is_err(),
            "expected Err(PathTraversal), got {result:?}"
        );

        if let Err(SynapseedError::PathTraversal { path }) = result {
            assert_eq!(path, "../../etc/passwd");
        } else {
            panic!("wrong error variant");
        }
    }

    /// A `..` that stays within root should still be allowed.
    #[test]
    fn safe_resolve_allows_dot_dot_within_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/top.txt"), "ok").unwrap();

        // "a/b/../top.txt" resolves to "a/top.txt" which is inside root
        let result = safe_resolve_path(root, "a/b/../top.txt");
        assert!(result.is_ok());
    }

    /// An absolute path pointing outside the root must be rejected.
    #[test]
    fn safe_resolve_blocks_absolute_outside_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let result = safe_resolve_path(root, "/etc/hostname");
        // This may be Ok or Err depending on whether /etc/hostname exists,
        // but if it resolves, it must NOT start_with root.
        match result {
            Ok(p) => panic!("should have been blocked, got {p:?}"),
            Err(SynapseedError::PathTraversal { .. }) => { /* correct */ }
            Err(other) => panic!("unexpected error: {other}"),
        }
    }

    /// A nonexistent file should produce a PathTraversal error (canonicalize fails).
    #[test]
    fn safe_resolve_rejects_nonexistent_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let result = safe_resolve_path(root, "nonexistent/file.txt");
        assert!(result.is_err());
    }

    /// Verify the error message format.
    #[test]
    fn path_traversal_error_display() {
        let err = SynapseedError::PathTraversal {
            path: "../../etc/shadow".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Path traversal blocked: '../../etc/shadow' escapes project root"
        );
    }
}
