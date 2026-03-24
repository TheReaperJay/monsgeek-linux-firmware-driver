use std::path::Path;

use monsgeek_protocol::{ChecksumType, DeviceRegistry};

fn devices_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("monsgeek-protocol")
        .join("devices")
}

fn decode_debounce(get_cmd: u8, resp: &[u8; 64]) -> u8 {
    if get_cmd == 0x91 { resp[2] } else { resp[1] }
}

fn build_set_debounce_payload(set_cmd: u8, value: u8) -> Vec<u8> {
    if set_cmd == 0x11 {
        vec![0, value]
    } else {
        vec![value]
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Init logger so transport log::warn/info messages are visible.
    let _ = simple_logger::init_with_level(log::Level::Info);

    let device_id = std::env::args()
        .nth(1)
        .map(|arg| arg.parse::<i32>())
        .transpose()?
        .unwrap_or(1308);

    let registry = DeviceRegistry::load_from_directory(&devices_dir())?;
    let device = registry
        .find_by_id(device_id)
        .ok_or_else(|| format!("device ID {device_id} not found in registry"))?;

    let commands = device.commands();
    println!(
        "Device: {} (ID {}), set_debounce=0x{:02X}, get_debounce=0x{:02X}",
        device.display_name, device.id, commands.set_debounce, commands.get_debounce
    );

    // Connect and verify identity
    let (handle, _events) = monsgeek_transport::connect(device)?;
    println!("Connected. Transport thread running.");

    // Step 1: Read current debounce via query (send + read, echo-matched)
    println!("\n--- Step 1: GET_DEBOUNCE (query) ---");
    let resp = handle.send_query(commands.get_debounce, &[], ChecksumType::Bit7)?;
    let current = decode_debounce(commands.get_debounce, &resp);
    println!(
        "Current debounce: {} (response echo=0x{:02X})",
        current, resp[0]
    );

    // Step 2: Send ONE debounce write (fire-and-forget, no read)
    let new_value = 1u8;
    println!(
        "\n--- Step 2: SET_DEBOUNCE fire-and-forget (value={}) ---",
        new_value
    );
    let payload = build_set_debounce_payload(commands.set_debounce, new_value);
    handle.send_fire_and_forget(commands.set_debounce, &payload, ChecksumType::Bit7)?;
    println!("send_fire_and_forget returned OK");

    // Step 3: Verify keyboard still responsive — query debounce again
    println!("\n--- Step 3: GET_DEBOUNCE (query, verify responsive) ---");
    match handle.send_query(commands.get_debounce, &[], ChecksumType::Bit7) {
        Ok(resp) => println!(
            "Still responsive. Debounce={} (echo=0x{:02X})",
            decode_debounce(commands.get_debounce, &resp),
            resp[0]
        ),
        Err(e) => println!("FAILED after 1 fire-and-forget: {}", e),
    }

    // Step 4: Send 3 more fire-and-forget writes, then check
    println!("\n--- Step 4: 3x SET_DEBOUNCE fire-and-forget ---");
    let payload = build_set_debounce_payload(commands.set_debounce, new_value);
    for i in 0..3 {
        handle.send_fire_and_forget(commands.set_debounce, &payload, ChecksumType::Bit7)?;
        println!("  write {} OK", i + 1);
    }
    println!("Verifying responsive...");
    match handle.send_query(commands.get_debounce, &[], ChecksumType::Bit7) {
        Ok(resp) => println!(
            "Still responsive after 3 writes. Debounce={} (echo=0x{:02X})",
            decode_debounce(commands.get_debounce, &resp),
            resp[0]
        ),
        Err(e) => println!("FAILED after 3 fire-and-forget writes: {}", e),
    }

    // Step 5: Send 9 fire-and-forget writes (matches the web app burst), then check
    println!("\n--- Step 5: 9x SET_DEBOUNCE fire-and-forget ---");
    let payload = build_set_debounce_payload(commands.set_debounce, new_value);
    for i in 0..9 {
        handle.send_fire_and_forget(commands.set_debounce, &payload, ChecksumType::Bit7)?;
        println!("  write {} OK", i + 1);
    }
    println!("Verifying responsive...");
    match handle.send_query(commands.get_debounce, &[], ChecksumType::Bit7) {
        Ok(resp) => println!(
            "Still responsive after 9 writes. Debounce={} (echo=0x{:02X})",
            decode_debounce(commands.get_debounce, &resp),
            resp[0]
        ),
        Err(e) => println!("FAILED after 9 fire-and-forget writes: {}", e),
    }

    // Restore original value
    println!(
        "\n--- Restoring debounce to original value ({}) ---",
        current
    );
    let payload = build_set_debounce_payload(commands.set_debounce, current);
    handle.send_fire_and_forget(commands.set_debounce, &payload, ChecksumType::Bit7)?;

    handle.shutdown();
    println!("\nDone.");
    Ok(())
}
