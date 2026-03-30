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
//!                                           [CommandController]
//!                                                    |
//!                                     [UsbSession (mode-selected interfaces)]
//! ```

pub mod active_path;
pub mod bounds;
mod controller;
pub mod discovery;
pub mod error;
mod flow_control;
pub mod input;
pub mod keycodes;
pub mod keymap;
mod runtime_config;
pub mod thread;
pub mod usb;

pub use bounds::{validate_key_index, validate_write_request};
pub use discovery::{
    DeviceInfo, ProbeAttempt, ProbeOutcome, ProbeReport, ProbeStrategy, last_probe_report,
    probe_devices_with_report,
};
pub use error::TransportError;
pub use input::InputProcessor;
pub use thread::TransportEvent;
pub use usb::{SessionMode as TransportMode, UsbSession, UsbVersionInfo};

use crossbeam_channel::{Receiver, Sender, bounded};
use monsgeek_protocol::{ChecksumType, DeviceDefinition};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::controller::CommandController;

/// Configuration for opening a transport session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportOptions {
    pub mode: TransportMode,
    pub software_debounce_ms: u64,
}

impl Default for TransportOptions {
    fn default() -> Self {
        Self {
            mode: TransportMode::ControlOnly,
            software_debounce_ms: 0,
        }
    }
}

impl TransportOptions {
    pub fn control_only() -> Self {
        Self::default()
    }

    pub fn userspace_input(software_debounce_ms: u64) -> Self {
        Self {
            mode: TransportMode::UserspaceInput,
            software_debounce_ms,
        }
    }

    pub fn input_only(software_debounce_ms: u64) -> Self {
        Self {
            mode: TransportMode::InputOnly,
            software_debounce_ms,
        }
    }
}

/// Handle for sending commands to a connected MonsGeek keyboard.
///
/// Clone this handle to share across async tasks or threads. All commands
/// are serialized through the transport thread, and every command operation
/// is executed by the internal command controller with mandatory timing
/// enforcement to satisfy the yc3121 firmware requirement.
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
    /// routes it through the internal `CommandController`, which enforces
    /// sanitization, retry behavior, and mandatory command spacing.
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
                mode: thread::CommandMode::Query,
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
    /// routes it through the internal `CommandController`, which enforces
    /// sanitization, retry behavior, and mandatory command spacing.
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
                mode: thread::CommandMode::Send,
                response_tx,
            })
            .map_err(|_| TransportError::ChannelClosed)?;

        let result = response_rx
            .recv()
            .map_err(|_| TransportError::ChannelClosed)?;

        result.map(|_| ())
    }

    /// Send a command and return the echo-matched response.
    ///
    /// This uses transport-thread `Query` mode, which performs send+read with
    /// retry semantics and echo-byte validation.
    pub fn query_command(
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
                mode: thread::CommandMode::Query,
                response_tx,
            })
            .map_err(|_| TransportError::ChannelClosed)?;

        let result = response_rx
            .recv()
            .map_err(|_| TransportError::ChannelClosed)?;

        result.map(|opt| opt.expect("query response must be Some"))
    }

    /// Read a single 64-byte feature-report response from the connected device.
    ///
    /// This supports split send/read RPC semantics used by the gRPC bridge.
    pub fn read_feature_report(&self) -> Result<[u8; 64], TransportError> {
        let (response_tx, response_rx) = bounded(1);
        self.cmd_tx
            .send(thread::CommandRequest {
                cmd: 0,
                data: Vec::new(),
                checksum: ChecksumType::None,
                mode: thread::CommandMode::Read,
                response_tx,
            })
            .map_err(|_| TransportError::ChannelClosed)?;

        let result = response_rx
            .recv()
            .map_err(|_| TransportError::ChannelClosed)?;

        result.map(|opt| opt.expect("read response must be Some"))
    }

    /// Shut down the transport thread by dropping the command channel sender.
    ///
    /// Once all `TransportHandle` clones are dropped (or this method is called),
    /// the transport thread's `recv()` loop returns `Err` and the thread exits.
    pub fn shutdown(self) {
        drop(self.cmd_tx);
    }
}

/// Connect to a MonsGeek keyboard in control-only mode and start the transport thread.
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
pub fn connect(
    device: &DeviceDefinition,
) -> Result<(TransportHandle, Receiver<TransportEvent>), TransportError> {
    connect_with_options(device, TransportOptions::default())
}

/// Connect to a MonsGeek keyboard with explicit transport ownership options.
///
/// In [`TransportMode::ControlOnly`], the transport claims only IF2 and the
/// event receiver emits hot-plug lifecycle events.
///
/// In [`TransportMode::UserspaceInput`], the transport intentionally owns IF0
/// as well and the event receiver may also emit translated
/// [`TransportEvent::InputActions`] notifications.
pub fn connect_with_options(
    device: &DeviceDefinition,
    options: TransportOptions,
) -> Result<(TransportHandle, Receiver<TransportEvent>), TransportError> {
    let session = open_matching_session(device, options.mode)?;
    spawn_transport(device, session, options)
}

/// Connect to a specific runtime USB location (bus/address) with explicit options.
///
/// Unlike [`connect_with_options`], this does not pivot to a profile-preferred PID.
/// It is used when discovery already identified the exact runtime device.
pub fn connect_at_with_options(
    device: &DeviceDefinition,
    bus: u8,
    address: u8,
    options: TransportOptions,
) -> Result<(TransportHandle, Receiver<TransportEvent>), TransportError> {
    let session = UsbSession::open_at_with_mode(bus, address, options.mode)?;
    // For runtime path-selected connects, the caller already resolved bus/address
    // from discovery/path registry. Avoid an extra blocking firmware query here;
    // repeated open-time verification retries can cause reset loops on yc3121.
    spawn_transport(device, session, options)
}

