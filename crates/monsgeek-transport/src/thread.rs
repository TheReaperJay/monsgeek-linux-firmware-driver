//! Dedicated transport thread for serialized HID I/O with throttling and hot-plug.
//!
//! The transport thread owns the `UsbSession` and processes `CommandRequest`s
//! from a bounded channel, enforcing a minimum 100ms inter-command delay.
//! A separate hot-plug thread monitors USB bus events for MonsGeek device
//! arrivals and departures.

use std::os::unix::io::AsRawFd;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use monsgeek_protocol::timing;
use monsgeek_protocol::ChecksumType;

use crate::error::TransportError;
use crate::flow_control;
use crate::usb::UsbSession;

/// Request sent from `TransportHandle` to the transport thread via bounded channel.
pub(crate) struct CommandRequest {
    /// FEA command byte.
    pub cmd: u8,
    /// Command data payload (variable length, will be padded by `build_command`).
    pub data: Vec<u8>,
    /// Checksum type for frame construction.
    pub checksum: ChecksumType,
    /// If true, perform echo-matched query (send + read). If false, fire-and-forget (send only).
    pub is_query: bool,
    /// One-shot channel to send the result back to the caller.
    pub response_tx: Sender<Result<Option<[u8; 64]>, TransportError>>,
}

/// Events emitted by the transport thread for device lifecycle notifications.
///
/// Consumers receive these via the `Receiver<TransportEvent>` returned by `connect()`.
#[derive(Debug, Clone)]
pub enum TransportEvent {
    /// A MonsGeek device was connected to the USB bus.
    DeviceArrived {
        vid: u16,
        pid: u16,
        bus: u8,
        address: u8,
    },
    /// A USB device was disconnected from the bus.
    DeviceLeft {
        bus: u8,
        address: u8,
    },
}

/// Spawn the transport thread that processes commands serially with throttling.
///
/// The thread runs a recv loop on `cmd_rx`, enforcing a minimum 100ms delay
/// between consecutive commands (tracked via `Instant`). When the sender side
/// of `cmd_rx` is dropped, the thread exits cleanly.
///
/// # Arguments
///
/// * `cmd_rx` - Receiver end of the command channel
/// * `event_tx` - Sender for transport lifecycle events (currently unused by this thread,
///   reserved for future device disconnect detection)
/// * `session` - The USB session to use for all HID I/O
pub(crate) fn spawn_transport_thread(
    cmd_rx: Receiver<CommandRequest>,
    _event_tx: Sender<TransportEvent>,
    session: UsbSession,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("monsgeek-transport".into())
        .spawn(move || {
            transport_loop(cmd_rx, session);
        })
        .expect("failed to spawn transport thread")
}

/// Main loop for the transport thread.
///
/// Processes commands serially, enforcing minimum inter-command delay.
fn transport_loop(cmd_rx: Receiver<CommandRequest>, session: UsbSession) {
    // Initialize to past time so the first command executes immediately.
    let mut last_command = Instant::now() - Duration::from_millis(200);

    while let Ok(req) = cmd_rx.recv() {
        // Enforce minimum inter-command delay (100ms for yc3121 firmware safety).
        let elapsed = last_command.elapsed();
        let min_delay = Duration::from_millis(timing::DEFAULT_DELAY_MS);
        if elapsed < min_delay {
            std::thread::sleep(min_delay - elapsed);
        }

        let result = if req.is_query {
            flow_control::query_command(&session, req.cmd, &req.data, req.checksum)
                .map(Some)
        } else {
            flow_control::send_command(&session, req.cmd, &req.data, req.checksum)
                .map(|()| None)
        };

        last_command = Instant::now();

        // Send result back; ignore error (caller may have dropped their receiver).
        let _ = req.response_tx.send(result);
    }

    log::info!("Transport thread shutting down");
}

