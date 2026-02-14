# Security Scanning

## Scan Selection for Secrets
Select code containing config, environment variables, or credentials. Then:
- Press `Cmd+Shift+D` / `Ctrl+Shift+D`
- Or right-click → **Scan Selection for Secrets**

SYNAPSEED DLP engine detects API keys, passwords, tokens, and code anti-patterns (SQL injection, XSS, command injection, path traversal).

## Check Command Safety
Run **SYNAPSEED: Check Shell Command Safety** from the Command Palette to validate a shell command against the project security policy before executing it.
