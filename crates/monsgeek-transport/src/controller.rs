use std::time::{Duration, Instant};

use monsgeek_protocol::{
    ChecksumType, CommandResolution, CommandSchemaMap, MAX_PAYLOAD_SIZE, PayloadSchema, cmd, timing,
};

use crate::error::TransportError;
use crate::flow_control;
use crate::usb::{UsbSession, UsbVersionInfo};

/// Single authority for issuing HID command traffic to the keyboard.
///
/// This controller centralizes:
/// - device-aware command payload normalization and validation
/// - mandatory inter-command spacing
/// - retry/query behavior via flow_control
///
/// All runtime command paths must pass through this type.
pub(crate) struct CommandController {
    session: UsbSession,
    schema_map: CommandSchemaMap,
    last_command_at: Instant,
}

impl CommandController {
    /// Create a controller for pre-identity probe sessions.
    ///
    /// Uses a minimal schema that only recognizes `GET_USB_VERSION` and `GET_REV`.
    /// Used by `open_matching_session` and `recover` before the device definition
    /// is known.
    pub(crate) fn new(session: UsbSession) -> Self {
        Self {
            session,
            schema_map: CommandSchemaMap::probe_only(),
            last_command_at: Instant::now() - Duration::from_millis(timing::DEFAULT_DELAY_MS * 2),
        }
    }

