pub mod ble;
pub mod checksum;
pub mod cmd;
pub mod command_policy;
pub mod command_schema;
pub mod device;
pub mod error;
pub mod hid;
pub mod magnetism;
pub mod precision;
pub mod protocol;
pub mod registry;
pub mod rgb;
pub mod timing;

pub use checksum::{
    ChecksumType, apply_checksum, build_ble_command, build_command, calculate_checksum,
};
pub use command_policy::{
    CommandClass, CommandDispatchPolicy, CommandPolicyError, CommandPolicyErrorCode,
    CommandReadPolicy, OutboundCommandDecision, evaluate_outbound_command,
    normalize_outbound_command,
};
pub use command_schema::{
    CommandResolution, CommandSchemaMap, MAX_PAYLOAD_SIZE, NormalizerFn, PayloadSchema,
};
pub use device::{CommandOverrides, ControlTransport, DeviceDefinition, FnSysLayer};
pub use error::{ProtocolError, RegistryError};
pub use protocol::{CommandTable, ProtocolFamily, RY5088_COMMANDS, YICHIP_COMMANDS};
pub use registry::DeviceRegistry;
