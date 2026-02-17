# DLP Reference

Detailed reference for SYNAPSEED's Data Loss Prevention (DLP) system.

## Detection Patterns

### AWS Access Keys

| Pattern | Method |
| :--- | :--- |
| `AKIA` prefix + 16 alphanumeric chars | Aho-Corasick prefix + length validation |

Example: `AKIAIOSFODNN7EXAMPLE`

### GitHub Tokens

| Pattern | Method |
| :--- | :--- |
| `ghp_` prefix | Aho-Corasick |
| `gho_` prefix | Aho-Corasick |
| `ghs_` prefix | Aho-Corasick |
| `ghr_` prefix | Aho-Corasick |
| `github_pat_` prefix | Aho-Corasick |

### Generic Secrets

| Pattern | Method |
| :--- | :--- |
| `password=<value>` | Regex |
| `api_key=<value>` | Regex |
| `secret=<value>` | Regex |
| `token=<value>` | Regex |
| `auth=<value>` | Regex |

### Private Keys

| Pattern | Method |
| :--- | :--- |
| `-----BEGIN RSA PRIVATE KEY-----` | Aho-Corasick |
| `-----BEGIN EC PRIVATE KEY-----` | Aho-Corasick |
| `-----BEGIN OPENSSH PRIVATE KEY-----` | Aho-Corasick |
| `-----BEGIN PRIVATE KEY-----` | Aho-Corasick |

## Whitelist (False-Positive Suppression)

Findings whose matched text matches a whitelist pattern are silently dropped. Built-in defaults suppress common Rust false positives:

| Pattern | Suppresses |
| :--- | :--- |
| `(?i)token\s*[:=]\s*[A-Z]\w+` | Type assignments like `token: CancellationToken` |
| `(?i)shutdown_token` | Async shutdown patterns |

Add custom whitelist patterns in `.synapseed/dna.yaml`:

```yaml
dlp_whitelist:
  - "(?i)token\\s*[:=]\\s*[A-Z]\\w+"
  - "(?i)shutdown_token"
```

## Redaction

When sensitive content is found, SYNAPSEED replaces the sensitive portion with `REDACTED`:

```
Input:  aws_key=AKIAIOSFODNN7EXAMPLE
Output: aws_key=REDACTED
```

The original content is never included in tool responses.

## API

### SecurityGuard

```rust
let guard = SecurityGuard::with_defaults();

// Check for secrets (returns Ok or Err with finding)
match guard.check(&content) {
    Ok(()) => println!("CLEAN"),
    Err(finding) => println!("ALERT: {finding}"),
}

// Redact secrets (returns sanitized content)
let safe = guard.redact(&content);
```

## Performance

The Aho-Corasick engine scans content in **O(n)** time regardless of the number of patterns. A typical DLP scan takes less than 1ms for content up to 100KB.
