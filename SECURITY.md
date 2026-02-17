# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 1.x     | :white_check_mark: |
| < 1.0   | :x:                |

Only the latest stable release receives security patches.

## Reporting a Vulnerability

**Do not open public GitHub issues for security vulnerabilities.**

If you discover a security issue (DLP bypass, command injection, RCE, path traversal, or any sensitive bug), please report it through one of the following channels:

1. **GitHub Private Vulnerability Reporting** — use the "Report a vulnerability" button on the [Security tab](https://github.com/fabriziosalmi/synapseed/security/advisories/new)
2. **Email** — send details to [fabrizio.salmi@gmail.com](mailto:fabrizio.salmi@gmail.com)

### What to include

- Description of the vulnerability
- Steps to reproduce
- Affected version(s)
- Potential impact assessment

### Response timeline

- **Acknowledgment**: within 48 hours
- **Initial assessment**: within 7 days
- **Fix or mitigation**: within 30 days for critical issues

### Scope

The following components are in scope for security reports:

| Component | Risk Area |
|-----------|-----------|
| **Husk** (DLP) | Pattern bypass, data exfiltration |
| **Root** (Sentinel) | Command injection, allow-list bypass |
| **MCP Server** | JSON-RPC injection, unauthorized tool access |
| **Cortex** (AST Parser) | Path traversal, symlink attacks |
| **Chronos** (Git) | Arbitrary file read via git operations |

### Out of scope

- Denial of service against the local CLI process
- Issues requiring physical access to the machine
- Vulnerabilities in third-party dependencies (report upstream, but let us know)

## Security Architecture

SYNAPSEED is designed with defense-in-depth:

- **DLP scanning** redacts sensitive data before it reaches the LLM
- **Sentinel** enforces a deny-first command execution policy
- **Network isolation** — the MCP server communicates only via stdio (no network listeners except the optional visualizer dashboard)
- **No secrets stored** — SYNAPSEED never persists API keys or credentials
