#![cfg(feature = "hardware")]
//! Debug test: log ALL udev usb events with all properties.

use std::os::unix::io::AsRawFd;
use std::time::{Duration, Instant};

#[test]
fn test_udev_events_raw() {
    let builder = udev::MonitorBuilder::new().unwrap();
    let builder = builder.match_subsystem("usb").unwrap();
    let socket = builder.listen().unwrap();
    let fd = socket.as_raw_fd();

    println!("\nMonitoring ALL udev usb events for 5 minutes.");
    println!("Unplug and replug the M5W keyboard.\n");

    let deadline = Instant::now() + Duration::from_secs(300);
    while Instant::now() < deadline {
        let mut fds = [libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        }];
        let ret = unsafe { libc::poll(fds.as_mut_ptr(), 1, 1000) };
        if ret <= 0 {
            continue;
        }

        if let Some(event) = socket.iter().next() {
            println!(
                "--- {:?} subsystem={:?} devtype={:?} ---",
                event.event_type(),
                event.subsystem().map(|s| s.to_string_lossy().to_string()),
                event.devtype().map(|s| s.to_string_lossy().to_string()),
            );
            for prop in event.properties() {
                println!(
                    "  {}={}",
                    prop.name().to_string_lossy(),
                    prop.value().to_string_lossy()
                );
            }
            println!();
        }
    }
    println!("Done.");
}
