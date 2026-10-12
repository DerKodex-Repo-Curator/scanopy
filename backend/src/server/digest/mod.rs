//! Per-discovery-session digest. Computes the four-set entity diff over the
//! session window using the SCD2 timestamp columns shipped by the Phase 2
//! foundation, then publishes `DiscoveryDigestOperation::Computed` for the
//! email subscriber to render and dispatch.

pub mod payload;
pub mod run_report;
pub mod service;
