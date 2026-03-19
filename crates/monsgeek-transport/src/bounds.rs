//! Key matrix bounds validation.
//!
//! Validates key indices and layer numbers against device bounds before any
//! USB write operation. This prevents firmware out-of-bounds memory corruption
//! on the yc3121.

use crate::error::TransportError;

/// Validate a key index and layer against device bounds.
///
/// Returns `Ok(())` if `key_index < max_keys` AND `layer < max_layers`.
/// Returns `Err(BoundsViolation)` otherwise.
///
/// This MUST be called before any SET_KEYMATRIX write to prevent
/// firmware out-of-bounds memory corruption.
pub fn validate_key_index(
    key_index: u16,
    max_keys: u16,
    layer: u8,
    max_layers: u8,
) -> Result<(), TransportError> {
    if key_index >= max_keys || layer >= max_layers {
        return Err(TransportError::BoundsViolation {
            key_index,
            max_keys,
            layer,
            max_layers,
        });
    }
    Ok(())
}
