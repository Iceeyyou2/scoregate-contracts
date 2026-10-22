#![cfg_attr(target_family = "wasm", allow(dead_code))]

/// Schema version for all events emitted by the aggregator.
/// Bump when event topic structure changes in a breaking way.
pub const EVENT_VERSION: u32 = 1;