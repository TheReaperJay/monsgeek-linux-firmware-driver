pub mod ble;
pub mod cmd;
pub mod device;
pub mod error;
pub mod hid;
pub mod magnetism;
pub mod precision;
pub mod registry;
pub mod rgb;
pub mod timing;

pub use device::DeviceDefinition;
pub use error::{ProtocolError, RegistryError};
pub use registry::DeviceRegistry;
