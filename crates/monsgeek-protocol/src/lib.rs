pub mod ble;
pub mod checksum;
pub mod cmd;
pub mod device;
pub mod error;
pub mod hid;
pub mod magnetism;
pub mod precision;
pub mod protocol;
pub mod registry;
pub mod rgb;
pub mod timing;

pub use checksum::{apply_checksum, build_ble_command, build_command, calculate_checksum, ChecksumType};
pub use device::{CommandOverrides, DeviceDefinition};
pub use error::{ProtocolError, RegistryError};
pub use protocol::{CommandTable, ProtocolFamily, RY5088_COMMANDS, YICHIP_COMMANDS};
pub use registry::DeviceRegistry;
