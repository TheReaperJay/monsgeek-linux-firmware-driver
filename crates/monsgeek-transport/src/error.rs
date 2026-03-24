use thiserror::Error;

/// Errors from transport-layer operations (USB I/O, device access, bounds validation).
#[derive(Debug, Error)]
pub enum TransportError {
    /// Generic USB error wrapping a rusb or libusb failure.
    #[error("USB error: {0}")]
    Usb(String),

    /// Command-level timeout: the firmware did not respond to a specific command.
    ///
    /// This variant is NOT constructed by `From<rusb::Error>`. The From impl maps all
    /// rusb errors (including `rusb::Error::Timeout`) uniformly to `Usb(String)`.
    /// `TransportError::Timeout` is constructed exclusively by the flow_control layer,
    /// which knows the command byte and can provide meaningful context. This avoids
    /// losing the command byte in the From conversion path.
    #[error("USB timeout on command 0x{cmd:02X}")]
    Timeout { cmd: u8 },

    /// Echo byte mismatch after retries: firmware responded with a different command byte.
    #[error(
        "echo mismatch: expected 0x{expected:02X}, got 0x{actual:02X} after {attempts} attempts"
    )]
    EchoMismatch {
        expected: u8,
        actual: u8,
        attempts: usize,
    },

    /// No device found matching the given VID:PID.
    #[error("device not found: VID 0x{vid:04X}, PID 0x{pid:04X}")]
    DeviceNotFound { vid: u16, pid: u16 },

    /// Key index or layer exceeds device bounds.
    #[error(
        "bounds violation: key_index {key_index} exceeds max {max_keys}, layer {layer} exceeds max {max_layers}"
    )]
    BoundsViolation {
        key_index: u16,
        max_keys: u16,
        layer: u8,
        max_layers: u8,
    },

    /// Kernel driver (usbhid) is still bound to the interface.
    #[error("kernel driver active on interface {interface}")]
    KernelDriverActive { interface: u8 },

    /// USB device was disconnected during operation.
    #[error("device disconnected")]
    Disconnected,

    /// Transport command channel was closed (transport thread shut down).
    #[error("transport channel closed")]
    ChannelClosed,

    /// Command payload exceeded fixed HID report capacity.
    #[error(
        "invalid command payload for 0x{cmd:02X}: {payload_len} bytes exceeds max {max_payload_len}"
    )]
    InvalidCommandPayload {
        cmd: u8,
        payload_len: usize,
        max_payload_len: usize,
    },

    /// Command byte is not in the device's command vocabulary (strict mode).
    #[error("unknown command 0x{cmd:02X} for this device")]
    UnknownCommand { cmd: u8 },
}

impl From<rusb::Error> for TransportError {
    /// Map ALL rusb errors uniformly to `TransportError::Usb(String)`.
    ///
    /// This intentionally does NOT special-case `rusb::Error::Timeout` into
    /// `TransportError::Timeout`. The `Timeout { cmd }` variant requires knowing
    /// which command byte timed out, which is only available at the flow_control layer.
    /// Mapping rusb timeouts to `Usb(String)` preserves the raw USB error context
    /// without losing information.
    fn from(err: rusb::Error) -> Self {
        TransportError::Usb(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usb_error_display() {
        let err = TransportError::Usb("test error".to_string());
        assert_eq!(err.to_string(), "USB error: test error");
    }

    #[test]
    fn test_timeout_display() {
        let err = TransportError::Timeout { cmd: 0x06 };
        assert_eq!(err.to_string(), "USB timeout on command 0x06");
    }

    #[test]
    fn test_echo_mismatch_stores_expected() {
        let err = TransportError::EchoMismatch {
            expected: 0x8F,
            actual: 0x06,
            attempts: 3,
        };
        assert_eq!(
            err.to_string(),
            "echo mismatch: expected 0x8F, got 0x06 after 3 attempts"
        );
    }

    #[test]
    fn test_device_not_found_display() {
        let err = TransportError::DeviceNotFound {
            vid: 0x3151,
            pid: 0x4015,
        };
        assert_eq!(err.to_string(), "device not found: VID 0x3151, PID 0x4015");
    }

    #[test]
    fn test_bounds_violation_stores_fields() {
        let err = TransportError::BoundsViolation {
            key_index: 110,
            max_keys: 108,
            layer: 5,
            max_layers: 4,
        };
        assert_eq!(
            err.to_string(),
            "bounds violation: key_index 110 exceeds max 108, layer 5 exceeds max 4"
        );
    }

    #[test]
    fn test_kernel_driver_active_stores_interface() {
        let err = TransportError::KernelDriverActive { interface: 2 };
        assert_eq!(err.to_string(), "kernel driver active on interface 2");
    }

    #[test]
    fn test_invalid_command_payload_display() {
        let err = TransportError::InvalidCommandPayload {
            cmd: 0x11,
            payload_len: 80,
            max_payload_len: 63,
        };
        assert_eq!(
            err.to_string(),
            "invalid command payload for 0x11: 80 bytes exceeds max 63"
        );
    }

    #[test]
    fn test_from_rusb_timeout_maps_to_usb() {
        let rusb_err = rusb::Error::Timeout;
        let err: TransportError = rusb_err.into();
        match &err {
            TransportError::Usb(msg) => {
                // rusb::Error::Timeout's Display output -- should NOT be TransportError::Timeout
                assert!(!msg.is_empty(), "USB error message should not be empty");
            }
            other => panic!("expected TransportError::Usb, got: {:?}", other),
        }
    }

    #[test]
    fn test_from_rusb_io_maps_to_usb() {
        let rusb_err = rusb::Error::Io;
        let err: TransportError = rusb_err.into();
        match &err {
            TransportError::Usb(msg) => {
                assert!(!msg.is_empty(), "USB error message should not be empty");
            }
            other => panic!("expected TransportError::Usb, got: {:?}", other),
        }
    }
}