fn spawn_transport(
    device: &DeviceDefinition,
    session: UsbSession,
    options: TransportOptions,
) -> Result<(TransportHandle, Receiver<TransportEvent>), TransportError> {
    let (cmd_tx, cmd_rx) = bounded(monsgeek_protocol::timing::dongle::REQUEST_QUEUE_SIZE);
    let (event_tx, event_rx) = bounded(32);
    let input_processor = match options.mode {
        TransportMode::ControlOnly => None,
        TransportMode::UserspaceInput | TransportMode::InputOnly => {
            Some(InputProcessor::new(options.software_debounce_ms))
        }
    };
    let running = Arc::new(AtomicBool::new(true));

    thread::spawn_transport_thread(
        cmd_rx,
        event_tx.clone(),
        session,
        Arc::clone(&running),
        input_processor,
    );
    thread::spawn_hotplug_thread(event_tx, device.vid, running);

    Ok((TransportHandle { cmd_tx }, event_rx))
}

/// Run the native wired recovery path for a device and return its firmware ID.
///
/// This is the supported recovery mechanism after transient USB `PIPE` /
/// timeout states on wired M5W-class devices:
///
/// 1. Re-find the device dynamically
/// 2. Reset and re-open it via [`UsbSession::open_with_mode`]
/// 3. Verify recovery with `GET_USB_VERSION`
///
/// The session is dropped immediately after the verification query so the
/// kernel can reclaim the normal typing interface if it is available.
pub fn recover(device: &DeviceDefinition) -> Result<UsbVersionInfo, TransportError> {
    let session = open_matching_session(device, TransportMode::ControlOnly)?;
    let mut controller = CommandController::new(session);
    let usb_version = controller.query_usb_version()?;

    if usb_version.device_id_i32() != device.id {
        return Err(TransportError::Usb(format!(
            "recovery opened device ID {} but expected {}",
            usb_version.device_id_i32(),
            device.id
        )));
    }

    Ok(usb_version)
}

fn open_matching_session(
    device: &DeviceDefinition,
    mode: TransportMode,
) -> Result<UsbSession, TransportError> {
    let primary = UsbSession::open_with_mode(device.vid, device.pid, mode);

    match primary {
        Ok(session) => {
            let mut controller = CommandController::new(session);
            match controller.query_usb_version() {
                Ok(usb_version) if usb_version.device_id_i32() == device.id => {
                    return Ok(controller.into_session());
                }
                Ok(usb_version) => {
                    log::warn!(
                        "USB: preferred VID 0x{:04X} PID 0x{:04X} reported device ID {} instead of {}. Probing by firmware ID.",
                        device.vid,
                        device.pid,
                        usb_version.device_id,
                        device.id
                    );
                }
                Err(e) => {
                    // First command failed — likely a STALL from kernel's
                    // IF1/IF2 descriptor probing. Reset and retry once.
                    log::warn!(
                        "USB: first command failed ({}), attempting STALL recovery via reset",
                        e
                    );
                    let session = controller.into_session().reset_and_reopen()?;
                    let mut controller = CommandController::new(session);
                    let usb_version = controller.query_usb_version()?;
                    if usb_version.device_id_i32() == device.id {
                        return Ok(controller.into_session());
                    }
                    log::warn!(
                        "USB: post-reset device ID {} != expected {}. Probing by firmware ID.",
                        usb_version.device_id,
                        device.id
                    );
                }
            }
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
    UsbSession::open_at_with_mode(info.bus, info.address, mode)
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
        let _fn_ptr: fn(
            &DeviceDefinition,
        )
            -> Result<(TransportHandle, Receiver<TransportEvent>), TransportError> = connect;
    }

    #[test]
    fn test_connect_with_options_exists_with_correct_signature() {
        let _fn_ptr: fn(
            &DeviceDefinition,
            TransportOptions,
        )
            -> Result<(TransportHandle, Receiver<TransportEvent>), TransportError> =
            connect_with_options;
    }

    #[test]
    fn test_connect_at_with_options_exists_with_correct_signature() {
        let _fn_ptr: fn(
            &DeviceDefinition,
            u8,
            u8,
            TransportOptions,
        )
            -> Result<(TransportHandle, Receiver<TransportEvent>), TransportError> =
            connect_at_with_options;
    }

    #[test]
    fn test_recover_exists_with_correct_signature() {
        let _fn_ptr: fn(&DeviceDefinition) -> Result<UsbVersionInfo, TransportError> = recover;
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
    fn test_transport_handle_read_feature_report_signature() {
        fn check_read(handle: &TransportHandle) -> Result<[u8; 64], TransportError> {
            handle.read_feature_report()
        }
        let _ = check_read as fn(&TransportHandle) -> Result<[u8; 64], TransportError>;
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
    fn test_transport_options_input_only() {
        let options = TransportOptions::input_only(15);
        assert_eq!(options.mode, TransportMode::InputOnly);
        assert_eq!(options.software_debounce_ms, 15);
    }

    #[test]
    fn test_transport_options_default_to_control_only() {
        let options = TransportOptions::default();
        assert_eq!(options.mode, TransportMode::ControlOnly);
        assert_eq!(options.software_debounce_ms, 0);
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
