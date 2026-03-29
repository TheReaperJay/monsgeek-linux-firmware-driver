#![cfg(feature = "hardware")]
//! Hardware-gated integration tests for the monsgeek-inputd daemon components.
//!
//! These tests require a real M5W keyboard connected via USB. They verify:
//! - InputOnly session opens and reads from IF0
//! - uinput virtual device creation works
//! - InputOnly and ControlOnly sessions coexist on the same device
//! - InputProcessor produces key actions from live HID reports
//!
//! Run with:
//! ```sh
//! cargo test -p monsgeek-inputd --features hardware -- --test-threads=1 --nocapture
//! ```
//!
//! All tests are `#[ignore]` to prevent accidental execution in CI.

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use monsgeek_inputd::uinput_device::create_uinput_device;
use monsgeek_protocol::DeviceRegistry;
use monsgeek_transport::input::InputProcessor;
use monsgeek_transport::usb::{SessionMode, UsbSession};
use rusb::UsbContext;

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

/// Look up the M5W VID/PID from the registry for session opening.
fn m5w_vid_pid(registry: &DeviceRegistry) -> (u16, u16) {
    let def = registry
        .find_by_id(M5W_DEVICE_ID)
        .expect("M5W (device ID 1308) not found in registry");
    (def.vid, def.pid)
}

/// Open an InputOnly session with STALL recovery.
///
/// The yc3121 firmware often STALLs SET_PROTOCOL(Boot) after the kernel's
/// usbhid driver has probed IF0/IF1. A USB reset clears the STALL. This
/// helper performs a raw USB reset via rusb, waits for re-enumeration, then
/// opens InputOnly cleanly.
fn open_input_only_with_recovery(vid: u16, pid: u16) -> UsbSession {
    match UsbSession::open_with_mode(vid, pid, SessionMode::InputOnly) {
        Ok(session) => session,
        Err(first_err) => {
            log::info!(
                "InputOnly open failed ({}), attempting STALL recovery via USB reset",
                first_err
            );
            // Perform a raw USB reset outside of UsbSession to clear the STALL.
            // We open the device directly via rusb, reset it, and wait for
            // re-enumeration before retrying the InputOnly open.
            let ctx = rusb::Context::new().expect("failed to create USB context for reset");
            let device = ctx
                .devices()
                .expect("failed to enumerate USB devices")
                .iter()
                .find(|d| {
                    d.device_descriptor()
                        .map(|desc| desc.vendor_id() == vid && desc.product_id() == pid)
                        .unwrap_or(false)
                })
                .expect("device not found for reset");
            let handle = device.open().expect("failed to open device for reset");
            let _ = handle.reset();
            drop(handle);
            // Wait for re-enumeration after reset
            std::thread::sleep(Duration::from_millis(1500));

            UsbSession::open_with_mode(vid, pid, SessionMode::InputOnly)
                .expect("InputOnly open after USB reset also failed")
        }
    }
}

// ---------------------------------------------------------------------------
// Test 1: InputOnly session opens and can read from IF0
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_input_only_session_opens() {
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    simple_logger::init_with_level(log::Level::Debug).ok();

    let registry = load_registry();
    let (vid, pid) = m5w_vid_pid(&registry);

    let session = open_input_only_with_recovery(vid, pid);

    assert_eq!(session.mode(), SessionMode::InputOnly);
    println!(
        "InputOnly session opened on bus {:03} addr {:03}",
        session.bus_number(),
        session.address()
    );

    // Read with a short timeout -- Ok with 0 bytes (timeout) or Ok with data
    // are both acceptable. The point is it does not error.
    let mut report = [0u8; 8];
    let result = session.read_report_with_timeout(&mut report, Duration::from_millis(100));
    match result {
        Ok(n) => println!("read_report_with_timeout returned {} bytes", n),
        Err(e) => panic!(
            "read_report_with_timeout should not fail on InputOnly session: {}",
            e
        ),
    }

    // Drop should not panic
    drop(session);
    println!("InputOnly session dropped cleanly");
}

// ---------------------------------------------------------------------------
// Test 2: uinput virtual device creation
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_uinput_device_creation() {
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    simple_logger::init_with_level(log::Level::Debug).ok();

    let device = create_uinput_device("test-monsgeek-inputd");
    assert!(
        device.is_ok(),
        "Failed to create uinput device: {:?}",
        device.err()
    );

    let device = device.unwrap();
    println!("uinput device created successfully");

    // Drop should not error
    drop(device);
    println!("uinput device dropped cleanly");
}

// ---------------------------------------------------------------------------
// Test 3: InputOnly + ControlOnly coexistence on same device
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_daemon_coexistence() {
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    simple_logger::init_with_level(log::Level::Debug).ok();

    let registry = load_registry();
    let (vid, pid) = m5w_vid_pid(&registry);

    // Open InputOnly first (claims IF0/IF1)
    let input_session = open_input_only_with_recovery(vid, pid);
    println!(
        "InputOnly session opened on bus {:03} addr {:03}",
        input_session.bus_number(),
        input_session.address()
    );

    // Open ControlOnly second (claims IF2 only)
    let control_session = UsbSession::open_with_mode(vid, pid, SessionMode::ControlOnly)
        .expect("failed to open ControlOnly session while InputOnly is active");
    println!(
        "ControlOnly session opened on bus {:03} addr {:03}",
        control_session.bus_number(),
        control_session.address()
    );

    // Verify the InputOnly session can still read from IF0
    let mut report = [0u8; 8];
    let result = input_session.read_report_with_timeout(&mut report, Duration::from_millis(100));
    match result {
        Ok(n) => println!("IF0 read with ControlOnly active: {} bytes", n),
        Err(e) => panic!(
            "IF0 read should not fail while ControlOnly is also active: {}",
            e
        ),
    }

    // Drop both sessions cleanly
    drop(control_session);
    drop(input_session);
    println!("Both sessions dropped cleanly -- coexistence verified");
}

// ---------------------------------------------------------------------------
// Test 4: InputProcessor produces actions from live HID reports (diagnostic)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_input_poll_produces_actions() {
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    simple_logger::init_with_level(log::Level::Debug).ok();

    let registry = load_registry();
    let (vid, pid) = m5w_vid_pid(&registry);

    let session = open_input_only_with_recovery(vid, pid);

    let mut processor = InputProcessor::new(15); // 15ms debounce
    let mut report = [0u8; 8];
    let mut total_actions = 0;

    println!("Polling IF0 for 2 seconds -- press a key on the M5W to see actions");

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        match session.read_report_with_timeout(&mut report, Duration::from_millis(10)) {
            Ok(n) if n >= 8 => {
                let actions = processor.process_report(&report);
                if !actions.is_empty() {
                    for action in &actions {
                        println!(
                            "  KeyAction: keycode={} value={} ({})",
                            action.keycode,
                            action.value,
                            if action.value == 1 {
                                "press"
                            } else {
                                "release"
                            }
                        );
                    }
                    total_actions += actions.len();
                }
            }
            Ok(_) => {} // Timeout or short read
            Err(e) => {
                println!("Poll error (non-fatal): {}", e);
            }
        }
    }

    println!(
        "Diagnostic: {} key actions observed in 2s (0 is OK if no keys were pressed)",
        total_actions
    );

    // Release any held keys
    let releases = processor.release_all_keys();
    if !releases.is_empty() {
        println!("Released {} held keys on cleanup", releases.len());
    }

    drop(session);
    // This test always passes -- it is a diagnostic for visual verification.
}
