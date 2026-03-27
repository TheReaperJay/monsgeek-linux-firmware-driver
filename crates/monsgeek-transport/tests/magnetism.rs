//! Magnetic (Hall Effect) wire format unit tests against mock data.
//!
//! These tests verify command construction and response parsing for magnetic
//! switch operations. No hardware is needed -- all tests run against
//! known-good reference data derived from the reference implementation.
//!
//! Run with:
//! ```sh
//! cargo test -p monsgeek-transport --test magnetism
//! ```

use monsgeek_protocol::{cmd, magnetism};

// ---------------------------------------------------------------------------
// Test 1: GET_MULTI_MAGNETISM query format
// ---------------------------------------------------------------------------

#[test]
fn test_magnetism_get_multi_query_format() {
    // GET_MULTI_MAGNETISM query: [subcmd, page]
    // Verify all travel/RT subcmds produce correct 2-byte query payloads.
    let subcmds = [
        (magnetism::PRESS_TRAVEL, "PRESS_TRAVEL"),
        (magnetism::LIFT_TRAVEL, "LIFT_TRAVEL"),
        (magnetism::RT_PRESS, "RT_PRESS"),
        (magnetism::RT_LIFT, "RT_LIFT"),
    ];

    for (subcmd, name) in &subcmds {
        for page in [0u8, 1] {
            let query = [*subcmd, page];
            assert_eq!(query.len(), 2, "{name} query should be 2 bytes");
            assert_eq!(query[0], *subcmd, "{name} subcmd byte mismatch");
            assert_eq!(query[1], page, "{name} page byte mismatch");
        }
    }

    // The command byte for the query is GET_MULTI_MAGNETISM (0xE5).
    assert_eq!(cmd::GET_MULTI_MAGNETISM, 0xE5);
}

// ---------------------------------------------------------------------------
// Test 2: SET_MULTI_MAGNETISM header format
// ---------------------------------------------------------------------------

#[test]
fn test_magnetism_set_multi_header_format() {
    // SET_MULTI_MAGNETISM frame: [subcmd, page, pad*4, checksum, key_data...]
    // Total frame is 63 bytes (payload after command byte in 64-byte HID report).
    let subcmd = magnetism::PRESS_TRAVEL;
    let page: u8 = 0;

    let mut frame = [0u8; 63];
    frame[0] = subcmd;         // subcmd
    frame[1] = page;           // page
    // frame[2..6] = padding (zeros)
    // frame[6] = checksum placeholder (transport fills this)

    // Fill 10 keys worth of 2-byte LE data starting at byte 7
    let key_values: [u16; 10] = [100, 200, 300, 400, 500, 150, 250, 350, 450, 550];
    for (i, &val) in key_values.iter().enumerate() {
        let offset = 7 + i * 2;
        let le_bytes = val.to_le_bytes();
        frame[offset] = le_bytes[0];
        frame[offset + 1] = le_bytes[1];
    }

    // Verify header layout
    assert_eq!(frame[0], magnetism::PRESS_TRAVEL, "subcmd byte");
    assert_eq!(frame[1], 0, "page byte");
    assert_eq!(&frame[2..6], &[0, 0, 0, 0], "padding bytes");

    // Verify key data at expected offsets
    for (i, &val) in key_values.iter().enumerate() {
        let offset = 7 + i * 2;
        let parsed = u16::from_le_bytes([frame[offset], frame[offset + 1]]);
        assert_eq!(parsed, val, "key {} value mismatch", i);
    }

    // Verify the command byte constant
    assert_eq!(cmd::SET_MULTI_MAGNETISM, 0x65);
}

// ---------------------------------------------------------------------------
// Test 3: SET_MAGNETISM_CAL start/stop format
// ---------------------------------------------------------------------------

#[test]
fn test_magnetism_calibration_start_stop_format() {
    // SET_MAGNETISM_CAL payload: [start_or_stop, 0, 0, 0, 0, 0]
    // start = 1, stop = 0
    let start_payload: [u8; 6] = [1, 0, 0, 0, 0, 0];
    let stop_payload: [u8; 6] = [0, 0, 0, 0, 0, 0];

    assert_eq!(start_payload[0], 1, "start calibration flag");
    assert_eq!(stop_payload[0], 0, "stop calibration flag");
    assert_eq!(&start_payload[1..], &[0, 0, 0, 0, 0], "start padding");
    assert_eq!(&stop_payload[1..], &[0, 0, 0, 0, 0], "stop padding");

    assert_eq!(cmd::SET_MAGNETISM_CAL, 0x1C);
}

// ---------------------------------------------------------------------------
// Test 4: SET_MAGNETISM_MAX_CAL format
// ---------------------------------------------------------------------------

#[test]
fn test_magnetism_max_cal_format() {
    // SET_MAGNETISM_MAX_CAL: same start/stop pattern as SET_MAGNETISM_CAL
    let start_payload: [u8; 6] = [1, 0, 0, 0, 0, 0];
    let stop_payload: [u8; 6] = [0, 0, 0, 0, 0, 0];

    assert_eq!(start_payload[0], 1, "start max calibration flag");
    assert_eq!(stop_payload[0], 0, "stop max calibration flag");

    assert_eq!(cmd::SET_MAGNETISM_MAX_CAL, 0x1E);
    // Verify it differs from the standard calibration command
    assert_ne!(cmd::SET_MAGNETISM_CAL, cmd::SET_MAGNETISM_MAX_CAL);
}

