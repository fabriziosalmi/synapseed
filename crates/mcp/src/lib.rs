#![forbid(unsafe_code)]
//! # synapseed-mcp
//!
//! MCP (Model Context Protocol) server implementation for SYNAPSEED.
//! Exposes all SYNAPSEED capabilities (AST, DLP, Git, Sentinel) as
//! MCP tools, resources, and prompts over JSON-RPC 2.0 via stdio.

pub mod prompts;
pub mod protocol;
pub mod resources;
pub mod server;
pub mod tools;
