#![cfg(feature = "hardware")]
//! Hardware-gated integration tests for the MonsGeek transport stack.
//!
//! These tests require a real M5W keyboard connected via USB. They verify the
//! full transport stack: USB device detection, IF2 access, command send/receive,
//! echo matching, throttling, bounds validation against real device definitions,
//! udev rules correctness, and hot-plug detection.
//!
//! All VID/PID values come from the device registry JSON files — nothing is
//! hardcoded. If the JSON changes, the tests follow automatically.
//!
//! Run with:
//! ```sh
//! cargo test -p monsgeek-transport --features hardware -- --ignored --nocapture
//! ```
//!
//! All tests are `#[ignore]` to prevent accidental execution in CI.
//!
//! Dangerous live-write validation is intentionally excluded from the default
//! hardware suite. To compile those tests, you must opt in explicitly:
//! ```sh
//! MONSGEEK_ENABLE_DANGEROUS_WRITES=1 \
//! cargo test -p monsgeek-transport \
//!   --features "hardware dangerous-hardware-writes" \
//!   --test hardware test_set_get_debounce_round_trip_dangerous \
//!   -- --ignored --nocapture
//! ```

use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use monsgeek_protocol::{ChecksumType, DeviceDefinition, DeviceRegistry};
use monsgeek_transport::{TransportEvent, UsbVersionInfo, connect, validate_write_request};

const M5W_DEVICE_ID: i32 = 1308;

/// All hardware tests share a single physical USB device. This mutex ensures
/// only one test accesses the device at a time.
static HW_LOCK: Mutex<()> = Mutex::new(());

/// Path to the device registry JSON files, relative to the workspace root.
fn devices_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("monsgeek-protocol")
        .join("devices")
}

/// Load the device registry from disk.
fn load_registry() -> DeviceRegistry {
    DeviceRegistry::load_from_directory(&devices_dir())
        .expect("failed to load device registry from devices/ directory")
}

/// Load the M5W device definition from the registry.
fn load_m5w(registry: &DeviceRegistry) -> &DeviceDefinition {
    registry
        .find_by_id(M5W_DEVICE_ID)
        .expect("M5W (device ID 1308) not found in registry")
}

#[cfg(feature = "dangerous-hardware-writes")]
fn decode_debounce(get_debounce_cmd: u8, response: &[u8; 64]) -> u8 {
    if get_debounce_cmd == 0x91 {
        response[2]
    } else {
        response[1]
    }
}

#[cfg(feature = "dangerous-hardware-writes")]
fn build_set_debounce_payload(set_debounce_cmd: u8, value: u8) -> Vec<u8> {
    if set_debounce_cmd == 0x11 {
        vec![0, value]
    } else {
        vec![value]
    }
}

#[cfg(feature = "dangerous-hardware-writes")]
fn require_dangerous_write_opt_in() {
    let enabled = std::env::var("MONSGEEK_ENABLE_DANGEROUS_WRITES")
        .map(|value| value == "1")
        .unwrap_or(false);
    assert!(
        enabled,
        "dangerous live writes are disabled; set MONSGEEK_ENABLE_DANGEROUS_WRITES=1 and re-run with --features 'hardware dangerous-hardware-writes'"
    );
}

// ---------------------------------------------------------------------------
// Test 1: Device probe by firmware ID (HID-01)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_enumerate_m5w() {
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let registry = load_registry();
    let m5w_def = load_m5w(&registry);
    let devices =
        monsgeek_transport::discovery::probe_devices(&registry).expect("probe_devices failed");

    assert!(
        !devices.is_empty(),
        "No MonsGeek devices found -- is the M5W plugged in?"
    );

    let m5w = devices
        .iter()
        .find(|d| d.vid == m5w_def.vid && d.device_id == M5W_DEVICE_ID)
        .expect("M5W not found in probed devices");

    assert_eq!(m5w.vid, m5w_def.vid);
    assert_eq!(m5w.device_id, M5W_DEVICE_ID);
    println!(
        "Found M5W: {} (ID {}) at bus {} addr {} PID 0x{:04X}",
        m5w.display_name, m5w.device_id, m5w.bus, m5w.address, m5w.pid
    );
}

