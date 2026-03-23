use std::path::Path;

use monsgeek_protocol::DeviceRegistry;

fn devices_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("monsgeek-protocol")
        .join("devices")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device_id = std::env::args()
        .nth(1)
        .map(|arg| arg.parse::<i32>())
        .transpose()?
        .unwrap_or(1308);

    let registry = DeviceRegistry::load_from_directory(&devices_dir())?;
    let device = registry
        .find_by_id(device_id)
        .ok_or_else(|| format!("device ID {device_id} not found in registry"))?;

    println!(
        "Recovering {} (registry ID {}) via native reset/reopen...",
        device.display_name, device.id
    );

    let usb_version = monsgeek_transport::recover(device)?;
    println!(
        "Recovery OK: device ID {} (0x{:08X}), firmware version 0x{:04X}",
        usb_version.device_id_i32(),
        usb_version.device_id,
        usb_version.firmware_version
    );

    Ok(())
}
