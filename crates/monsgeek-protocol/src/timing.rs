//! HID communication timing constants.
//!
//! The yc3121 firmware requires a 100ms delay between HID commands or it
//! crashes. These constants encode that requirement and related timing values
//! from the reference implementation.

/// Number of retries for query operations.
pub const QUERY_RETRIES: usize = 5;
/// Number of retries for send operations.
pub const SEND_RETRIES: usize = 3;
/// Default delay after HID command (ms) -- for wired devices.
pub const DEFAULT_DELAY_MS: u64 = 100;
/// Short delay for fast operations (ms).
pub const SHORT_DELAY_MS: u64 = 50;
/// Minimum delay for streaming (ms).
pub const MIN_DELAY_MS: u64 = 5;
/// Delay after starting animation upload (ms).
pub const ANIMATION_START_DELAY_MS: u64 = 500;

/// Dongle-specific timing for polling-based flow control.
///
/// Based on throughput testing:
/// - Minimum observed latency: ~8-10ms (awake keyboard)
/// - Response requires flush command to push into buffer
/// - Concurrent commands not supported by hardware
pub mod dongle {
    /// Initial wait before first poll attempt (ms).
    /// Adaptive baseline -- actual wait is computed from moving average.
    pub const INITIAL_WAIT_MS: u64 = 5;
    /// Default timeout for query operations (ms).
    pub const QUERY_TIMEOUT_MS: u64 = 500;
    /// Extended timeout when keyboard may be waking from sleep (ms).
    pub const WAKE_TIMEOUT_MS: u64 = 2000;
    /// Minimum time per poll cycle -- flush + read (ms).
    pub const POLL_CYCLE_MS: u64 = 1;
    /// Moving average window size for latency tracking.
    pub const LATENCY_WINDOW_SIZE: usize = 8;
    /// Maximum consecutive timeouts before marking device offline.
    pub const MAX_CONSECUTIVE_TIMEOUTS: usize = 3;
    /// Queue capacity for pending command requests.
    pub const REQUEST_QUEUE_SIZE: usize = 16;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_delay_ms() {
        assert_eq!(DEFAULT_DELAY_MS, 100);
    }

    #[test]
    fn test_query_retries() {
        assert_eq!(QUERY_RETRIES, 5);
    }

    #[test]
    fn test_send_retries() {
        assert_eq!(SEND_RETRIES, 3);
    }
}