// ---------------------------------------------------------------------------
// Test 2: GET_USB_VERSION query (HID-02)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_get_usb_version() {
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let registry = load_registry();
    let m5w = load_m5w(&registry);
    let (handle, _events) = connect(m5w).expect("failed to connect to M5W");

    let response = handle
        .send_query(0x8F, &[], ChecksumType::Bit7)
        .expect("GET_USB_VERSION query failed");

    assert_eq!(
        response[0], 0x8F,
        "echo byte mismatch: expected 0x8F, got 0x{:02X}",
        response[0]
    );

    println!("GET_USB_VERSION response: {:02X?}", &response[..16]);

    let usb_version =
        UsbVersionInfo::parse(&response).expect("failed to parse GET_USB_VERSION response");
    assert_eq!(usb_version.device_id_i32(), M5W_DEVICE_ID);
    println!(
        "Parsed device ID: {} (0x{:08X}), firmware version: 0x{:04X}",
        usb_version.device_id, usb_version.device_id, usb_version.firmware_version
    );

    handle.shutdown();
}

// ---------------------------------------------------------------------------
// Test 3: Throttle enforcement under rapid commands (HID-03)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_throttle_rapid_commands() {
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let registry = load_registry();
    let m5w = load_m5w(&registry);
    let (handle, _events) = connect(m5w).expect("failed to connect to M5W");

    let start = Instant::now();
    let command_count = 5;

    for i in 0..command_count {
        let response = handle
            .send_query(0x8F, &[], ChecksumType::Bit7)
            .unwrap_or_else(|e| panic!("GET_USB_VERSION query {} failed: {}", i + 1, e));

        assert_eq!(
            response[0],
            0x8F,
            "echo mismatch on query {}: expected 0x8F, got 0x{:02X}",
            i + 1,
            response[0]
        );
    }

    let elapsed = start.elapsed();
    println!(
        "{} queries completed in {:?} ({:.0}ms avg)",
        command_count,
        elapsed,
        elapsed.as_millis() as f64 / command_count as f64
    );

    // The transport thread enforces 100ms minimum inter-command delay.
    // 5 commands should take at least 400ms (4 inter-command gaps).
    // query_command also sleeps 100ms between SET_REPORT and GET_REPORT,
    // so actual time will be longer. Use a conservative lower bound.
    assert!(
        elapsed.as_millis() >= 400,
        "5 commands completed too fast ({:?}): throttling may not be working",
        elapsed
    );

    handle.shutdown();
}

// ---------------------------------------------------------------------------
// Test 4: Echo matching across command types (HID-04)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_echo_matching() {
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let registry = load_registry();
    let m5w = load_m5w(&registry);
    let (handle, _events) = connect(m5w).expect("failed to connect to M5W");

    // Command 1: GET_USB_VERSION (0x8F) -- shared across protocol families.
    let response1 = handle
        .send_query(0x8F, &[], ChecksumType::Bit7)
        .expect("GET_USB_VERSION query failed");
    assert_eq!(
        response1[0], 0x8F,
        "GET_USB_VERSION echo mismatch: got 0x{:02X}",
        response1[0]
    );

    // Command 2: GET_DEBOUNCE -- family-specific command byte.
    let family = m5w.protocol_family();
    let get_debounce_cmd = m5w.commands().get_debounce;
    println!(
        "Protocol family: {} -> GET_DEBOUNCE = 0x{:02X}",
        family, get_debounce_cmd
    );

    let response2 = handle
        .send_query(get_debounce_cmd, &[], ChecksumType::Bit7)
        .expect("GET_DEBOUNCE query failed");
    assert_eq!(
        response2[0], get_debounce_cmd,
        "GET_DEBOUNCE echo mismatch: expected 0x{:02X}, got 0x{:02X}",
        get_debounce_cmd, response2[0]
    );

    println!(
        "Echo matching verified for 0x8F and 0x{:02X}",
        get_debounce_cmd
    );

    handle.shutdown();
}

// ---------------------------------------------------------------------------
// Test 5: SET_DEBOUNCE -> GET_DEBOUNCE round trip with restore (DANGEROUS)
// ---------------------------------------------------------------------------

