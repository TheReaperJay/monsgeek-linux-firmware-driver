//! HID transport layer for MonsGeek yc3121 keyboards.
//!
//! Provides USB device access via `rusb` control transfers on IF2 (vendor interface),
//! key matrix bounds validation, and error types for all transport-layer operations.

pub mod bounds;
pub mod error;
pub mod usb;

pub use bounds::validate_key_index;
pub use error::TransportError;
pub use usb::UsbSession;