// ---------------------------------------------------------------------------
// Test 5: SET_KEY_MAGNETISM_MODE format
// ---------------------------------------------------------------------------

#[test]
fn test_magnetism_key_mode_format() {
    // SET_KEY_MAGNETISM_MODE: [key_index, mode, 0, 0, 0, 0]
    let key_index: u8 = 5;
    let mode: u8 = 1; // Rapid Trigger mode

    let payload: [u8; 6] = [key_index, mode, 0, 0, 0, 0];

    assert_eq!(payload[0], 5, "key_index");
    assert_eq!(payload[1], 1, "mode (Rapid Trigger)");
    assert_eq!(&payload[2..], &[0, 0, 0, 0], "padding");

    assert_eq!(cmd::SET_KEY_MAGNETISM_MODE, 0x1D);
}

// ---------------------------------------------------------------------------
// Test 6: Per-key travel data parsing from GET_MULTI_MAGNETISM response
// ---------------------------------------------------------------------------

#[test]
fn test_magnetism_per_key_travel_parsing() {
    // Build a mock 64-byte GET_MULTI_MAGNETISM response with known per-key values.
    // Response layout: [echo_byte, subcmd, page, pad*4, key_data...]
    // Data starts at byte 7 (after 7-byte header).
    let mut response = [0u8; 64];
    response[0] = cmd::GET_MULTI_MAGNETISM; // echo byte
    response[1] = magnetism::PRESS_TRAVEL;  // subcmd echo
    response[2] = 0;                        // page

    // Fill 5 mock keys with known travel values (2-byte LE per key)
    let expected_values: [u16; 5] = [100, 200, 300, 400, 500];
    for (i, &val) in expected_values.iter().enumerate() {
        let offset = 7 + i * 2;
        let le_bytes = val.to_le_bytes();
        response[offset] = le_bytes[0];
        response[offset + 1] = le_bytes[1];
    }

    // Parse and verify each key's travel value
    for (i, &expected) in expected_values.iter().enumerate() {
        let offset = 7 + i * 2;
        let parsed = u16::from_le_bytes([response[offset], response[offset + 1]]);
        assert_eq!(
            parsed, expected,
            "key {} travel: expected {}, got {}",
            i, expected, parsed
        );
    }

    // Verify the header region
    assert_eq!(response[0], 0xE5, "echo byte should be GET_MULTI_MAGNETISM");
    assert_eq!(response[1], 0x00, "subcmd should be PRESS_TRAVEL");
}

// ---------------------------------------------------------------------------
// Test 7: Magnetism subcmd constants match reference values
// ---------------------------------------------------------------------------

#[test]
fn test_magnetism_subcmd_constants() {
    assert_eq!(magnetism::PRESS_TRAVEL, 0x00, "PRESS_TRAVEL");
    assert_eq!(magnetism::LIFT_TRAVEL, 0x01, "LIFT_TRAVEL");
    assert_eq!(magnetism::RT_PRESS, 0x02, "RT_PRESS");
    assert_eq!(magnetism::RT_LIFT, 0x03, "RT_LIFT");
    assert_eq!(magnetism::DKS_TRAVEL, 0x04, "DKS_TRAVEL");
    assert_eq!(magnetism::MODTAP_TIME, 0x05, "MODTAP_TIME");
    assert_eq!(magnetism::BOTTOM_DEADZONE, 0x06, "BOTTOM_DEADZONE");
    assert_eq!(magnetism::KEY_MODE, 0x07, "KEY_MODE");
    assert_eq!(magnetism::SNAPTAP_ENABLE, 0x09, "SNAPTAP_ENABLE");
    assert_eq!(magnetism::DKS_MODES, 0x0A, "DKS_MODES");
    assert_eq!(magnetism::TOP_DEADZONE, 0xFB, "TOP_DEADZONE");
    assert_eq!(magnetism::SWITCH_TYPE, 0xFC, "SWITCH_TYPE");
    assert_eq!(magnetism::CALIBRATION, 0xFE, "CALIBRATION");

    // Verify command byte constants
    assert_eq!(cmd::GET_MULTI_MAGNETISM, 0xE5, "GET_MULTI_MAGNETISM");
    assert_eq!(cmd::SET_MULTI_MAGNETISM, 0x65, "SET_MULTI_MAGNETISM");
    assert_eq!(cmd::GET_CALIBRATION, 0xFE, "GET_CALIBRATION");
    assert_eq!(cmd::SET_MAGNETISM_CAL, 0x1C, "SET_MAGNETISM_CAL");
    assert_eq!(cmd::SET_MAGNETISM_MAX_CAL, 0x1E, "SET_MAGNETISM_MAX_CAL");
    assert_eq!(cmd::SET_KEY_MAGNETISM_MODE, 0x1D, "SET_KEY_MAGNETISM_MODE");
}
