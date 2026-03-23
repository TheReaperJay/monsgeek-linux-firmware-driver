#![cfg(feature = "hardware")]
//! Debug test: does libusb hotplug detect device arrival at all?
//!
//! Removes the VID filter to see if ANY arrival events fire after replug.
//!
//! Run with:
//! ```sh
//! cargo test -p monsgeek-transport --features hardware --test hotplug_debug -- --nocapture
//! ```

use std::time::{Duration, Instant};

use rusb::UsbContext;

struct DebugWatcher;

impl<T: UsbContext> rusb::Hotplug<T> for DebugWatcher {
    fn device_arrived(&mut self, device: rusb::Device<T>) {
        let desc = device.device_descriptor().ok();
        let (vid, pid) = desc
            .map(|d| (d.vendor_id(), d.product_id()))
            .unwrap_or((0, 0));
        println!(
            "  ARRIVED: VID 0x{:04X} PID 0x{:04X} bus {} addr {}",
            vid,
            pid,
            device.bus_number(),
            device.address()
        );
    }

    fn device_left(&mut self, device: rusb::Device<T>) {
        println!(
            "  LEFT:    bus {} addr {}",
            device.bus_number(),
            device.address()
        );
    }
}

#[test]
fn test_libusb_hotplug_raw() {
    println!("\nRegistering libusb hotplug with NO VID filter (all devices)...");

    let context = rusb::Context::new().expect("failed to create USB context");

    let _reg: rusb::Registration<rusb::Context> = rusb::HotplugBuilder::new()
        .enumerate(false) // Don't fire for already-connected devices
        .register(&context, Box::new(DebugWatcher))
        .expect("failed to register hotplug");

    println!("Waiting 30 seconds — unplug and replug the M5W keyboard.");
    println!("If libusb hotplug works, you'll see ARRIVED/LEFT lines.\n");

    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        context.handle_events(Some(Duration::from_millis(500))).ok();
    }

    println!("\nDone.");
}
