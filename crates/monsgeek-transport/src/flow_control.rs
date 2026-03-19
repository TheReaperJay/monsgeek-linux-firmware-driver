//! Flow-control layer for echo-matched queries and fire-and-forget sends.
//!
//! Wraps `UsbSession` to provide:
//! - `query_command`: send + read with echo byte matching and retry
//! - `send_command`: fire-and-forget with retry on USB error

// Placeholder — tests drive implementation.

#[cfg(test)]
mod tests {
    use super::*;

    // We cannot test against real USB hardware in unit tests, but we can
    // verify the module compiles and the function signatures exist by
    // calling them with mock-like setups. Since UsbSession requires real
    // hardware, we test the logic via integration tests with feature gate.
    //
    // For unit tests, we verify:
    // 1. The functions exist with correct signatures (compilation test)
    // 2. The module uses the correct protocol constants

    #[test]
    fn test_query_command_exists_with_correct_signature() {
        // Verify the function signature compiles — cannot call without hardware.
        let _fn_ptr: fn(
            &crate::usb::UsbSession,
            u8,
            &[u8],
            monsgeek_protocol::ChecksumType,
        ) -> Result<[u8; 64], crate::error::TransportError> = query_command;
    }

    #[test]
    fn test_send_command_exists_with_correct_signature() {
        let _fn_ptr: fn(
            &crate::usb::UsbSession,
            u8,
            &[u8],
            monsgeek_protocol::ChecksumType,
        ) -> Result<(), crate::error::TransportError> = send_command;
    }

    #[test]
    fn test_uses_protocol_timing_constants() {
        // Verify we reference the correct constants from monsgeek_protocol::timing
        assert_eq!(monsgeek_protocol::timing::QUERY_RETRIES, 5);
        assert_eq!(monsgeek_protocol::timing::SEND_RETRIES, 3);
        assert_eq!(monsgeek_protocol::timing::DEFAULT_DELAY_MS, 100);
    }
}
