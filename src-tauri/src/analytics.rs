//! Compatibility shims for analytics calls made by older frontend code.
//!
//! This maintained fork does not collect or transmit telemetry. Keeping these
//! async functions avoids coupling unrelated call sites to that policy.

use serde_json::Value;

/// Discard an analytics event without performing network I/O.
pub async fn send_event(_name: &str, _props: Value, _app_version: &str) {}

/// Discard a page-view event without performing network I/O.
pub async fn send_pageview() {}
