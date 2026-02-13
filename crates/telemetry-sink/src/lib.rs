#![forbid(unsafe_code)]
//! # synapseed-telemetry-sink
//!
//! OTLP gRPC Receiver — ingests OpenTelemetry traces from running
//! applications and maps spans to source code symbols via Cortex.
//!
//! Listens on port 4317 (standard OTLP gRPC), resolves `code.file.path`
//! and `code.line.number` span attributes to SymbolIds, stores in a
//! ring buffer, and broadcasts `TelemetryUpdate` events for the
//! visualizer heatmap.
//!
//! ## Feature flags
//!
//! - **`grpc`** — Enables the tonic/OTLP gRPC server (`server` module)
//!   and pulls in `tonic`, `opentelemetry-proto`, and `prost`.
//!   Without this feature the crate still exports [`store::SpanStore`]
//!   and a no-op plugin that registers an empty store.

pub mod plugin;
#[cfg(feature = "grpc")]
pub mod server;
pub mod store;
