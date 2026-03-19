//! HID transport layer for MonsGeek yc3121 keyboards.
//!
//! Provides USB device access via `rusb` control transfers on IF2 (vendor interface),
//! key matrix bounds validation, and error types for all transport-layer operations.

pub mod bounds;
pub mod discovery;
pub mod error;
pub mod flow_control;
pub mod thread;
pub mod usb;

pub use bounds::{validate_key_index, validate_write_request};
pub use discovery::DeviceInfo;
pub use error::TransportError;
pub use thread::TransportEvent;
pub use usb::UsbSession;
