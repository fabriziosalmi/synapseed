//! # synapseed-telemetry-sink
//!
//! OTLP gRPC Receiver — ingests OpenTelemetry traces from running
//! applications and maps spans to source code symbols via Cortex.
//!
//! Listens on port 4317 (standard OTLP gRPC), resolves `code.file.path`
//! and `code.line.number` span attributes to SymbolIds, stores in a
//! ring buffer, and broadcasts `TelemetryUpdate` events for the
//! visualizer heatmap.

pub mod plugin;
pub mod server;
pub mod store;
