//! The Neural Decompiler — Binary analysis for code intelligence.
//!
//! Analyzes compiled binaries and external libraries to extract behavioral
//! understanding: function signatures, string references, call graphs,
//! and inferred purpose.
//!
//! Supports ELF (Linux), Mach-O (macOS), and PE (Windows) formats
//! via the `goblin` crate.

pub mod binary;
pub mod callgraph;
pub mod strings;

use anyhow::Result;
use serde::Serialize;

pub use binary::{BinaryInfo, ExportedSymbol};
pub use callgraph::CallGraph;
pub use strings::{ClassifiedString, StringClass};

/// Complete analysis result for a binary.
#[derive(Debug, Serialize)]
pub struct BinaryAnalysis {
    /// Basic binary metadata.
    pub info: BinaryInfo,
    /// Exported/imported symbols.
    pub symbols: Vec<ExportedSymbol>,
    /// Extracted and classified strings.
    pub strings: Vec<ClassifiedString>,
    /// Inter-function call graph.
    pub call_graph: CallGraph,
    /// Inferred behavioral categories.
    pub behaviors: Vec<BehaviorTag>,
    /// Summary: one-line description of likely purpose.
    pub summary: String,
}

/// High-level behavioral tag inferred from heuristics.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum BehaviorTag {
    /// Network I/O (TCP, HTTP, DNS, TLS).
    NetworkIO,
    /// File system operations (open, read, write, mmap).
    FileIO,
    /// Cryptographic operations (AES, SHA, RSA, TLS).
    Crypto,
    /// Serialization/deserialization (JSON, protobuf, msgpack).
    Serialization,
    /// Memory allocation (malloc, mmap, arena).
    MemoryManagement,
    /// Threading/concurrency (pthread, futex, mutex).
    Concurrency,
    /// Process management (fork, exec, spawn).
    ProcessManagement,
    /// Logging/tracing (syslog, tracing, log).
    Logging,
    /// Database operations (SQL, key-value).
    Database,
    /// Compression (zlib, zstd, lz4).
    Compression,
}

/// Analyze a binary file. This is the main entry point.
pub fn analyze_binary(path: &std::path::Path) -> Result<BinaryAnalysis> {
    let data = std::fs::read(path)?;
    let info = binary::parse_binary(&data, path)?;
    let symbols = binary::extract_symbols(&data)?;
    let raw_strings = strings::extract_strings(&data, 6);
    let classified = strings::classify_strings(&raw_strings);
    let call_graph = callgraph::build_call_graph(&symbols);
    let behaviors = infer_behaviors(&symbols, &classified);
    let summary = build_summary(&info, &symbols, &behaviors);

    Ok(BinaryAnalysis {
        info,
        symbols,
        strings: classified,
        call_graph,
        behaviors,
        summary,
    })
}

