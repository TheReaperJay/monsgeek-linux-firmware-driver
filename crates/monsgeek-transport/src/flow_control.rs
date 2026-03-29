//! Flow-control layer for echo-matched queries and fire-and-forget sends.
//!
//! Wraps `UsbSession` to provide:
//! - `query_command`: send + read with echo byte matching and retry (up to 5 attempts)
//! - `send_command`: fire-and-forget with retry on USB error (up to 3 attempts)
//!
//! Both functions build the HID frame via `monsgeek_protocol::build_command` and
//! strip the report ID byte before passing to `UsbSession::vendor_set_report`.
//! The centralized `CommandController` owns cross-command timing policy.
//! `query_command` still sleeps after SET_REPORT to give firmware time to
//! prepare the response before GET_REPORT.

use std::time::Duration;

use monsgeek_protocol::{ChecksumType, build_command, cmd, timing};

use crate::error::TransportError;
use crate::usb::UsbSession;

/// Send a command and read the response, retrying until the echo byte matches.
///
/// Builds a 65-byte frame via `build_command`, sends `&frame[1..]` (64 bytes,
/// skipping the report ID) via `vendor_set_report`, sleeps 100ms for firmware
/// processing, then reads via `vendor_get_report`.
///
/// Echo validation accepts either:
/// - normalized payload form: `response[0] == cmd_byte`
/// - zero-prefixed form where command is shifted right by a small offset
///   (for example: `response[0] == 0x00 && response[1] == cmd_byte`)
///
/// In the zero-prefixed form, the response is normalized by shifting left to
/// return cmd at index 0 for callers.
/// Otherwise retries up to `QUERY_RETRIES` (5) times.
///
/// # Errors
///
/// Returns `TransportError::EchoMismatch` if all retries exhaust without a match.
/// Returns `TransportError::Usb` if any USB transfer fails fatally.
pub(crate) fn query_command(
    session: &UsbSession,
    cmd_byte: u8,
    data: &[u8],
    checksum: ChecksumType,
) -> Result<[u8; 64], TransportError> {
    let frame = build_command(cmd_byte, data, checksum);
    log::debug!(
        "query 0x{:02X} ({}) frame[1..10]={:02X?} checksum={:?} data_len={}",
        cmd_byte,
        cmd::name(cmd_byte),
        &frame[1..10.min(frame.len())],
        checksum,
        data.len()
    );
    let mut last_actual: u8 = 0;
    let mut last_err: Option<TransportError> = None;

    if is_probable_dongle_session(session) && !is_dongle_local_command(cmd_byte) {
        if let Some(response) = query_via_dongle_forward_path(session, cmd_byte, &frame[1..])? {
            return Ok(response);
        }
    }

    for attempt in 0..timing::QUERY_RETRIES {
        if let Err(err) = session.vendor_set_report(&frame[1..]) {
            log::warn!(
                "Query attempt {} for 0x{:02X} ({}) send failed: {}",
                attempt + 1,
                cmd_byte,
                cmd::name(cmd_byte),
                err
            );
            last_err = Some(err);
            if attempt + 1 < timing::QUERY_RETRIES {
                std::thread::sleep(Duration::from_millis(timing::DEFAULT_DELAY_MS));
                continue;
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(timing::DEFAULT_DELAY_MS));
        let response = match session.vendor_get_report() {
            Ok(response) => response,
            Err(err) => {
                log::warn!(
                    "Query attempt {} for 0x{:02X} ({}) read failed: {}",
                    attempt + 1,
                    cmd_byte,
                    cmd::name(cmd_byte),
                    err
                );
                last_err = Some(err);
                if attempt + 1 < timing::QUERY_RETRIES {
                    std::thread::sleep(Duration::from_millis(timing::DEFAULT_DELAY_MS));
                    continue;
                }
                break;
            }
        };

        if let Some(normalized) = normalize_query_response(cmd_byte, response) {
            return Ok(normalized);
        }

        if is_empty_placeholder_response(&response) {
            if let Some(normalized) = try_read_cached_response_after_flush(session, cmd_byte) {
                return Ok(normalized);
            }
            if let Some(normalized) = try_read_via_dongle_poll_cycle(session, cmd_byte) {
                return Ok(normalized);
            }

            match session.vendor_get_report() {
                Ok(followup) => {
                    if let Some(normalized) = normalize_query_response(cmd_byte, followup) {
                        return Ok(normalized);
                    }
                    last_actual = followup[0];
                    log::debug!(
                        "Echo mismatch attempt {} follow-up: expected 0x{:02X}, got 0x{:02X} prefix={:02X?}",
                        attempt + 1,
                        cmd_byte,
                        last_actual,
                        &followup[..4]
                    );
                    continue;
                }
                Err(err) => {
                    log::debug!(
                        "Query attempt {} follow-up read failed for 0x{:02X} ({}): {}",
                        attempt + 1,
                        cmd_byte,
                        cmd::name(cmd_byte),
                        err
                    );
                }
            }
        }

        last_actual = response[0];
        log::debug!(
            "Echo mismatch attempt {}: expected 0x{:02X}, got 0x{:02X} prefix={:02X?}",
            attempt + 1,
            cmd_byte,
            last_actual,
            &response[..4]
        );
    }

    if let Some(err) = last_err {
        return Err(err);
    }

    Err(TransportError::EchoMismatch {
        expected: cmd_byte,
        actual: last_actual,
        attempts: timing::QUERY_RETRIES,
    })
}

fn normalize_query_response(cmd_byte: u8, response: [u8; 64]) -> Option<[u8; 64]> {
    if response[0] == cmd_byte {
        return Some(response);
    }

    // Some stacks can surface zero-prefixed responses where the echoed command
    // is shifted right by one or more bytes.
    for offset in 1..=8 {
        if response[offset] == cmd_byte && response[..offset].iter().all(|b| *b == 0) {
            let mut normalized = [0u8; 64];
            normalized[..(64 - offset)].copy_from_slice(&response[offset..64]);
            return Some(normalized);
        }
    }

    None
}

fn is_empty_placeholder_response(response: &[u8; 64]) -> bool {
    response[..8].iter().all(|b| *b == 0)
}

fn is_probable_dongle_session(session: &UsbSession) -> bool {
    // M5W runtime dongle PID from registry/runtime paths.
    matches!(session.product_id(), Some(0x4011))
}

fn is_dongle_local_command(cmd_byte: u8) -> bool {
    matches!(
        cmd_byte,
        cmd::GET_DONGLE_INFO
            | cmd::SET_CTRL_BYTE
            | cmd::GET_DONGLE_STATUS
            | cmd::ENTER_PAIRING
            | cmd::PAIRING_CMD
            | cmd::GET_RF_INFO
            | cmd::GET_CACHED_RESPONSE
            | cmd::GET_DONGLE_ID
            | cmd::SET_RESPONSE_SIZE
    )
}

fn query_via_dongle_forward_path(
    session: &UsbSession,
    cmd_byte: u8,
    cmd_payload: &[u8],
) -> Result<Option<[u8; 64]>, TransportError> {
    const POLL_RETRIES_PER_SEND: usize = 15;
    const POLL_DELAY_MS: u64 = 20;

    for send_attempt in 0..timing::QUERY_RETRIES {
        session.vendor_set_report(cmd_payload)?;
        std::thread::sleep(Duration::from_millis(2));

        for _ in 0..POLL_RETRIES_PER_SEND {
            if let Some(normalized) = try_read_via_dongle_poll_cycle(session, cmd_byte) {
                log::debug!(
                    "dongle-forward query resolved 0x{:02X} ({}) on send attempt {}",
                    cmd_byte,
                    cmd::name(cmd_byte),
                    send_attempt + 1
                );
                return Ok(Some(normalized));
            }
            std::thread::sleep(Duration::from_millis(POLL_DELAY_MS));
        }
    }

    Ok(None)
}

fn try_read_cached_response_after_flush(session: &UsbSession, cmd_byte: u8) -> Option<[u8; 64]> {
    // Dongle path: request cached keyboard response (0xFC), then read it.
    // On direct wired keyboards this may not yield anything useful.
    let flush_frame = build_command(cmd::GET_CACHED_RESPONSE, &[], ChecksumType::Bit7);
    if session.vendor_set_report(&flush_frame[1..]).is_err() {
        return None;
    }
    std::thread::sleep(Duration::from_millis(timing::DEFAULT_DELAY_MS));
    let flushed = session.vendor_get_report().ok()?;
    normalize_query_response(cmd_byte, flushed)
}

fn try_read_via_dongle_poll_cycle(session: &UsbSession, cmd_byte: u8) -> Option<[u8; 64]> {
    // Dongle forwarding path:
    // 1) poll GET_DONGLE_STATUS (0xF7) until has_response=1
    // 2) fetch cached keyboard response via GET_CACHED_RESPONSE (0xFC)
    const POLL_RETRIES: usize = 5;
    const POLL_DELAY_MS: u64 = 20;

    for _ in 0..POLL_RETRIES {
        let status_frame = build_command(cmd::GET_DONGLE_STATUS, &[], ChecksumType::Bit7);
        if session.vendor_set_report(&status_frame[1..]).is_err() {
            return None;
        }
        std::thread::sleep(Duration::from_millis(POLL_DELAY_MS));
        let status = match session.vendor_get_report() {
            Ok(status) => status,
            Err(_) => return None,
        };

        if !dongle_status_has_response(&status) {
            std::thread::sleep(Duration::from_millis(POLL_DELAY_MS));
            continue;
        }

        if let Some(normalized) = try_read_cached_response_after_flush(session, cmd_byte) {
            return Some(normalized);
        }
    }

    None
}

fn dongle_status_has_response(status: &[u8; 64]) -> bool {
    // Status layout after report-id stripping (reference):
    // [has_response, kb_battery, 0, kb_charging, 1, rf_ready, ...]
    // Some stacks may leak report-id at index 0:
    // [0, has_response, kb_battery, ...]
    if status[0] == 0 && status[1] <= 1 {
        status[1] != 0
    } else {
        status[0] != 0
    }
}

/// Send a command without reading a response (fire-and-forget).
///
/// This matches the reference driver behavior for SET commands:
/// issue SET_REPORT and return success/failure for that write only.
/// Any follow-up reads are handled explicitly by query/read paths.
///
/// Builds a 65-byte frame via `build_command`, then sends `&frame[1..]`
/// (64 bytes, skipping the report ID) via `vendor_set_report`.
///
/// # Errors
///
/// Returns `TransportError::Usb` if the SET_REPORT fails after retries.
pub(crate) fn send_command(
    session: &UsbSession,
    cmd_byte: u8,
    data: &[u8],
    checksum: ChecksumType,
) -> Result<(), TransportError> {
    let frame = build_command(cmd_byte, data, checksum);
    log::debug!(
        "send 0x{:02X} ({}) wire[0..16]={:02X?} checksum={:?} data_len={}",
        cmd_byte,
        cmd::name(cmd_byte),
        &frame[1..17.min(frame.len())],
        checksum,
        data.len()
    );
    let mut last_err: Option<TransportError> = None;

    for attempt in 0..timing::SEND_RETRIES {
        match session.vendor_set_report(&frame[1..]) {
            Ok(()) => return Ok(()),
            Err(e) => {
                log::warn!(
                    "Send attempt {} for 0x{:02X} ({}) failed: {}",
                    attempt + 1,
                    cmd_byte,
                    cmd::name(cmd_byte),
                    e
                );
                last_err = Some(e);
                if attempt + 1 < timing::SEND_RETRIES {
                    std::thread::sleep(Duration::from_millis(timing::DEFAULT_DELAY_MS));
                }
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

    #[test]
    fn normalize_query_response_accepts_direct_echo() {
        let mut response = [0u8; 64];
        response[0] = 0x8F;
        response[1] = 0x12;

        let normalized = normalize_query_response(0x8F, response).expect("must match");
        assert_eq!(normalized[0], 0x8F);
        assert_eq!(normalized[1], 0x12);
    }

    #[test]
    fn normalize_query_response_accepts_report_id_prefixed_echo() {
        let mut response = [0u8; 64];
        response[0] = 0x00;
        response[1] = 0x8F;
        response[2] = 0x34;

        let normalized = normalize_query_response(0x8F, response).expect("must match");
        assert_eq!(normalized[0], 0x8F);
        assert_eq!(normalized[1], 0x34);
        assert_eq!(normalized[63], 0x00);
    }

    #[test]
    fn normalize_query_response_accepts_two_zero_prefixed_echo() {
        let mut response = [0u8; 64];
        response[0] = 0x00;
        response[1] = 0x00;
        response[2] = 0x8F;
        response[3] = 0x56;

        let normalized = normalize_query_response(0x8F, response).expect("must match");
        assert_eq!(normalized[0], 0x8F);
        assert_eq!(normalized[1], 0x56);
    }

    #[test]
    fn normalize_query_response_rejects_mismatch() {
        let mut response = [0u8; 64];
        response[0] = 0x00;
        response[1] = 0x81;

        assert!(normalize_query_response(0x8F, response).is_none());
    }

    #[test]
    fn empty_placeholder_detection_checks_prefix() {
        let mut response = [0u8; 64];
        assert!(is_empty_placeholder_response(&response));

        response[7] = 0x01;
        assert!(!is_empty_placeholder_response(&response));
    }

    #[test]
    fn dongle_status_has_response_normalized_layout() {
        let mut status = [0u8; 64];
        status[0] = 1;
        assert!(dongle_status_has_response(&status));
        status[0] = 0;
        assert!(!dongle_status_has_response(&status));
    }

    #[test]
    fn dongle_status_has_response_report_id_leaked_layout() {
        let mut status = [0u8; 64];
        status[0] = 0;
        status[1] = 1;
        assert!(dongle_status_has_response(&status));
        status[1] = 0;
        assert!(!dongle_status_has_response(&status));
    }
}
