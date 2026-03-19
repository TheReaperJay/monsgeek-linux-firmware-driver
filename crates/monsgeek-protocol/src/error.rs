use thiserror::Error;

/// Errors from protocol-level operations (command construction, response parsing).
#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("invalid checksum: expected {expected:#04X}, actual {actual:#04X}")]
    InvalidChecksum { expected: u8, actual: u8 },

    #[error("invalid command: {0:#04X}")]
    InvalidCommand(u8),

    #[error("response error for command {cmd:#04X}: status {status:#04X}")]
    ResponseError { cmd: u8, status: u8 },
}

/// Errors from device registry operations (loading, scanning, parsing).
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("glob pattern error: {0}")]
    GlobPattern(String),

    #[error("failed to read file: {0}")]
    ReadFile(String),

    #[error("failed to parse JSON: {0}")]
    ParseJson(String),

    #[error("duplicate device ID: {0}")]
    DuplicateDeviceId(i32),

    #[error("no devices found in: {0}")]
    NoDevicesFound(String),
}
