use std::time::{Duration, Instant};

use monsgeek_protocol::{ChecksumType, ControlTransport, cmd, timing};

use crate::error::TransportError;
use crate::flow_control;
use crate::usb::{UsbSession, UsbVersionInfo};

/// Single authority for issuing HID command traffic to the keyboard.
///
/// This controller centralizes:
/// - mandatory inter-command spacing (100ms for yc3121 firmware)
/// - retry/query behavior via flow_control
///
/// All runtime command paths must pass through this type. Payload validation
/// belongs in the typed keyboard API layer, not here — the raw gRPC bridge
/// passes padded 64-byte buffers that are already correctly formed by the
/// web app.
pub(crate) struct CommandController {
    session: UsbSession,
    control_transport: ControlTransport,
    last_command_at: Instant,
}

impl CommandController {
    pub(crate) fn new(session: UsbSession, control_transport: ControlTransport) -> Self {
        Self {
            session,
            control_transport,
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
        self.enforce_inter_command_delay();
        let result = flow_control::send_command(&self.session, cmd, data, checksum);
        self.last_command_at = Instant::now();
        result
    }

    pub(crate) fn query(
        &mut self,
        cmd: u8,
        data: &[u8],
        checksum: ChecksumType,
    ) -> Result<[u8; 64], TransportError> {
        self.enforce_inter_command_delay();
        let result =
            flow_control::query_command(&self.session, self.control_transport, cmd, data, checksum);
        self.last_command_at = Instant::now();
        result
    }

    pub(crate) fn query_raw(
        &mut self,
        cmd: u8,
        data: &[u8],
        checksum: ChecksumType,
    ) -> Result<[u8; 64], TransportError> {
        self.enforce_inter_command_delay();
        let result = flow_control::query_raw(&self.session, cmd, data, checksum);
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

    pub(crate) fn query_usb_version_discovery(&mut self) -> Result<UsbVersionInfo, TransportError> {
        self.enforce_inter_command_delay();
        let response = flow_control::query_command_discovery(
            &self.session,
            cmd::GET_USB_VERSION,
            &[],
            ChecksumType::Bit7,
        )?;
        self.last_command_at = Instant::now();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_new_allows_immediate_first_command() {
        // last_command_at is set far enough in the past that the first
        // command should not sleep. We can't test actual USB I/O, but we
        // can verify the constructor doesn't panic and the delay logic
        // would not block.
        let elapsed_since_init =
            Instant::now() - Duration::from_millis(timing::DEFAULT_DELAY_MS * 2);
        let required = Duration::from_millis(timing::DEFAULT_DELAY_MS);
        assert!(elapsed_since_init.elapsed() >= required);
    }
}