/// Spawn a dedicated thread for USB hot-plug event monitoring via udev.
///
/// Monitors the `usb` subsystem for add/remove events matching the given VID.
/// Uses udev (not libusb hotplug) because libusb hotplug does not reliably
/// fire arrival events on Linux (departure only — verified empirically).
///
/// The thread uses `poll()` with a 1-second timeout so it doesn't spin.
pub(crate) fn spawn_hotplug_thread(
    event_tx: Sender<TransportEvent>,
    vid: u16,
) -> Option<std::thread::JoinHandle<()>> {
    let handle = std::thread::Builder::new()
        .name("monsgeek-hotplug".into())
        .spawn(move || {
            let builder = match udev::MonitorBuilder::new() {
                Ok(b) => b,
                Err(e) => {
                    log::error!("Failed to create udev monitor: {}", e);
                    return;
                }
            };

            let builder = match builder.match_subsystem("usb") {
                Ok(b) => b,
                Err(e) => {
                    log::error!("Failed to set udev subsystem filter: {}", e);
                    return;
                }
            };

            let socket = match builder.listen() {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Failed to start udev monitor: {}", e);
                    return;
                }
            };

            let fd = socket.as_raw_fd();
            let vid_str = format!("{:04x}", vid);

            log::info!("Hot-plug monitoring started via udev for VID 0x{}", vid_str);

            loop {
                let mut fds = [libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                }];

                let ret = unsafe { libc::poll(fds.as_mut_ptr(), 1, 1000) };
                if ret <= 0 {
                    continue;
                }

                let Some(event) = socket.iter().next() else {
                    continue;
                };

                // Filter to our VID by checking the PRODUCT property
                // (format: "VID/PID/bcdDevice", e.g. "3151/4015/103")
                let matches_vid = event
                    .property_value("PRODUCT")
                    .and_then(|v| v.to_str())
                    .map(|v| v.starts_with(&vid_str))
                    .unwrap_or(false);

                if !matches_vid {
                    continue;
                }

                match event.event_type() {
                    udev::EventType::Add => {
                        let (pid, bus, addr) = extract_device_info(&event, &vid_str);
                        log::info!("Hot-plug: device arrived VID 0x{} PID 0x{:04X} bus {} addr {}", vid_str, pid, bus, addr);
                        let _ = event_tx.send(TransportEvent::DeviceArrived {
                            vid,
                            pid,
                            bus,
                            address: addr,
                        });
                    }
                    udev::EventType::Remove => {
                        let (_, bus, addr) = extract_device_info(&event, &vid_str);
                        log::info!("Hot-plug: device removed bus {} addr {}", bus, addr);
                        let _ = event_tx.send(TransportEvent::DeviceLeft {
                            bus,
                            address: addr,
                        });
                    }
                    _ => {}
                }
            }
        })
        .expect("failed to spawn hot-plug thread");

    Some(handle)
}

/// Extract PID, bus, and address from a udev event.
fn extract_device_info(event: &udev::Event, _vid_str: &str) -> (u16, u8, u8) {
    let pid = event
        .property_value("PRODUCT")
        .and_then(|v| v.to_str())
        .and_then(|v| {
            let parts: Vec<&str> = v.split('/').collect();
            if parts.len() >= 2 {
                u16::from_str_radix(parts[1], 16).ok()
            } else {
                None
            }
        })
        .unwrap_or(0);

    let busnum = event
        .property_value("BUSNUM")
        .and_then(|v| v.to_str())
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(0);

    let devnum = event
        .property_value("DEVNUM")
        .and_then(|v| v.to_str())
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(0);

    (pid, busnum, devnum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_request_fields() {
        let (tx, _rx) = crossbeam_channel::bounded(1);
        let req = CommandRequest {
            cmd: 0x8F,
            data: vec![0x01, 0x02],
            checksum: monsgeek_protocol::ChecksumType::Bit7,
            is_query: true,
            response_tx: tx,
        };
        assert_eq!(req.cmd, 0x8F);
        assert_eq!(req.data, vec![0x01, 0x02]);
        assert!(req.is_query);
    }

    #[test]
    fn test_transport_event_device_arrived() {
        let event = TransportEvent::DeviceArrived {
            vid: 0x3151,
            pid: 0x4015,
            bus: 1,
            address: 5,
        };
        match event {
            TransportEvent::DeviceArrived {
                vid,
                pid,
                bus,
                address,
            } => {
                assert_eq!(vid, 0x3151);
                assert_eq!(pid, 0x4015);
                assert_eq!(bus, 1);
                assert_eq!(address, 5);
            }
            _ => panic!("expected DeviceArrived"),
        }
    }

    #[test]
    fn test_transport_event_device_left() {
        let event = TransportEvent::DeviceLeft {
            bus: 1,
            address: 5,
        };
        match event {
            TransportEvent::DeviceLeft { bus, address } => {
                assert_eq!(bus, 1);
                assert_eq!(address, 5);
            }
            _ => panic!("expected DeviceLeft"),
        }
    }

    #[test]
    fn test_transport_event_is_clone() {
        let event = TransportEvent::DeviceArrived {
            vid: 0x3151,
            pid: 0x4015,
            bus: 1,
            address: 5,
        };
        let cloned = event.clone();
        match cloned {
            TransportEvent::DeviceArrived { vid, .. } => assert_eq!(vid, 0x3151),
            _ => panic!("expected DeviceArrived"),
        }
    }

    #[test]
    fn test_transport_event_is_debug() {
        let event = TransportEvent::DeviceArrived {
            vid: 0x3151,
            pid: 0x4015,
            bus: 1,
            address: 5,
        };
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("DeviceArrived"));
    }

    #[test]
    fn test_spawn_transport_thread_exists_with_correct_signature() {
        let _fn_ptr: fn(
            crossbeam_channel::Receiver<CommandRequest>,
            crossbeam_channel::Sender<TransportEvent>,
            crate::usb::UsbSession,
        ) -> std::thread::JoinHandle<()> = spawn_transport_thread;
    }
}
