#![cfg(feature = "hardware")]
//! Standalone test: does reset-then-reopen clear the STALL on IF2?
//!
//! Tests the exact sequence from the reference project (monsgeek-hid-driver)
//! adapted to our architecture (only claim IF2, leave IF0/IF1 to the kernel).
//!
//! Run with:
//! ```sh
//! cargo test -p monsgeek-transport --features hardware --test stall_recovery -- --nocapture
//! ```

use std::time::Duration;

use monsgeek_transport::UsbVersionInfo;
use rusb::UsbContext;

const VID: u16 = 0x3151;
const PID: u16 = 0x4015;
const IF2: u8 = 2;
const RESET_SETTLE_MS: u64 = 500;

const REQUEST_TYPE_OUT: u8 = 0x21;
const REQUEST_TYPE_IN: u8 = 0xA1;
const HID_SET_REPORT: u8 = 0x09;
const HID_GET_REPORT: u8 = 0x01;
const FEATURE_REPORT_WVALUE: u16 = 0x0300;
const USB_TIMEOUT: Duration = Duration::from_secs(1);

fn find_device() -> rusb::Device<rusb::Context> {
    let ctx = rusb::Context::new().expect("failed to create USB context");
    ctx.devices()
        .expect("failed to list USB devices")
        .iter()
        .find(|d| {
            d.device_descriptor()
                .map(|desc| desc.vendor_id() == VID && desc.product_id() == PID)
                .unwrap_or(false)
        })
        .expect("M5W not found — is it plugged in?")
}

#[test]
fn test_reset_then_reopen_clears_stall() {
    println!("\n=== Step 1: Find device ===");
    let device = find_device();
    println!(
        "Found M5W at bus {} addr {}",
        device.bus_number(),
        device.address()
    );

    println!("\n=== Step 2: Open, reset, drop (reference project sequence) ===");
    {
        let handle = device.open().expect("failed to open device for reset");
        handle.reset().expect("USB reset failed");
        println!("Reset OK, dropping handle");
    }

    println!(
        "\n=== Step 3: Wait {}ms for re-enumeration ===",
        RESET_SETTLE_MS
    );
    std::thread::sleep(Duration::from_millis(RESET_SETTLE_MS));

    println!("\n=== Step 4: Re-find device (address may have changed) ===");
    let device = find_device();
    println!(
        "Re-found M5W at bus {} addr {}",
        device.bus_number(),
        device.address()
    );

    println!("\n=== Step 5: Open, detach IF2 only, claim IF2 ===");
    let handle = device.open().expect("failed to re-open device");

    match handle.kernel_driver_active(IF2) {
        Ok(true) => {
            handle
                .detach_kernel_driver(IF2)
                .expect("failed to detach IF2");
            println!("Detached kernel driver from IF2");
        }
        Ok(false) => println!("No kernel driver on IF2"),
        Err(e) => println!("kernel_driver_active check failed: {} (continuing)", e),
    }

    handle.claim_interface(IF2).expect("failed to claim IF2");
    println!("Claimed IF2");

    println!("\n=== Step 6: Send GET_USB_VERSION (0x8F) via control transfer ===");
    let frame = monsgeek_protocol::build_command(0x8F, &[], monsgeek_protocol::ChecksumType::Bit7);
    let written = handle
        .write_control(
            REQUEST_TYPE_OUT,
            HID_SET_REPORT,
            FEATURE_REPORT_WVALUE,
            IF2 as u16,
            &frame[1..],
            USB_TIMEOUT,
        )
        .expect("SET_REPORT failed (Pipe error = STALL not cleared)");
    println!("SET_REPORT wrote {} bytes", written);

    std::thread::sleep(Duration::from_millis(100));

    let mut buf = [0u8; 64];
    let read = handle
        .read_control(
            REQUEST_TYPE_IN,
            HID_GET_REPORT,
            FEATURE_REPORT_WVALUE,
            IF2 as u16,
            &mut buf,
            USB_TIMEOUT,
        )
        .expect("GET_REPORT failed (Pipe error = STALL not cleared)");
    println!("GET_REPORT read {} bytes", read);
    println!("Response: {:02X?}", &buf[..16]);

    assert_eq!(
        buf[0], 0x8F,
        "Echo byte mismatch: expected 0x8F, got 0x{:02X}",
        buf[0]
    );

    let usb_version =
        UsbVersionInfo::parse(&buf).expect("failed to parse GET_USB_VERSION response");
    println!(
        "\nDevice ID: {} (0x{:08X}), firmware version: 0x{:04X}",
        usb_version.device_id, usb_version.device_id, usb_version.firmware_version
    );
    println!("\n=== PASS: reset-then-reopen clears STALL, IF2 control transfers work ===");

    handle.release_interface(IF2).ok();
}