/// Infer high-level behaviors from symbol names and strings.
fn infer_behaviors(symbols: &[ExportedSymbol], strings: &[ClassifiedString]) -> Vec<BehaviorTag> {
    let mut tags = Vec::new();
    let all_names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    let all_strings: Vec<&str> = strings.iter().map(|s| s.value.as_str()).collect();

    // Network I/O
    let net_patterns = [
        "socket", "connect", "bind", "listen", "accept", "send", "recv",
        "tcp", "udp", "http", "https", "dns", "tls", "ssl", "curl",
        "reqwest", "hyper", "tokio::net", "getaddrinfo",
    ];
    if has_pattern(&all_names, &all_strings, &net_patterns) {
        tags.push(BehaviorTag::NetworkIO);
    }

    // File I/O
    let file_patterns = [
        "open", "read_to_string", "write_all", "fopen", "fclose", "fread",
        "fwrite", "mmap", "std::fs", "create_dir", "remove_file",
    ];
    if has_pattern(&all_names, &all_strings, &file_patterns) {
        tags.push(BehaviorTag::FileIO);
    }

    // Crypto
    let crypto_patterns = [
        "aes", "sha256", "sha512", "rsa", "ed25519", "hmac", "encrypt",
        "decrypt", "digest", "sign", "verify", "ring::", "openssl",
        "rustls", "chacha", "argon2", "bcrypt", "pbkdf",
    ];
    if has_pattern(&all_names, &all_strings, &crypto_patterns) {
        tags.push(BehaviorTag::Crypto);
    }

    // Serialization
    let ser_patterns = [
        "serde", "json", "yaml", "toml", "protobuf", "msgpack",
        "serialize", "deserialize", "from_str", "to_string",
        "bincode", "cbor",
    ];
    if has_pattern(&all_names, &all_strings, &ser_patterns) {
        tags.push(BehaviorTag::Serialization);
    }

    // Memory management
    let mem_patterns = [
        "malloc", "free", "realloc", "mmap", "munmap", "alloc::alloc",
        "arena", "bump_alloc", "jemalloc",
    ];
    if has_pattern(&all_names, &all_strings, &mem_patterns) {
        tags.push(BehaviorTag::MemoryManagement);
    }

    // Concurrency
    let conc_patterns = [
        "pthread", "mutex", "rwlock", "condvar", "futex", "atomic",
        "rayon", "tokio::spawn", "thread::spawn", "crossbeam",
        "arc", "channel", "mpsc",
    ];
    if has_pattern(&all_names, &all_strings, &conc_patterns) {
        tags.push(BehaviorTag::Concurrency);
    }

    // Process management
    let proc_patterns = [
        "fork", "exec", "spawn", "waitpid", "kill", "signal",
        "std::process", "Command::new",
    ];
    if has_pattern(&all_names, &all_strings, &proc_patterns) {
        tags.push(BehaviorTag::ProcessManagement);
    }

    // Logging
    let log_patterns = [
        "tracing", "log::", "syslog", "env_logger", "debug!", "info!",
        "warn!", "error!", "trace!", "spdlog",
    ];
    if has_pattern(&all_names, &all_strings, &log_patterns) {
        tags.push(BehaviorTag::Logging);
    }

    // Database
    let db_patterns = [
        "sqlite", "postgres", "mysql", "diesel", "sqlx", "rusqlite",
        "SELECT", "INSERT", "CREATE TABLE", "redis", "rocksdb", "sled",
    ];
    if has_pattern(&all_names, &all_strings, &db_patterns) {
        tags.push(BehaviorTag::Database);
    }

    // Compression
    let comp_patterns = [
        "zlib", "zstd", "lz4", "gzip", "deflate", "inflate",
        "compress", "decompress", "flate2", "brotli", "snappy",
    ];
    if has_pattern(&all_names, &all_strings, &comp_patterns) {
        tags.push(BehaviorTag::Compression);
    }

    tags
}

/// Check if any pattern appears (case-insensitive) in names or strings.
fn has_pattern(names: &[&str], strings: &[&str], patterns: &[&str]) -> bool {
    for pat in patterns {
        let pat_lower = pat.to_lowercase();
        for name in names {
            if name.to_lowercase().contains(&pat_lower) {
                return true;
            }
        }
        for s in strings {
            if s.to_lowercase().contains(&pat_lower) {
                return true;
            }
        }
    }
    false
}

/// Build a one-line summary.
fn build_summary(
    info: &BinaryInfo,
    symbols: &[ExportedSymbol],
    behaviors: &[BehaviorTag],
) -> String {
    let behavior_desc: Vec<&str> = behaviors
        .iter()
        .map(|b| match b {
            BehaviorTag::NetworkIO => "network I/O",
            BehaviorTag::FileIO => "file I/O",
            BehaviorTag::Crypto => "cryptography",
            BehaviorTag::Serialization => "serialization",
            BehaviorTag::MemoryManagement => "memory management",
            BehaviorTag::Concurrency => "concurrency",
            BehaviorTag::ProcessManagement => "process management",
            BehaviorTag::Logging => "logging",
            BehaviorTag::Database => "database",
            BehaviorTag::Compression => "compression",
        })
        .collect();

    let behaviors_str = if behavior_desc.is_empty() {
        "general-purpose".to_string()
    } else {
        behavior_desc.join(", ")
    };

    format!(
        "{} binary ({}, {} symbols) — involves: {}",
        info.format, info.arch, symbols.len(), behaviors_str
    )
}
