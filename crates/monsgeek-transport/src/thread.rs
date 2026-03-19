//! Dedicated transport thread for serialized HID I/O with throttling and hot-plug.
//!
//! The transport thread owns the `UsbSession` and processes `CommandRequest`s
//! from a bounded channel, enforcing a minimum 100ms inter-command delay.
//! A separate hot-plug thread monitors USB bus events for MonsGeek device
//! arrivals and departures.

use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use rusb::UsbContext;

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

/// Hot-plug watcher that sends `TransportEvent`s when MonsGeek devices
/// are plugged in or removed.
///
/// CRITICAL per RESEARCH Pitfall 5: Do NOT call any `DeviceHandle` methods
/// (control transfers, descriptor reads beyond `device_descriptor`) inside
/// these callbacks. Only send notification events via channel.
struct HotplugWatcher {
    event_tx: Sender<TransportEvent>,
}

impl<T: UsbContext> rusb::Hotplug<T> for HotplugWatcher {
    fn device_arrived(&mut self, device: rusb::Device<T>) {
        if let Ok(desc) = device.device_descriptor() {
            if desc.vendor_id() == 0x3141 {
                let _ = self.event_tx.send(TransportEvent::DeviceArrived {
                    vid: desc.vendor_id(),
                    pid: desc.product_id(),
                    bus: device.bus_number(),
                    address: device.address(),
                });
            }
        }
    }

    fn device_left(&mut self, device: rusb::Device<T>) {
        let _ = self.event_tx.send(TransportEvent::DeviceLeft {
            bus: device.bus_number(),
            address: device.address(),
        });
    }
}

/// Spawn a dedicated thread for USB hot-plug event monitoring.
///
/// Returns `Some(JoinHandle)` if the platform supports hot-plug (Linux with
/// libusb >= 1.0.16). Returns `None` if not supported (e.g., macOS without
/// libusb hot-plug support).
///
/// The thread calls `context.handle_events()` in a loop, which dispatches
/// to the `HotplugWatcher` callbacks. It runs until the process exits or
/// the `Registration` is dropped (which happens when the context goes out
/// of scope inside the thread — in practice, this thread lives for the
/// program's lifetime).
pub(crate) fn spawn_hotplug_thread(
    event_tx: Sender<TransportEvent>,
) -> Option<std::thread::JoinHandle<()>> {
    if !rusb::has_hotplug() {
        log::warn!("Hot-plug not supported on this platform");
        return None;
    }

    let handle = std::thread::Builder::new()
        .name("monsgeek-hotplug".into())
        .spawn(move || {
            let context = match rusb::Context::new() {
                Ok(ctx) => ctx,
                Err(e) => {
                    log::error!("Failed to create USB context for hot-plug: {}", e);
                    return;
                }
            };

            let watcher = Box::new(HotplugWatcher { event_tx });
            let _registration: rusb::Registration<rusb::Context> = match rusb::HotplugBuilder::new()
                .vendor_id(0x3141)
                .enumerate(true)
                .register(&context, watcher)
            {
                Ok(reg) => reg,
                Err(e) => {
                    log::error!("Failed to register hot-plug callback: {}", e);
                    return;
                }
            };

            log::info!("Hot-plug monitoring started for VID 0x3141");

            // Event loop — handle_events blocks until an event occurs or timeout.
            // The Registration must stay alive (not dropped) for callbacks to fire.
            loop {
                if let Err(e) = context.handle_events(Some(Duration::from_millis(500))) {
                    log::error!("Hot-plug handle_events error: {}", e);
                    break;
                }
            }
        })
        .expect("failed to spawn hot-plug thread");

    Some(handle)
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
            vid: 0x3141,
            pid: 0x4005,
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
                assert_eq!(vid, 0x3141);
                assert_eq!(pid, 0x4005);
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
            vid: 0x3141,
            pid: 0x4005,
            bus: 1,
            address: 5,
        };
        let cloned = event.clone();
        match cloned {
            TransportEvent::DeviceArrived { vid, .. } => assert_eq!(vid, 0x3141),
            _ => panic!("expected DeviceArrived"),
        }
    }

    #[test]
    fn test_transport_event_is_debug() {
        let event = TransportEvent::DeviceArrived {
            vid: 0x3141,
            pid: 0x4005,
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