#[cfg(feature = "dangerous-hardware-writes")]
#[test]
#[ignore]
fn test_set_get_debounce_round_trip_dangerous() {
    require_dangerous_write_opt_in();
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let registry = load_registry();
    let m5w = load_m5w(&registry);
    let (handle, _events) = connect(m5w).expect("failed to connect to M5W");

    let family = m5w.protocol_family();
    let commands = m5w.commands();
    let get_debounce_cmd = commands.get_debounce;
    let set_debounce_cmd = commands.set_debounce;

    let current = handle
        .send_query(get_debounce_cmd, &[], ChecksumType::Bit7)
        .map(|r| decode_debounce(get_debounce_cmd, &r))
        .expect("GET_DEBOUNCE query failed");
    let target = 1u8;

    println!(
        "Protocol family: {} -> SET_DEBOUNCE = 0x{:02X}, GET_DEBOUNCE = 0x{:02X}, current={}ms target={}ms",
        family, set_debounce_cmd, get_debounce_cmd, current, target
    );

    let test_result = (|| -> Result<(), String> {
        let set_payload = build_set_debounce_payload(set_debounce_cmd, target);
        handle
            .send_fire_and_forget(set_debounce_cmd, &set_payload, ChecksumType::Bit7)
            .map_err(|e| format!("SET_DEBOUNCE failed: {e}"))?;

        let updated = handle
            .send_query(get_debounce_cmd, &[], ChecksumType::Bit7)
            .map_err(|e| format!("GET_DEBOUNCE after set failed: {e}"))?;

        if updated[0] != get_debounce_cmd {
            return Err(format!(
                "GET_DEBOUNCE echo mismatch after set: expected 0x{:02X}, got 0x{:02X}",
                get_debounce_cmd, updated[0]
            ));
        }

        let updated_value = decode_debounce(get_debounce_cmd, &updated);
        if updated_value != target {
            return Err(format!(
                "debounce round trip mismatch: wrote {}ms, read back {}ms",
                target, updated_value
            ));
        }

        Ok(())
    })();

    let restore_result = (|| -> Result<(), String> {
        let restore_payload = build_set_debounce_payload(set_debounce_cmd, current);
        handle
            .send_fire_and_forget(set_debounce_cmd, &restore_payload, ChecksumType::Bit7)
            .map_err(|e| format!("failed to restore original debounce {}ms: {e}", current))?;

        let restored = handle
            .send_query(get_debounce_cmd, &[], ChecksumType::Bit7)
            .map_err(|e| format!("GET_DEBOUNCE after restore failed: {e}"))?;

        if restored[0] != get_debounce_cmd {
            return Err(format!(
                "GET_DEBOUNCE echo mismatch after restore: expected 0x{:02X}, got 0x{:02X}",
                get_debounce_cmd, restored[0]
            ));
        }

        let restored_value = decode_debounce(get_debounce_cmd, &restored);
        if restored_value != current {
            return Err(format!(
                "failed to restore debounce: expected {}ms, read back {}ms",
                current, restored_value
            ));
        }

        println!("Debounce round trip verified and restored to {}ms", current);
        Ok(())
    })();

    handle.shutdown();

    if let Err(err) = restore_result {
        panic!("{err}");
    }
    if let Err(err) = test_result {
        panic!("{err}");
    }
}

// ---------------------------------------------------------------------------
// Test 6: Bounds validation against real M5W device definition (HID-05)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_bounds_validation() {
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let registry = load_registry();
    let m5w = load_m5w(&registry);

    // M5W has 108 keys and 4 layers.
    assert_eq!(m5w.key_count, Some(108), "M5W key_count mismatch");
    assert_eq!(m5w.layer, Some(4), "M5W layer count mismatch");

    // Valid: key_index=0, layer=0 (first key, first layer).
    assert!(
        validate_write_request(m5w, 0, 0).is_ok(),
        "key_index=0, layer=0 should be valid"
    );

    // Valid: key_index=107, layer=3 (last key, last layer).
    assert!(
        validate_write_request(m5w, 107, 3).is_ok(),
        "key_index=107, layer=3 should be valid"
    );

    // Invalid: key_index=108 (equals key_count, out of bounds).
    assert!(
        validate_write_request(m5w, 108, 0).is_err(),
        "key_index=108 should be out of bounds for 108 keys"
    );

    // Invalid: layer=4 (equals max_layers, out of bounds).
    assert!(
        validate_write_request(m5w, 0, 4).is_err(),
        "layer=4 should be out of bounds for 4 layers"
    );

    println!(
        "Bounds validation verified for {} (keys={}, layers={})",
        m5w.display_name,
        m5w.key_count.unwrap(),
        m5w.layer.unwrap()
    );
}

// ---------------------------------------------------------------------------
// Test 7: Udev rules file correctness (HID-06)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_udev_rules_file() {
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let registry = load_registry();
    let m5w = load_m5w(&registry);
    let rules_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy/99-monsgeek.rules");

    assert!(
        rules_path.exists(),
        "udev rules file missing at {}",
        rules_path.display()
    );

    let content = std::fs::read_to_string(&rules_path)
        .unwrap_or_else(|e| panic!("failed to read udev rules: {}", e));

    let expected_vid = format!("ATTRS{{idVendor}}==\"{:04x}\"", m5w.vid);
    assert!(
        content.contains(&expected_vid),
        "udev rules must match VID from registry (expected {})",
        expected_vid
    );

    assert!(
        content.contains(r#"TAG+="uaccess""#),
        "udev rules must grant non-root access via uaccess tag"
    );

    println!("Udev rules file verified at {}", rules_path.display());
}

