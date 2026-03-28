use anyhow::Result;
use serde::Serialize;

use crate::commands::CommandExecution;
use crate::device_select::{OnlineDevice, ResolvedTargetDevice, preferred_model_slug};

#[derive(Debug, Clone, Serialize)]
struct DeviceRow {
    path: String,
    device_id: i32,
    model: String,
    display_name: String,
    vid: u16,
    pid: u16,
}

#[derive(Debug, Clone, Serialize)]
struct CommandOutput<'a> {
    target_path: &'a str,
    target_device_id: i32,
    target_model: String,
    operation: &'a str,
    request: &'a Option<Vec<u8>>,
    checksum: &'a Option<String>,
    response: &'a Option<Vec<u8>>,
    detail: &'a Option<String>,
}

pub fn print_devices(json: bool, devices: &[OnlineDevice]) -> Result<()> {
    let rows: Vec<DeviceRow> = devices
        .iter()
        .map(|device| DeviceRow {
            path: device.path.clone(),
            device_id: device.device_id,
            model: preferred_model_slug(&device.definition),
            display_name: device.definition.display_name.clone(),
            vid: device.vid,
            pid: device.pid,
        })
        .collect();

    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }

    if rows.is_empty() {
        println!("no supported online devices found");
        return Ok(());
    }

    for row in &rows {
        println!(
            "{}  id={}  model={}  {} ({:04x}:{:04x})",
            row.path, row.device_id, row.model, row.display_name, row.vid, row.pid
        );
    }
    Ok(())
}

pub fn print_command_result(
    json: bool,
    target: &ResolvedTargetDevice,
    result: &CommandExecution,
) -> Result<()> {
    if json {
        let payload = CommandOutput {
            target_path: &target.path,
            target_device_id: target.device_id,
            target_model: preferred_model_slug(&target.definition),
            operation: &result.operation,
            request: &result.request,
            checksum: &result.checksum,
            response: &result.response,
            detail: &result.detail,
        };
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    println!(
        "target={} id={} model={}",
        target.path,
        target.device_id,
        preferred_model_slug(&target.definition)
    );
    println!("operation={}", result.operation);
    if let Some(request) = result.request.as_ref() {
        println!("request={}", bytes_to_hex(request));
    }
    if let Some(checksum) = result.checksum.as_ref() {
        println!("checksum={checksum}");
    }
    if let Some(response) = result.response.as_ref() {
        println!("response={}", bytes_to_hex(response));
    }
    if let Some(detail) = result.detail.as_ref() {
        println!("detail={detail}");
    }
    Ok(())
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<String>>()
        .join(" ")
}
