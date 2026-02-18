# Introduction

## What is SYNAPSEED? (Simple Version)

SYNAPSEED helps AI assistants understand your code better. Instead of just reading text files, it gives AI tools like Claude:

- 📍 **Exact locations** of functions, classes, and variables
- 🔍 **Smart search** that understands concepts, not just keywords
- 🛡️ **Security protection** to prevent leaking passwords or API keys
- 📊 **Code history** showing who changed what and why
- 🏗️ **Quality metrics** with letter grades (A-F) for code health

**Real-world impact:** With SYNAPSEED, AI assistants make 50% fewer mistakes when understanding code (measured in our benchmark suite).

## For Technical Users

**SYNAPSEED** is a local, high-performance middleware designed to act as the **Thinking Layer** between you (the Developer) and your LLM (Claude, Copilot, etc.).

Most AI coding agents treat your codebase as a pile of text files, using `grep` and `cat` to understand logic. This leads to hallucinations, broken imports, and security leaks.

**SYNAPSEED is different.** It parses your code into an **AST (Abstract Syntax Tree)**, indexes it semantically, scans for secrets in real-time, and visualizes the architecture live — all in a single **Rust binary**.

## The Problem

| Standard LLM Context | SYNAPSEED Capability |
| :--- | :--- |
| `cat file.rs` (Blind text) | **AST Skeleton** — Semantic symbols & relationships |
| Regex / Grep | **Tantivy Semantic Search** — Concepts over keywords |
| None (Leaked secrets) | **DLP Fail-Closed** — Real-time redaction |
| Zero context (Stateless) | **Git Time-Travel** — History & intent analysis |
| Suggests `rm -rf /` | **Sentinel Sandbox** — Policy enforcement |
| No visibility | **Architecture Health** — Dependency graph & metrics |
| High latency (Network) | **Zero-Copy / Direct Rust** — <10ms response |

## How It Works

SYNAPSEED connects to your LLM via the **Model Context Protocol (MCP)** — an open standard for tool-augmented AI. When your LLM needs to understand code, check security, or explore history, it calls SYNAPSEED tools instead of blindly reading files.

```
Developer ←→ LLM (Claude/Copilot) ←→ MCP Protocol ←→ SYNAPSEED
                                                         ├── Cortex (AST Engine)
                                                         ├── Husk (DLP Shield + Code Patterns)
                                                         ├── Root (Command Sentinel)
                                                         ├── Chronos (Git Intelligence)
                                                         ├── Search (Tantivy Index)
                                                         ├── Shadow (Live Compiler)
                                                         ├── Architect (Structural Health)
                                                         ├── Gym (Code Sandbox + Adversarial)
                                                         ├── Janitor (Automated Maintenance)
                                                         ├── Bench (Evaluation Suite)
                                                         ├── Decompiler (Binary Analysis)
                                                         ├── Whisper (Intent Router)
                                                         └── Telemetry (OTLP Receiver)
```

## Key Design Principles

- **Local-first** — No cloud, no network, no telemetry leaks. Runs entirely on your machine.
- **Fail-closed security** — If a secret is found, the operation is blocked. No exceptions.
- **Plugin architecture** — Each subsystem is an independent crate with a clean interface.
- **Event-driven** — Plugins communicate via an async broadcast channel, enabling reactive pipelines.
- **Zero-copy where possible** — Direct memory access, no serialization overhead between crates.