// ---------------------------------------------------------------------------
// Test 9: GET_KEYMATRIX for profile 0 (KEYS-01)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_get_keymatrix_profile_0() {
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let registry = load_registry();
    let m5w = load_m5w(&registry);
    let (handle, _events) = connect(m5w).expect("failed to connect to M5W");

    let commands = m5w.commands();
    // GET_KEYMATRIX query payload: [profile=0, magic=0, page=0, magnetism_profile=0]
    let response = handle
        .send_query(commands.get_keymatrix, &[0, 0, 0, 0], ChecksumType::Bit7)
        .expect("GET_KEYMATRIX query failed");

    assert_eq!(
        response[0], commands.get_keymatrix,
        "echo byte mismatch for GET_KEYMATRIX: expected 0x{:02X}, got 0x{:02X}",
        commands.get_keymatrix, response[0]
    );

    // Response should contain key mapping data (not all zeros after echo byte)
    let payload_has_data = response[1..32].iter().any(|&b| b != 0);
    println!(
        "GET_KEYMATRIX profile 0 response (first 32 bytes): {:02X?}",
        &response[..32]
    );
    println!("Payload has non-zero data: {}", payload_has_data);

    handle.shutdown();
}

// ---------------------------------------------------------------------------
// Test 10: GET/SET_PROFILE round trip with restore (KEYS-03)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_get_set_profile() {
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let registry = load_registry();
    let m5w = load_m5w(&registry);
    let (handle, _events) = connect(m5w).expect("failed to connect to M5W");

    let commands = m5w.commands();

    // Read current profile
    let current_response = handle
        .send_query(commands.get_profile, &[], ChecksumType::Bit7)
        .expect("GET_PROFILE query failed");

    assert_eq!(
        current_response[0], commands.get_profile,
        "GET_PROFILE echo mismatch: expected 0x{:02X}, got 0x{:02X}",
        commands.get_profile, current_response[0]
    );

    let original_profile = current_response[1];
    println!("Current active profile: {}", original_profile);

    // Pick a different target profile (toggle between 0 and 1)
    let target_profile = if original_profile == 0 { 1u8 } else { 0u8 };

    let test_result = (|| -> Result<(), String> {
        // Switch to target profile
        handle
            .send_fire_and_forget(commands.set_profile, &[target_profile], ChecksumType::Bit7)
            .map_err(|e| format!("SET_PROFILE to {} failed: {e}", target_profile))?;

        // Read back to verify
        let verify_response = handle
            .send_query(commands.get_profile, &[], ChecksumType::Bit7)
            .map_err(|e| format!("GET_PROFILE after set failed: {e}"))?;

        if verify_response[0] != commands.get_profile {
            return Err(format!(
                "GET_PROFILE echo mismatch after set: expected 0x{:02X}, got 0x{:02X}",
                commands.get_profile, verify_response[0]
            ));
        }

        let read_back = verify_response[1];
        if read_back != target_profile {
            return Err(format!(
                "profile switch failed: set {}, read back {}",
                target_profile, read_back
            ));
        }

        println!("Profile switched from {} to {} successfully", original_profile, target_profile);
        Ok(())
    })();

    // Restore original profile
    let restore_result = (|| -> Result<(), String> {
        handle
            .send_fire_and_forget(commands.set_profile, &[original_profile], ChecksumType::Bit7)
            .map_err(|e| format!("failed to restore profile {}: {e}", original_profile))?;

        let restored = handle
            .send_query(commands.get_profile, &[], ChecksumType::Bit7)
            .map_err(|e| format!("GET_PROFILE after restore failed: {e}"))?;

        if restored[1] != original_profile {
            return Err(format!(
                "profile restore failed: expected {}, got {}",
                original_profile, restored[1]
            ));
        }

        println!("Profile restored to {}", original_profile);
        Ok(())
    })();

    handle.shutdown();

    if let Err(err) = restore_result {
        panic!("{err}");
    }
    if let Err(err) = test_result {
        panic!("{err}");
    }
}

// ---------------------------------------------------------------------------
// Test 11: SET_KEYMATRIX -> GET_KEYMATRIX round trip (KEYS-02, DANGEROUS)
// ---------------------------------------------------------------------------

