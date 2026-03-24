use std::time::{Duration, Instant};

use monsgeek_protocol::{ChecksumType, DeviceDefinition, ProtocolFamily, cmd, hid, timing};

use crate::error::TransportError;
use crate::flow_control;
use crate::usb::{UsbSession, UsbVersionInfo};

/// Single authority for issuing HID command traffic to the keyboard.
///
/// This controller centralizes:
/// - command payload sanitization
/// - mandatory inter-command spacing
/// - retry/query behavior via flow_control
///
/// All runtime command paths must pass through this type.
pub(crate) struct CommandController {
    session: UsbSession,
    policy: CommandPolicy,
    last_command_at: Instant,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CommandPolicy {
    /// YiChip debounce SET payload requires [profile, value] with profile=0.
    /// We normalize legacy single-byte callers to this wire shape.
    yi_chip_set_debounce_cmd: Option<u8>,
}

impl CommandPolicy {
    pub(crate) fn for_device(device: &DeviceDefinition) -> Self {
        let commands = device.commands();
        if device.protocol_family() == ProtocolFamily::YiChip {
            Self {
                yi_chip_set_debounce_cmd: Some(commands.set_debounce),
            }
        } else {
            Self::default()
        }
    }
}

impl CommandController {
    pub(crate) fn new(session: UsbSession) -> Self {
        Self::with_policy(session, CommandPolicy::default())
    }

    pub(crate) fn with_policy(session: UsbSession, policy: CommandPolicy) -> Self {
        Self {
            session,
            policy,
            // First command should execute immediately.
            last_command_at: Instant::now() - Duration::from_millis(timing::DEFAULT_DELAY_MS * 2),
        }
    }

    pub(crate) fn session(&self) -> &UsbSession {
        &self.session
    }

    pub(crate) fn into_session(self) -> UsbSession {
        self.session
    }

    pub(crate) fn send(
        &mut self,
        cmd: u8,
        data: &[u8],
        checksum: ChecksumType,
    ) -> Result<(), TransportError> {
        let sanitized = sanitize_send_payload(cmd, data, self.policy);
        validate_payload(cmd, &sanitized)?;
        self.enforce_inter_command_delay();
        let result = flow_control::send_command(&self.session, cmd, &sanitized, checksum);
        self.last_command_at = Instant::now();
        result
    }

    pub(crate) fn query(
        &mut self,
        cmd: u8,
        data: &[u8],
        checksum: ChecksumType,
    ) -> Result<[u8; 64], TransportError> {
        validate_payload(cmd, data)?;
        self.enforce_inter_command_delay();
        let result = flow_control::query_command(&self.session, cmd, data, checksum);
        self.last_command_at = Instant::now();
        result
    }

    pub(crate) fn read_feature_report(&mut self) -> Result<[u8; 64], TransportError> {
        self.enforce_inter_command_delay();
        let result = self.session.vendor_get_report();
        self.last_command_at = Instant::now();
        result
    }

    pub(crate) fn query_usb_version(&mut self) -> Result<UsbVersionInfo, TransportError> {
        let response = self.query(cmd::GET_USB_VERSION, &[], ChecksumType::Bit7)?;
        UsbVersionInfo::parse(&response)
    }

    fn enforce_inter_command_delay(&self) {
        let elapsed = self.last_command_at.elapsed();
        let required = Duration::from_millis(timing::DEFAULT_DELAY_MS);
        if elapsed < required {
            std::thread::sleep(required - elapsed);
        }
    }
}

fn validate_payload(cmd: u8, data: &[u8]) -> Result<(), TransportError> {
    let max_payload_len = hid::REPORT_SIZE.saturating_sub(2);
    if data.len() > max_payload_len {
        return Err(TransportError::InvalidCommandPayload {
            cmd,
            payload_len: data.len(),
            max_payload_len,
        });
    }
    Ok(())
}

fn sanitize_send_payload(cmd: u8, data: &[u8], policy: CommandPolicy) -> Vec<u8> {
    // YiChip debounce SET uses payload shape [0x00, value].
    // If legacy callers pass only [value], normalize to the correct wire shape.
    if policy.yi_chip_set_debounce_cmd == Some(cmd) && data.len() == 1 && data[0] <= 50 {
        vec![0, data[0]]
    } else {
        data.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::{CommandPolicy, sanitize_send_payload, validate_payload};
    use crate::error::TransportError;

    #[test]
    fn validate_payload_accepts_max_size() {
        let data = vec![0u8; 63];
        validate_payload(0x11, &data).expect("max-size payload should be accepted");
    }

    #[test]
    fn validate_payload_rejects_oversize() {
        let data = vec![0u8; 64];
        let err = validate_payload(0x11, &data).expect_err("oversize payload must be rejected");
        match err {
            TransportError::InvalidCommandPayload {
                cmd,
                payload_len,
                max_payload_len,
            } => {
                assert_eq!(cmd, 0x11);
                assert_eq!(payload_len, 64);
                assert_eq!(max_payload_len, 63);
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn sanitize_yichip_debounce_single_byte() {
        let out = sanitize_send_payload(
            0x11,
            &[5],
            CommandPolicy {
                yi_chip_set_debounce_cmd: Some(0x11),
            },
        );
        assert_eq!(out, vec![0, 5]);
    }

    #[test]
    fn sanitize_leaves_non_debounce_payload_unchanged() {
        let policy = CommandPolicy {
            yi_chip_set_debounce_cmd: Some(0x11),
        };

        let out = sanitize_send_payload(0x11, &[0, 5], policy);
        assert_eq!(out, vec![0, 5]);
        let out2 = sanitize_send_payload(0x06, &[5], policy);
        assert_eq!(out2, vec![5]);
    }

    #[test]
    fn sanitize_does_not_rewrite_without_policy() {
        let out = sanitize_send_payload(0x11, &[5], CommandPolicy::default());
        assert_eq!(out, vec![5]);
    }
}
