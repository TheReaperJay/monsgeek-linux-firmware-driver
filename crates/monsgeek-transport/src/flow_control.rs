//! Flow-control layer for echo-matched queries and fire-and-forget sends.
//!
//! Wraps `UsbSession` to provide:
//! - `query_command`: send + read with echo byte matching and retry (up to 5 attempts)
//! - `query_raw`: send + read with retry, accepting the first response without echo validation
//! - `send_command`: fire-and-forget with retry on USB error (up to 3 attempts)
//!
//! Both functions build the HID frame via `monsgeek_protocol::build_command` and
//! strip the report ID byte before passing to `UsbSession::vendor_set_report`.
//! The centralized `CommandController` owns cross-command timing policy.
//! `query_command` still sleeps after SET_REPORT to give firmware time to
//! prepare the response before GET_REPORT.

use std::time::{Duration, Instant};

use monsgeek_protocol::{ChecksumType, ControlTransport, build_command, cmd, timing};

use crate::error::TransportError;
use crate::runtime_config::runtime_config;
use crate::usb::UsbSession;

#[derive(Debug, Clone, Copy)]
struct QueryExecutionOptions {
    retries: usize,
    allow_dongle_forwarding: bool,
    allow_placeholder_followups: bool,
    dongle_forward_send_retries: usize,
    dongle_forward_poll_retries_per_send: usize,
    dongle_status_poll_retries: usize,
    dongle_forward_budget: Option<Duration>,
    fallback_to_direct_query: bool,
}

impl QueryExecutionOptions {
    fn regular() -> Self {
        Self {
            retries: timing::QUERY_RETRIES,
            allow_dongle_forwarding: true,
            allow_placeholder_followups: true,
            dongle_forward_send_retries: timing::QUERY_RETRIES,
            dongle_forward_poll_retries_per_send: 15,
            dongle_status_poll_retries: 5,
            dongle_forward_budget: None,
            fallback_to_direct_query: true,
        }
    }

    fn discovery() -> Self {
        let discovery = &runtime_config().discovery;
        Self {
            retries: discovery.query_retries.max(1),
            allow_dongle_forwarding: false,
            allow_placeholder_followups: false,
            dongle_forward_send_retries: 0,
            dongle_forward_poll_retries_per_send: 0,
            dongle_status_poll_retries: 0,
            dongle_forward_budget: None,
            fallback_to_direct_query: false,
        }
    }
}

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
    control_transport: ControlTransport,
    cmd_byte: u8,
    data: &[u8],
    checksum: ChecksumType,
) -> Result<[u8; 64], TransportError> {
    query_command_with_options(
        session,
        control_transport,
        cmd_byte,
        data,
        checksum,
        QueryExecutionOptions::regular(),
    )
}

