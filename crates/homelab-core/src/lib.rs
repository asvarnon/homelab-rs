//! # Homelab Core
//!
//! Pure capabilities and data models for homelab automation.

/// Data models for the homelab.
pub mod models;

/// Driver traits for external systems.
pub mod traits;

/// Capability modules (e.g. proxmox, opnsense).
pub mod capabilities;

pub mod analysis;
/// Orchestration and analysis.
pub mod orchestrator;

/// Error definitions.
pub mod error;
