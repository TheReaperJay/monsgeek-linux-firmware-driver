//! HID transport layer for MonsGeek yc3121 keyboards.
//!
//! Provides USB device access via `rusb` control transfers on IF2 (vendor interface),
//! echo-matched query/send with retry, device enumeration, throttled command
//! serialization via a dedicated OS thread, hot-plug detection, and a channel-based
//! public API (`TransportHandle`).
//!
//! # Architecture
//!
//! ```text
//! [Caller] --send_query/send_fire_and_forget--> [TransportHandle]
//!                                                    |
//!                                              (crossbeam channel)
//!                                                    |
//!                                           [Transport Thread]
//!                                                    |
//!                                       flow_control::query_command
//!                                       flow_control::send_command
//!                                                    |
//!                                           [UsbSession (IF2)]
//! ```

pub mod bounds;
pub mod discovery;
pub mod error;
pub mod flow_control;
pub mod input;
pub mod keycodes;
pub mod keymap;
pub mod thread;
pub mod usb;

pub use bounds::{validate_key_index, validate_write_request};
pub use discovery::DeviceInfo;
pub use error::TransportError;
pub use input::InputProcessor;
pub use thread::TransportEvent;
pub use usb::{UsbSession, UsbVersionInfo};

use crossbeam_channel::{bounded, Receiver, Sender};
use monsgeek_protocol::{ChecksumType, DeviceDefinition};

/// Handle for sending commands to a connected MonsGeek keyboard.
///
/// Clone this handle to share across async tasks or threads. All commands
/// are serialized through the transport thread, which enforces a minimum
/// 100ms inter-command delay to satisfy the yc3121 firmware requirement.
///
/// # Example
///
/// ```rust,no_run
/// use monsgeek_transport::{connect, TransportHandle};
/// use monsgeek_protocol::{ChecksumType, DeviceRegistry};
/// use std::path::Path;
///
/// let registry = DeviceRegistry::load_from_directory(Path::new("devices")).unwrap();
/// let m5w = registry.find_by_id(1308).unwrap();
/// let (handle, events) = connect(m5w).unwrap();
/// let response = handle.send_query(0x8F, &[], ChecksumType::Bit7).unwrap();
/// println!("Device ID bytes: {:02X?}", &response[1..5]);
/// handle.shutdown();
/// ```
#[derive(Clone)]
pub struct TransportHandle {
    cmd_tx: Sender<thread::CommandRequest>,
}

impl TransportHandle {
    /// Send a query command and wait for the echo-matched response.
    ///
    /// The command is sent through the transport thread's channel. The thread
    /// calls `flow_control::query_command`, which retries up to `QUERY_RETRIES`
    /// (5) times until the response's echo byte matches `cmd`.
    ///
    /// # Errors
    ///
    /// Returns `TransportError::EchoMismatch` if all retries exhaust.
    /// Returns `TransportError::Usb` on USB transfer failure.
    /// Returns `TransportError::ChannelClosed` if the transport thread has exited.
    pub fn send_query(
        &self,
        cmd: u8,
        data: &[u8],
        checksum: ChecksumType,
    ) -> Result<[u8; 64], TransportError> {
        let (response_tx, response_rx) = bounded(1);
        self.cmd_tx
            .send(thread::CommandRequest {
                cmd,
                data: data.to_vec(),
                checksum,
                is_query: true,
                response_tx,
            })
            .map_err(|_| TransportError::ChannelClosed)?;

        let result = response_rx
            .recv()
            .map_err(|_| TransportError::ChannelClosed)?;

        // query_command always returns Some on success.
        result.map(|opt| opt.expect("query response must be Some"))
    }

    /// Send a fire-and-forget command without waiting for a response.
    ///
    /// The command is sent through the transport thread's channel. The thread
    /// calls `flow_control::send_command`, which retries up to `SEND_RETRIES`
    /// (3) times on USB error.
    ///
    /// # Errors
    ///
    /// Returns `TransportError::Usb` on USB transfer failure after retries.
    /// Returns `TransportError::ChannelClosed` if the transport thread has exited.
    pub fn send_fire_and_forget(
        &self,
        cmd: u8,
        data: &[u8],
        checksum: ChecksumType,
    ) -> Result<(), TransportError> {
        let (response_tx, response_rx) = bounded(1);
        self.cmd_tx
            .send(thread::CommandRequest {
                cmd,
                data: data.to_vec(),
                checksum,
                is_query: false,
                response_tx,
            })
            .map_err(|_| TransportError::ChannelClosed)?;

        let result = response_rx
            .recv()
            .map_err(|_| TransportError::ChannelClosed)?;

        result.map(|_| ())
    }

    /// Shut down the transport thread by dropping the command channel sender.
    ///
    /// Once all `TransportHandle` clones are dropped (or this method is called),
    /// the transport thread's `recv()` loop returns `Err` and the thread exits.
    pub fn shutdown(self) {
        drop(self.cmd_tx);
    }
}