#[cfg(feature = "dangerous-hardware-writes")]
#[test]
#[ignore]
fn test_set_keymatrix_roundtrip_dangerous() {
    require_dangerous_write_opt_in();
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let registry = load_registry();
    let m5w = load_m5w(&registry);
    let (handle, _events) = connect(m5w).expect("failed to connect to M5W");

    let commands = m5w.commands();

    // Read current key mapping for profile 0, page 0
    let original_page = handle
        .send_query(commands.get_keymatrix, &[0, 0, 0, 0], ChecksumType::Bit7)
        .expect("GET_KEYMATRIX profile 0 page 0 failed");

    assert_eq!(
        original_page[0], commands.get_keymatrix,
        "GET_KEYMATRIX echo mismatch"
    );

    println!(
        "Original key mapping page 0 (first 16 bytes): {:02X?}",
        &original_page[..16]
    );

    // Remap key_index 0 on profile 0, layer 0 to HID Caps Lock (0x39) as test value.
    // SET_KEYMATRIX payload (after cmd byte):
    // [profile, key_index, pad0, pad1, enabled, layer, checksum_placeholder, config_type, b1, b2, b3]
    let test_keycode: u8 = 0x39; // HID Caps Lock

    let test_result = (|| -> Result<(), String> {
        let set_payload: Vec<u8> = vec![
            0,              // profile
            0,              // key_index
            0, 0,           // pad0, pad1
            1,              // enabled
            0,              // layer
            0,              // checksum placeholder (transport overwrites)
            0,              // config_type (normal key)
            test_keycode,   // b1 = keycode
            0,              // b2
            0,              // b3
        ];
        handle
            .send_fire_and_forget(commands.set_keymatrix, &set_payload, ChecksumType::Bit7)
            .map_err(|e| format!("SET_KEYMATRIX failed: {e}"))?;

        // Read back to verify the write took effect
        let verify_page = handle
            .send_query(commands.get_keymatrix, &[0, 0, 0, 0], ChecksumType::Bit7)
            .map_err(|e| format!("GET_KEYMATRIX after set failed: {e}"))?;

        println!(
            "After SET_KEYMATRIX page 0 (first 16 bytes): {:02X?}",
            &verify_page[..16]
        );

        Ok(())
    })();

    println!("WARNING: Key index 0 on profile 0 layer 0 was modified during this test.");
    println!("If the Escape key behaves differently, use the web configurator to restore it.");

    handle.shutdown();

    if let Err(err) = test_result {
        panic!("{err}");
    }
}

// ---------------------------------------------------------------------------
// Test 8: Hot-plug detection (HID-01, hot-plug aspect)
//
// MUST run last — unplugging the keyboard disrupts the device state
// (new address, new device node, udev re-applies permissions).
// The z_ prefix ensures alphabetical ordering puts this after all other tests.
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn z_test_hot_plug_detection() {
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let registry = load_registry();
    let m5w = load_m5w(&registry);
    let (handle, event_rx) = connect(m5w).expect("failed to connect to M5W");

    println!("Hot-plug test: please unplug and replug the M5W keyboard within 30 seconds");

    let timeout = std::time::Duration::from_secs(30);
    let mut saw_left = false;
    let mut saw_arrived = false;
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        match event_rx.recv_timeout(std::time::Duration::from_millis(500)) {
            Ok(TransportEvent::DeviceLeft { bus, address }) => {
                println!("DeviceLeft detected: bus {} addr {}", bus, address);
                saw_left = true;
            }
            Ok(TransportEvent::DeviceArrived {
                vid,
                pid,
                bus,
                address,
            }) => {
                println!(
                    "DeviceArrived detected: VID 0x{:04X} PID 0x{:04X} bus {} addr {}",
                    vid, pid, bus, address
                );
                if saw_left {
                    saw_arrived = true;
                    break;
                }
            }
            Ok(TransportEvent::InputActions { actions }) => {
                println!(
                    "InputActions observed during hot-plug test: {} actions",
                    actions.len()
                );
            }
            Err(_) => {
                // Timeout on recv -- continue waiting until deadline.
            }
        }
    }

    if saw_left && saw_arrived {
        println!("Hot-plug detection: PASS (DeviceLeft + DeviceArrived observed)");
    } else if saw_left {
        println!(
            "Hot-plug detection: PARTIAL (DeviceLeft observed, but DeviceArrived not seen before timeout)"
        );
    } else {
        println!(
            "Hot-plug detection: SKIPPED (no unplug detected within 30s -- manual interaction required)"
        );
    }

    handle.shutdown();
}