    /// Create a controller with an explicit schema map.
    pub(crate) fn with_schema(session: UsbSession, schema_map: CommandSchemaMap) -> Self {
        Self {
            session,
            schema_map,
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
        let wire_data = self.normalize_and_validate(cmd, data)?;
        self.enforce_inter_command_delay();
        let result = flow_control::send_command(&self.session, cmd, &wire_data, checksum);
        self.last_command_at = Instant::now();
        result
    }

    pub(crate) fn query(
        &mut self,
        cmd: u8,
        data: &[u8],
        checksum: ChecksumType,
    ) -> Result<[u8; 64], TransportError> {
        let wire_data = self.normalize_and_validate(cmd, data)?;
        self.enforce_inter_command_delay();
        let result = flow_control::query_command(&self.session, cmd, &wire_data, checksum);
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

    /// Normalize and validate a command payload against the device's schema.
    ///
    /// For known/shared commands: applies schema-specific normalization and
    /// strict validation. For unknown commands: applies size-only validation
    /// with a warning (or rejects in strict mode).
    fn normalize_and_validate(&self, cmd: u8, data: &[u8]) -> Result<Vec<u8>, TransportError> {
        match self.schema_map.resolve(cmd) {
            CommandResolution::Known(schema) | CommandResolution::Shared(schema) => {
                apply_schema(cmd, data, schema)
            }
            CommandResolution::Unknown => {
                if self.schema_map.is_strict() {
                    return Err(TransportError::UnknownCommand { cmd });
                }
                log::warn!(
                    "Unknown command 0x{:02X} for {} device — passing through with size validation only",
                    cmd,
                    self.schema_map.protocol_family()
                );
                validate_size(cmd, data)?;
                Ok(data.to_vec())
            }
        }
    }

    fn enforce_inter_command_delay(&self) {
        let elapsed = self.last_command_at.elapsed();
        let required = Duration::from_millis(timing::DEFAULT_DELAY_MS);
        if elapsed < required {
            std::thread::sleep(required - elapsed);
        }
    }
}

/// Apply a payload schema: normalize the data and validate the result.
fn apply_schema(cmd: u8, data: &[u8], schema: &PayloadSchema) -> Result<Vec<u8>, TransportError> {
    match schema {
        PayloadSchema::Empty => {
            if !data.is_empty() {
                return Err(TransportError::InvalidCommandPayload {
                    cmd,
                    payload_len: data.len(),
                    max_payload_len: 0,
                });
            }
            Ok(Vec::new())
        }
        PayloadSchema::FixedSize(expected) => {
            if data.len() != *expected {
                return Err(TransportError::InvalidCommandPayload {
                    cmd,
                    payload_len: data.len(),
                    max_payload_len: *expected,
                });
            }
            Ok(data.to_vec())
        }
        PayloadSchema::Range { min, max } => {
            if data.len() < *min || data.len() > *max {
                return Err(TransportError::InvalidCommandPayload {
                    cmd,
                    payload_len: data.len(),
                    max_payload_len: *max,
                });
            }
            Ok(data.to_vec())
        }
        PayloadSchema::Normalized {
            wire_size,
            normalizer,
        } => {
            let normalized = normalizer.normalize(data);
            if normalized.len() != *wire_size {
                return Err(TransportError::InvalidCommandPayload {
                    cmd,
                    payload_len: normalized.len(),
                    max_payload_len: *wire_size,
                });
            }
            Ok(normalized)
        }
        PayloadSchema::VariableWithMax(max) => {
            let effective_max = (*max).min(MAX_PAYLOAD_SIZE);
            if data.len() > effective_max {
                return Err(TransportError::InvalidCommandPayload {
                    cmd,
                    payload_len: data.len(),
                    max_payload_len: effective_max,
                });
            }
            Ok(data.to_vec())
        }
    }
}

/// Basic size-only validation for unknown commands (permissive mode fallback).
fn validate_size(cmd: u8, data: &[u8]) -> Result<(), TransportError> {
    if data.len() > MAX_PAYLOAD_SIZE {
        return Err(TransportError::InvalidCommandPayload {
            cmd,
            payload_len: data.len(),
            max_payload_len: MAX_PAYLOAD_SIZE,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use monsgeek_protocol::NormalizerFn;

    // ── apply_schema tests ──────────────────────────────────────────────

    #[test]
    fn apply_schema_empty_accepts_empty() {
        let result = apply_schema(0x01, &[], &PayloadSchema::Empty);
        assert_eq!(result.unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn apply_schema_empty_rejects_data() {
        let result = apply_schema(0x01, &[0x05], &PayloadSchema::Empty);
        assert!(result.is_err());
        match result.unwrap_err() {
            TransportError::InvalidCommandPayload {
                cmd,
                payload_len,
                max_payload_len,
            } => {
                assert_eq!(cmd, 0x01);
                assert_eq!(payload_len, 1);
                assert_eq!(max_payload_len, 0);
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn apply_schema_fixed_size_exact() {
        let result = apply_schema(0x04, &[0x01], &PayloadSchema::FixedSize(1));
        assert_eq!(result.unwrap(), vec![0x01]);
    }

    #[test]
    fn apply_schema_fixed_size_wrong() {
        let result = apply_schema(0x04, &[0x01, 0x02], &PayloadSchema::FixedSize(1));
        assert!(result.is_err());
    }

    #[test]
    fn apply_schema_fixed_size_empty_when_expects_one() {
        let result = apply_schema(0x04, &[], &PayloadSchema::FixedSize(1));
        assert!(result.is_err());
    }

    #[test]
    fn apply_schema_normalized_transforms() {
        let schema = PayloadSchema::Normalized {
            wire_size: 2,
            normalizer: NormalizerFn::PrependProfileZero,
        };
        let result = apply_schema(0x11, &[5], &schema);
        assert_eq!(result.unwrap(), vec![0x00, 5]);
    }

    #[test]
    fn apply_schema_normalized_passthrough_correct_size() {
        let schema = PayloadSchema::Normalized {
            wire_size: 2,
            normalizer: NormalizerFn::PrependProfileZero,
        };
        let result = apply_schema(0x11, &[0x00, 5], &schema);
        assert_eq!(result.unwrap(), vec![0x00, 5]);
    }

    #[test]
    fn apply_schema_normalized_rejects_wrong_size_after_transform() {
        let schema = PayloadSchema::Normalized {
            wire_size: 2,
            normalizer: NormalizerFn::PrependProfileZero,
        };
        // 3 bytes in, normalizer passes through (len != 1), result is 3 bytes, wire_size is 2
        let result = apply_schema(0x11, &[0x00, 5, 0xFF], &schema);
        assert!(result.is_err());
    }

    #[test]
    fn apply_schema_variable_max_within() {
        let result = apply_schema(0x0A, &[0u8; 60], &PayloadSchema::VariableWithMax(63));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 60);
    }

    #[test]
    fn apply_schema_variable_max_at_boundary() {
        let result = apply_schema(0x0A, &[0u8; 63], &PayloadSchema::VariableWithMax(63));
        assert!(result.is_ok());
    }

    #[test]
    fn apply_schema_variable_max_over() {
        let result = apply_schema(0x0A, &[0u8; 64], &PayloadSchema::VariableWithMax(63));
        assert!(result.is_err());
    }

    #[test]
    fn apply_schema_variable_max_empty() {
        let result = apply_schema(0x0A, &[], &PayloadSchema::VariableWithMax(63));
        assert!(result.is_ok());
    }

    #[test]
    fn apply_schema_range_within() {
        let schema = PayloadSchema::Range { min: 1, max: 8 };
        let result = apply_schema(0x09, &[0x01, 0x02, 0x03, 0x04], &schema);
        assert!(result.is_ok());
    }

    #[test]
    fn apply_schema_range_at_min() {
        let schema = PayloadSchema::Range { min: 1, max: 8 };
        let result = apply_schema(0x09, &[0x01], &schema);
        assert!(result.is_ok());
    }

    #[test]
    fn apply_schema_range_under() {
        let schema = PayloadSchema::Range { min: 1, max: 8 };
        let result = apply_schema(0x09, &[], &schema);
        assert!(result.is_err());
    }

    #[test]
    fn apply_schema_range_over() {
        let schema = PayloadSchema::Range { min: 1, max: 8 };
        let result = apply_schema(0x09, &[0u8; 9], &schema);
        assert!(result.is_err());
    }

    // ── validate_size tests ─────────────────────────────────────────────

    #[test]
    fn validate_size_accepts_max() {
        assert!(validate_size(0x11, &[0u8; 63]).is_ok());
    }

    #[test]
    fn validate_size_rejects_oversize() {
        assert!(validate_size(0x11, &[0u8; 64]).is_err());
    }
}
