//! Dedicated transport thread for serialized HID I/O with throttling and hot-plug.
//!
//! The transport thread owns the `UsbSession` and processes `CommandRequest`s
//! from a bounded channel. Timing and retry policy are enforced by the
//! centralized command controller.
//! A separate hot-plug thread monitors USB bus events for MonsGeek device
//! arrivals and departures.

use std::os::unix::io::AsRawFd;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};

use monsgeek_protocol::ChecksumType;

use crate::controller::CommandController;
use crate::error::TransportError;
use crate::input::{InputProcessor, KeyAction};
use crate::usb::UsbSession;

const INPUT_POLL_TIMEOUT_MS: u64 = 10;

/// Request sent from `TransportHandle` to the transport thread via bounded channel.
pub(crate) struct CommandRequest {
    /// FEA command byte.
    pub cmd: u8,
    /// Command data payload (variable length, will be padded by `build_command`).
    pub data: Vec<u8>,
    /// Checksum type for frame construction.
    pub checksum: ChecksumType,
    /// Command operation mode.
    pub mode: CommandMode,
    /// One-shot channel to send the result back to the caller.
    pub response_tx: Sender<Result<Option<[u8; 64]>, TransportError>>,
}

/// Command operation type used by the transport thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandMode {
    /// Send command and read echo-matched response.
    Query,
    /// Send command without reading a response.
    Send,
    /// Read one pending feature report response.
    Read,
}

/// Events emitted by the transport layer for device lifecycle and optional
/// userspace-input notifications.
///
/// Consumers receive these via the `Receiver<TransportEvent>` returned by
/// `connect()` or `connect_with_options()`.
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
    DeviceLeft { bus: u8, address: u8 },
    /// Translated key actions from IF0 when userspace-input mode is active.
    InputActions { actions: Vec<KeyAction> },
}

/// Spawn the transport thread that processes commands serially.
///
/// The thread runs a recv loop on `cmd_rx`; each command is delegated to the
/// internal command controller, which performs sanitization and timing
/// enforcement. When the sender side of `cmd_rx` is dropped, the thread exits
/// cleanly.
///
/// # Arguments
///
/// * `cmd_rx` - Receiver end of the command channel
/// * `event_tx` - Sender for transport lifecycle events (currently unused by this thread,
///   reserved for future device disconnect detection)
/// * `session` - The USB session to use for all HID I/O
pub(crate) fn spawn_transport_thread(
    cmd_rx: Receiver<CommandRequest>,
    event_tx: Sender<TransportEvent>,
    session: UsbSession,
    input_processor: Option<InputProcessor>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("monsgeek-transport".into())
        .spawn(move || {
            transport_loop(cmd_rx, event_tx, session, input_processor);
        })
        .expect("failed to spawn transport thread")
}

/// Main loop for the transport thread.
///
/// Processes commands serially through the centralized command controller.
fn transport_loop(
    cmd_rx: Receiver<CommandRequest>,
    event_tx: Sender<TransportEvent>,
    session: UsbSession,
    mut input_processor: Option<InputProcessor>,
) {
    let mut controller = CommandController::new(session);

    loop {
        let request = if input_processor.is_some() {
            match cmd_rx.recv_timeout(Duration::from_millis(INPUT_POLL_TIMEOUT_MS)) {
                Ok(req) => Some(req),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    pump_input(controller.session(), input_processor.as_mut(), &event_tx);
                    None
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match cmd_rx.recv() {
                Ok(req) => Some(req),
                Err(_) => break,
            }
        };

        let Some(req) = request else {
            continue;
        };

        handle_command(req, &mut controller);
    }

    log::info!("Transport thread shutting down");
}

fn handle_command(req: CommandRequest, controller: &mut CommandController) {
    let result = match req.mode {
        CommandMode::Query => controller.query(req.cmd, &req.data, req.checksum).map(Some),
        CommandMode::Send => controller
            .send(req.cmd, &req.data, req.checksum)
            .map(|()| None),
        CommandMode::Read => controller.read_feature_report().map(Some),
    };

    // Send result back; ignore error (caller may have dropped their receiver).
    let _ = req.response_tx.send(result);
}

fn pump_input(
    session: &UsbSession,
    input_processor: Option<&mut InputProcessor>,
    event_tx: &Sender<TransportEvent>,
) {
    let Some(processor) = input_processor else {
        return;
    };

    let mut report = [0u8; 8];
    match session
        .read_report_with_timeout(&mut report, Duration::from_millis(INPUT_POLL_TIMEOUT_MS))
    {
        Ok(n) if n >= 8 => {
            let actions = processor.process_report(&report);
            if !actions.is_empty() {
                let _ = event_tx.send(TransportEvent::InputActions { actions });
            }
        }
        Ok(_) => {}
        Err(TransportError::Disconnected) => {
            let actions = processor.release_all_keys();
            if !actions.is_empty() {
                let _ = event_tx.send(TransportEvent::InputActions { actions });
            }
        }
        Err(err) => {
            log::debug!("Userspace input poll failed: {}", err);
        }
    }
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
                        log::info!(
                            "Hot-plug: device arrived VID 0x{} PID 0x{:04X} bus {} addr {}",
                            vid_str,
                            pid,
                            bus,
                            addr
                        );
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
                        let _ = event_tx.send(TransportEvent::DeviceLeft { bus, address: addr });
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
            mode: CommandMode::Query,
            response_tx: tx,
        };
        assert_eq!(req.cmd, 0x8F);
        assert_eq!(req.data, vec![0x01, 0x02]);
        assert_eq!(req.mode, CommandMode::Query);
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
        let event = TransportEvent::DeviceLeft { bus: 1, address: 5 };
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
    fn test_transport_event_input_actions() {
        let event = TransportEvent::InputActions {
            actions: vec![KeyAction {
                keycode: 30,
                value: 1,
            }],
        };
        match event {
            TransportEvent::InputActions { actions } => {
                assert_eq!(actions.len(), 1);
                assert_eq!(actions[0].keycode, 30);
                assert_eq!(actions[0].value, 1);
            }
            _ => panic!("expected InputActions"),
        }
    }

    #[test]
    fn test_spawn_transport_thread_exists_with_correct_signature() {
        let _fn_ptr: fn(
            crossbeam_channel::Receiver<CommandRequest>,
            crossbeam_channel::Sender<TransportEvent>,
            crate::usb::UsbSession,
            Option<InputProcessor>,
        ) -> std::thread::JoinHandle<()> = spawn_transport_thread;
    }
}