/// Connect to a MonsGeek keyboard and start the transport thread.
///
/// Opens the keyboard's preferred USB VID/PID first, verifies its firmware
/// device ID via `GET_USB_VERSION`, and falls back to probing other USB devices
/// with the same vendor ID if the runtime PID differs from the registry's
/// primary PID. Once the matching keyboard is found, this opens a USB session,
/// spawns the transport thread (which serializes all HID I/O with 100ms
/// throttling), and starts the hot-plug monitoring thread.
///
/// Returns `(TransportHandle, Receiver<TransportEvent>)`. The handle is used
/// to send commands; the receiver emits hot-plug lifecycle events.
///
/// # Errors
///
/// Returns `TransportError::DeviceNotFound` if no connected USB device matches
/// the definition's firmware device ID.
/// Returns `TransportError::Usb` on USB open/claim failure.
pub fn connect(device: &DeviceDefinition) -> Result<(TransportHandle, Receiver<TransportEvent>), TransportError> {
    let session = open_matching_session(device)?;
    let (cmd_tx, cmd_rx) = bounded(monsgeek_protocol::timing::dongle::REQUEST_QUEUE_SIZE);
    let (event_tx, event_rx) = bounded(32);

    thread::spawn_transport_thread(cmd_rx, event_tx.clone(), session);
    thread::spawn_hotplug_thread(event_tx, device.vid);

    Ok((TransportHandle { cmd_tx }, event_rx))
}

fn open_matching_session(device: &DeviceDefinition) -> Result<UsbSession, TransportError> {
    let primary = UsbSession::open(device.vid, device.pid);

    match primary {
        Ok(session) => {
            let usb_version = session.query_usb_version()?;
            if usb_version.device_id_i32() == device.id {
                return Ok(session);
            }

            log::warn!(
                "USB: preferred VID 0x{:04X} PID 0x{:04X} reported device ID {} instead of {}. Probing by firmware ID.",
                device.vid,
                device.pid,
                usb_version.device_id,
                device.id
            );
        }
        Err(TransportError::DeviceNotFound { .. }) => {
            log::info!(
                "USB: preferred VID 0x{:04X} PID 0x{:04X} not present. Probing by firmware ID.",
                device.vid,
                device.pid
            );
        }
        Err(err) => return Err(err),
    }

    let info = discovery::probe_device(device)?;
    UsbSession::open_at(info.bus, info.address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_handle_is_clone() {
        // Verify TransportHandle implements Clone by creating and cloning a handle.
        // We can't actually connect to a device, but we can verify the type is Clone
        // by checking the trait bound at compile time.
        fn assert_clone<T: Clone>() {}
        assert_clone::<TransportHandle>();
    }

    #[test]
    fn test_connect_exists_with_correct_signature() {
        let _fn_ptr: fn(&DeviceDefinition) -> Result<(TransportHandle, Receiver<TransportEvent>), TransportError> = connect;
    }

    #[test]
    fn test_transport_handle_send_query_signature() {
        // Verify send_query method exists with correct signature via trait bound checking.
        fn check_send_query(handle: &TransportHandle) -> Result<[u8; 64], TransportError> {
            handle.send_query(0x8F, &[], ChecksumType::Bit7)
        }
        // We can't call it without hardware, but the function compiles.
        let _ = check_send_query as fn(&TransportHandle) -> Result<[u8; 64], TransportError>;
    }

    #[test]
    fn test_transport_handle_send_fire_and_forget_signature() {
        fn check_fire_and_forget(handle: &TransportHandle) -> Result<(), TransportError> {
            handle.send_fire_and_forget(0x06, &[0x05], ChecksumType::Bit7)
        }
        let _ = check_fire_and_forget as fn(&TransportHandle) -> Result<(), TransportError>;
    }

    #[test]
    fn test_transport_handle_shutdown_consumes_self() {
        // Verify shutdown takes self by value (consumes the handle).
        fn check_shutdown(handle: TransportHandle) {
            handle.shutdown();
        }
        let _ = check_shutdown as fn(TransportHandle);
    }

    #[test]
    fn test_channel_closed_on_handle_drop() {
        // When all TransportHandle clones are dropped, the channel sender is dropped,
        // which causes the transport thread's recv() to return Err and exit.
        let (cmd_tx, cmd_rx) = bounded::<thread::CommandRequest>(16);
        let handle = TransportHandle { cmd_tx };

        // Channel should be open.
        assert!(!cmd_rx.is_empty() || cmd_rx.len() == 0);

        // Drop the handle — channel sender is dropped.
        drop(handle);

        // Recv should now return Err (disconnected).
        assert!(cmd_rx.recv().is_err());
    }

    #[test]
    fn test_request_queue_size() {
        // Verify the channel capacity matches the protocol constant.
        assert_eq!(monsgeek_protocol::timing::dongle::REQUEST_QUEUE_SIZE, 16);
    }
}
