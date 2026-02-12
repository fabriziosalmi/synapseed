//! # synapseed-visualizer
//!
//! Live architecture visualization dashboard. Spawns an embedded HTTP
//! server (axum) that serves a Cytoscape.js-powered graph of the project's
//! code structure, updated in real time via WebSocket.

pub mod plugin;
pub mod server;