/// Send a command and read the first response without echo-byte validation.
///
/// This matches the reference stack's `query_raw` behavior for page-based
/// commands like `GET_KEYMATRIX` and `GET_FN`, where the reply body is raw
/// page data rather than an echoed command frame.
pub(crate) fn query_raw(
    session: &UsbSession,
    cmd_byte: u8,
    data: &[u8],
    checksum: ChecksumType,
) -> Result<[u8; 64], TransportError> {
    let frame = build_command(cmd_byte, data, checksum);
    log::debug!(
        "query_raw 0x{:02X} ({}) frame[1..10]={:02X?} checksum={:?} data_len={}",
        cmd_byte,
        cmd::name(cmd_byte),
        &frame[1..10.min(frame.len())],
        checksum,
        data.len()
    );
    let mut last_err: Option<TransportError> = None;

    for attempt in 0..timing::QUERY_RETRIES {
        if let Err(err) = session.vendor_set_report(&frame[1..]) {
            log::warn!(
                "Raw query attempt {} for 0x{:02X} ({}) send failed: {}",
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
        match session.vendor_get_report() {
            Ok(response) => return Ok(response),
            Err(err) => {
                log::warn!(
                    "Raw query attempt {} for 0x{:02X} ({}) read failed: {}",
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
        }
    }

    Err(last_err.unwrap_or(TransportError::Timeout { cmd: cmd_byte }))
}

/// Discovery variant of [`query_command`] with bounded retries and no dongle
/// forwarding side-path.
///
/// Discovery should stay fast and non-intrusive:
/// - fewer retries than runtime command paths
/// - no cached-response/dongle polling loops
pub(crate) fn query_command_discovery(
    session: &UsbSession,
    cmd_byte: u8,
    data: &[u8],
    checksum: ChecksumType,
) -> Result<[u8; 64], TransportError> {
    query_command_with_options(
        session,
        ControlTransport::Direct,
        cmd_byte,
        data,
        checksum,
        QueryExecutionOptions::discovery(),
    )
}

fn query_command_with_options(
    session: &UsbSession,
    control_transport: ControlTransport,
    cmd_byte: u8,
    data: &[u8],
    checksum: ChecksumType,
    options: QueryExecutionOptions,
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

    let used_dongle_forwarding = options.allow_dongle_forwarding
        && control_transport == ControlTransport::DongleForward
        && !is_dongle_local_command(cmd_byte);
    if used_dongle_forwarding {
        if let Some(response) = query_via_dongle_forward_path(
            session,
            cmd_byte,
            &frame[1..],
            options.dongle_forward_send_retries,
            options.dongle_forward_poll_retries_per_send,
            options.dongle_status_poll_retries,
            options.dongle_forward_budget,
        )? {
            return Ok(response);
        }
        log::info!(
            "dongle-forward path exhausted for 0x{:02X} ({}), falling back_to_direct_query={}",
            cmd_byte,
            cmd::name(cmd_byte),
            options.fallback_to_direct_query
        );
        if !options.fallback_to_direct_query {
            return Err(TransportError::EchoMismatch {
                expected: cmd_byte,
                actual: 0x00,
                attempts: options.dongle_forward_send_retries,
            });
        }
    }

    for attempt in 0..options.retries {
        if let Err(err) = session.vendor_set_report(&frame[1..]) {
            log::warn!(
                "Query attempt {} for 0x{:02X} ({}) send failed: {}",
                attempt + 1,
                cmd_byte,
                cmd::name(cmd_byte),
                err
            );
            last_err = Some(err);
            if attempt + 1 < options.retries {
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
                if attempt + 1 < options.retries {
                    std::thread::sleep(Duration::from_millis(timing::DEFAULT_DELAY_MS));
                    continue;
                }
                break;
            }
        };

        if let Some(normalized) = normalize_query_response(cmd_byte, response) {
            return Ok(normalized);
        }

        if options.allow_placeholder_followups && is_empty_placeholder_response(&response) {
            if let Some(normalized) = try_read_cached_response_after_flush(session, cmd_byte) {
                return Ok(normalized);
            }
            if let Some(normalized) = try_read_via_dongle_poll_cycle(session, cmd_byte, 5) {
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
        attempts: options.retries,
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
    send_retries: usize,
    poll_retries_per_send: usize,
    status_poll_retries: usize,
    budget: Option<Duration>,
) -> Result<Option<[u8; 64]>, TransportError> {
    const FORWARD_INITIAL_WAIT_MS: u64 = 30;
    const POLL_DELAY_MS: u64 = 20;
    if send_retries == 0 && budget.is_none() {
        return Ok(None);
    }

    prepare_dongle_forwarding(session);

    let started = Instant::now();
    let max_send_attempts = send_retries.max(1);

    for send_attempt in 1..=max_send_attempts {
        if budget.is_some_and(|deadline| started.elapsed() >= deadline) {
            break;
        }
        session.vendor_set_report(cmd_payload)?;
        std::thread::sleep(Duration::from_millis(FORWARD_INITIAL_WAIT_MS));

        for poll_attempt in 1..=poll_retries_per_send.max(1) {
            if let Some(normalized) =
                try_read_via_dongle_poll_cycle(session, cmd_byte, status_poll_retries)
            {
                log::info!(
                    "dongle-forward query resolved 0x{:02X} ({}) on send attempt {} poll_attempt {}",
                    cmd_byte,
                    cmd::name(cmd_byte),
                    send_attempt,
                    poll_attempt
                );
                return Ok(Some(normalized));
            }
            std::thread::sleep(Duration::from_millis(POLL_DELAY_MS));
            if budget.is_some_and(|deadline| started.elapsed() >= deadline) {
                break;
            }
        }

        log::debug!(
            "dongle-forward send attempt {} exhausted for 0x{:02X} ({})",
            send_attempt,
            cmd_byte,
            cmd::name(cmd_byte)
        );
    }

    log::info!(
        "dongle-forward unresolved for 0x{:02X} ({}) after {} send attempt(s) elapsed_ms={}",
        cmd_byte,
        cmd::name(cmd_byte),
        max_send_attempts,
        started.elapsed().as_millis()
    );
    Ok(None)
}

fn prepare_dongle_forwarding(session: &UsbSession) {
    // Ensure the dongle returns full keyboard responses for cached reads.
    let set_size = build_command(cmd::SET_RESPONSE_SIZE, &[64], ChecksumType::Bit7);
    if session.vendor_set_report(&set_size[1..]).is_ok() {
        std::thread::sleep(Duration::from_millis(2));
        let _ = session.vendor_get_report_with_timeout(Duration::from_millis(80));
    } else {
        log::debug!("dongle-forward prepare: SET_RESPONSE_SIZE write failed");
    }
}

fn try_read_cached_response_after_flush(session: &UsbSession, cmd_byte: u8) -> Option<[u8; 64]> {
    const FLUSH_READ_TIMEOUT_MS: u64 = 120;
    // Dongle path: request cached keyboard response (0xFC), then read it.
    // On direct wired keyboards this may not yield anything useful.
    let flush_frame = build_command(cmd::GET_CACHED_RESPONSE, &[], ChecksumType::Bit7);
    if let Err(err) = session.vendor_set_report(&flush_frame[1..]) {
        log::debug!("dongle-forward flush write failed: {}", err);
        return None;
    }
    std::thread::sleep(Duration::from_millis(8));
    let flushed = match session
        .vendor_get_report_with_timeout(Duration::from_millis(FLUSH_READ_TIMEOUT_MS))
    {
        Ok(flushed) => flushed,
        Err(err) => {
            log::debug!("dongle-forward flush read failed: {}", err);
            return None;
        }
    };
    if let Some(normalized) = normalize_query_response(cmd_byte, flushed) {
        return Some(normalized);
    }
    log::debug!(
        "dongle-forward flush returned non-matching response for 0x{:02X}: prefix={:02X?}",
        cmd_byte,
        &flushed[..4]
    );
    None
}

fn try_read_via_dongle_poll_cycle(
    session: &UsbSession,
    cmd_byte: u8,
    status_poll_retries: usize,
) -> Option<[u8; 64]> {
    const STATUS_READ_TIMEOUT_MS: u64 = 80;
    // Dongle forwarding path:
    // 1) poll GET_DONGLE_STATUS (0xF7) until has_response=1
    // 2) fetch cached keyboard response via GET_CACHED_RESPONSE (0xFC)
    const POLL_DELAY_MS: u64 = 20;

    for _ in 0..status_poll_retries {
        let status_frame = build_command(cmd::GET_DONGLE_STATUS, &[], ChecksumType::Bit7);
        if let Err(err) = session.vendor_set_report(&status_frame[1..]) {
            log::debug!("dongle-forward status write failed: {}", err);
            return None;
        }
        std::thread::sleep(Duration::from_millis(POLL_DELAY_MS));
        let status = match session
            .vendor_get_report_with_timeout(Duration::from_millis(STATUS_READ_TIMEOUT_MS))
        {
            Ok(status) => status,
            Err(err) => {
                log::debug!("dongle-forward status read failed: {}", err);
                return None;
            }
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
            monsgeek_protocol::ControlTransport,
            u8,
            &[u8],
            monsgeek_protocol::ChecksumType,
        ) -> Result<[u8; 64], crate::error::TransportError> = query_command;
    }

    #[test]
    fn test_query_raw_exists_with_correct_signature() {
        let _fn_ptr: fn(
            &crate::usb::UsbSession,
            u8,
            &[u8],
            monsgeek_protocol::ChecksumType,
        ) -> Result<[u8; 64], crate::error::TransportError> = query_raw;
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
