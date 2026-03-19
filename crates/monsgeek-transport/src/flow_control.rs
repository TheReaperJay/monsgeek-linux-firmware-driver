//! Flow-control layer for echo-matched queries and fire-and-forget sends.
//!
//! Wraps `UsbSession` to provide:
//! - `query_command`: send + read with echo byte matching and retry (up to 5 attempts)
//! - `send_command`: fire-and-forget with retry on USB error (up to 3 attempts)
//!
//! Both functions build the HID frame via `monsgeek_protocol::build_command` and
//! strip the report ID byte before passing to `UsbSession::vendor_set_report`.
//! The 100ms inter-command delay is enforced at the transport thread level
//! (see `thread.rs`), not here — but `query_command` does sleep after SET_REPORT
//! to give the firmware time to prepare the response before GET_REPORT.

use std::time::Duration;

use monsgeek_protocol::timing;
use monsgeek_protocol::{build_command, ChecksumType};

use crate::error::TransportError;
use crate::usb::UsbSession;

/// Send a command and read the response, retrying until the echo byte matches.
///
/// Builds a 65-byte frame via `build_command`, sends `&frame[1..]` (64 bytes,
/// skipping the report ID) via `vendor_set_report`, sleeps 100ms for firmware
/// processing, then reads via `vendor_get_report`. If `response[0] == cmd_byte`
/// (echo match), returns the full 64-byte response. Otherwise retries up to
/// `QUERY_RETRIES` (5) times.
///
/// # Errors
///
/// Returns `TransportError::EchoMismatch` if all retries exhaust without a match.
/// Returns `TransportError::Usb` if any USB transfer fails fatally.
pub fn query_command(
    session: &UsbSession,
    cmd_byte: u8,
    data: &[u8],
    checksum: ChecksumType,
) -> Result<[u8; 64], TransportError> {
    let frame = build_command(cmd_byte, data, checksum);
    let mut last_actual: u8 = 0;

    for attempt in 0..timing::QUERY_RETRIES {
        session.vendor_set_report(&frame[1..])?;
        std::thread::sleep(Duration::from_millis(timing::DEFAULT_DELAY_MS));
        let response = session.vendor_get_report()?;

        if response[0] == cmd_byte {
            return Ok(response);
        }

        last_actual = response[0];
        log::debug!(
            "Echo mismatch attempt {}: expected 0x{:02X}, got 0x{:02X}",
            attempt + 1,
            cmd_byte,
            last_actual
        );
    }

    Err(TransportError::EchoMismatch {
        expected: cmd_byte,
        actual: last_actual,
        attempts: timing::QUERY_RETRIES,
    })
}

/// Send a command without reading a response (fire-and-forget with retry).
///
/// Builds a 65-byte frame via `build_command`, sends `&frame[1..]` (64 bytes,
/// skipping the report ID) via `vendor_set_report`. On success, returns immediately.
/// On USB error, retries up to `SEND_RETRIES` (3) times.
///
/// # Errors
///
/// Returns the last `TransportError::Usb` if all retries fail.
pub fn send_command(
    session: &UsbSession,
    cmd_byte: u8,
    data: &[u8],
    checksum: ChecksumType,
) -> Result<(), TransportError> {
    let frame = build_command(cmd_byte, data, checksum);
    let mut last_err: Option<TransportError> = None;

    for attempt in 0..timing::SEND_RETRIES {
        match session.vendor_set_report(&frame[1..]) {
            Ok(()) => return Ok(()),
            Err(e) => {
                log::warn!(
                    "Send attempt {} for 0x{:02X} failed: {}",
                    attempt + 1,
                    cmd_byte,
                    e
                );
                last_err = Some(e);
            }
        }
    }

    Err(last_err.expect("SEND_RETRIES must be >= 1"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_command_exists_with_correct_signature() {
        // Verify the function signature compiles.
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
        assert_eq!(monsgeek_protocol::timing::QUERY_RETRIES, 5);
        assert_eq!(monsgeek_protocol::timing::SEND_RETRIES, 3);
        assert_eq!(monsgeek_protocol::timing::DEFAULT_DELAY_MS, 100);
    }
}
