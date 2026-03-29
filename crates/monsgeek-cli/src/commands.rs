use std::future::Future;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Instant;

use anyhow::{Result, anyhow, bail};
use monsgeek_driver::pb::driver::CheckSumType;
use monsgeek_firmware::{
    CompatibilityCheck, DOWNLOAD_BASE, DownloadProgress, FirmwareManifest, FirmwareSource,
    FirmwareTarget, PreflightDecision, PreflightRequest, REQUIRED_TYPED_PHRASE, VendorApiError,
    check_vendor_firmware, download_vendor_firmware_with_progress, run_preflight,
};
use monsgeek_protocol::{DeviceDefinition, cmd};
use monsgeek_transport::usb::UsbVersionInfo;
use serde::Serialize;

use crate::client::DriverClient;
use crate::device_select::ResolvedTargetDevice;
use crate::device_select::preferred_model_slug;
use crate::{
    Commands, DebounceCommands, FirmwareCommands, KeymapCommands, LedCommands, MacroCommands,
    PollCommands, ProfileCommands, RawCommands,
};

#[derive(Debug, Clone, Serialize)]
pub struct CommandRequestPlan {
    pub operation: String,
    pub request: Option<Vec<u8>>,
    pub checksum: Option<i32>,
    pub expects_read: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandExecution {
    pub operation: String,
    pub request: Option<Vec<u8>>,
    pub checksum: Option<String>,
    pub response: Option<Vec<u8>>,
    pub detail: Option<String>,
}

pub trait DriverTransport {
    fn send_msg(
        &mut self,
        device_path: String,
        msg_bytes: Vec<u8>,
        checksum_enum_i32: i32,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    fn read_msg(
        &mut self,
        device_path: String,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + '_>>;
}

impl DriverTransport for DriverClient {
    fn send_msg(
        &mut self,
        device_path: String,
        msg_bytes: Vec<u8>,
        checksum_enum_i32: i32,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            DriverClient::send_msg(self, device_path, msg_bytes, checksum_enum_i32).await
        })
    }

    fn read_msg(
        &mut self,
        device_path: String,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + '_>> {
        Box::pin(async move { DriverClient::read_msg(self, device_path).await })
    }
}

pub async fn execute_command<T: DriverTransport>(
    transport: &mut T,
    target: &ResolvedTargetDevice,
    command: &Commands,
    unsafe_mode: bool,
) -> Result<CommandExecution> {
    if let Commands::Firmware { command } = command {
        return execute_firmware_command(transport, target, command).await;
    }

    let plan = build_command_request(command, &target.definition, unsafe_mode)?;

    if let (Some(request), Some(checksum)) = (plan.request.clone(), plan.checksum) {
        transport
            .send_msg(target.path.clone(), request, checksum)
            .await?;
    }

    let response = if plan.expects_read {
        Some(transport.read_msg(target.path.clone()).await?)
    } else {
        None
    };

    Ok(CommandExecution {
        operation: plan.operation,
        request: plan.request,
        checksum: plan.checksum.map(checksum_name).map(ToString::to_string),
        response,
        detail: None,
    })
}

pub fn build_command_request(
    command: &Commands,
    definition: &DeviceDefinition,
    unsafe_mode: bool,
) -> Result<CommandRequestPlan> {
    let table = definition.commands();

    match command {
        Commands::Devices { .. } => bail!("devices list does not use per-device command framing"),
        Commands::Firmware { command } => match command {
            FirmwareCommands::Version => Ok(CommandRequestPlan {
                operation: "firmware version".to_string(),
                request: Some(vec![cmd::GET_USB_VERSION]),
                checksum: Some(CheckSumType::Bit7 as i32),
                expects_read: true,
            }),
            FirmwareCommands::Validate { .. } | FirmwareCommands::Flash { .. } => bail!(
                "firmware validate/flash are multi-step operations and are only available via execute_command"
            ),
        },
        Commands::Info => Ok(CommandRequestPlan {
            operation: "info".to_string(),
            request: Some(vec![cmd::GET_USB_VERSION]),
            checksum: Some(CheckSumType::Bit7 as i32),
            expects_read: true,
        }),
        Commands::Led { command } => match command {
            LedCommands::Get => Ok(CommandRequestPlan {
                operation: "led get".to_string(),
                request: Some(vec![cmd::GET_LEDPARAM]),
                checksum: Some(CheckSumType::Bit7 as i32),
                expects_read: true,
            }),
            LedCommands::Set {
                mode,
                speed,
                brightness,
                dazzle,
                r,
                g,
                b,
            } => {
                let option = if *dazzle { 7 } else { 8 };
                Ok(CommandRequestPlan {
                    operation: "led set".to_string(),
                    request: Some(vec![
                        cmd::SET_LEDPARAM,
                        *mode,
                        4u8.wrapping_sub(*speed),
                        *brightness,
                        option,
                        *r,
                        *g,
                        *b,
                    ]),
                    checksum: Some(CheckSumType::Bit8 as i32),
                    expects_read: false,
                })
            }
        },
        Commands::Debounce { command } => match command {
            DebounceCommands::Get => Ok(CommandRequestPlan {
                operation: "debounce get".to_string(),
                request: Some(vec![table.get_debounce]),
                checksum: Some(CheckSumType::Bit7 as i32),
                expects_read: true,
            }),
            DebounceCommands::Set { value } => {
                let mut message = vec![table.set_debounce];
                if table.set_debounce == 0x11 {
                    message.extend_from_slice(&[0, *value]);
                } else {
                    message.push(*value);
                }

                Ok(CommandRequestPlan {
                    operation: "debounce set".to_string(),
                    request: Some(message),
                    checksum: Some(CheckSumType::Bit7 as i32),
                    expects_read: false,
                })
            }
        },
        Commands::Poll { command } => {
            let get_cmd = table.get_report.unwrap_or(cmd::GET_REPORT);
            let set_cmd = table.set_report.unwrap_or(cmd::SET_REPORT);
            match command {
                PollCommands::Get => Ok(CommandRequestPlan {
                    operation: "poll get".to_string(),
                    request: Some(vec![get_cmd]),
                    checksum: Some(CheckSumType::Bit7 as i32),
                    expects_read: true,
                }),
                PollCommands::Set { value } => Ok(CommandRequestPlan {
                    operation: "poll set".to_string(),
                    request: Some(vec![set_cmd, *value]),
                    checksum: Some(CheckSumType::Bit7 as i32),
                    expects_read: false,
                }),
            }
        }
        Commands::Profile { command } => match command {
            ProfileCommands::Get => Ok(CommandRequestPlan {
                operation: "profile get".to_string(),
                request: Some(vec![table.get_profile]),
                checksum: Some(CheckSumType::Bit7 as i32),
                expects_read: true,
            }),
            ProfileCommands::Set { value } => Ok(CommandRequestPlan {
                operation: "profile set".to_string(),
                request: Some(vec![table.set_profile, *value]),
                checksum: Some(CheckSumType::Bit7 as i32),
                expects_read: false,
            }),
        },
        Commands::Keymap { command } => match command {
            KeymapCommands::Get { profile, key_index } => Ok(CommandRequestPlan {
                operation: "keymap get".to_string(),
                request: Some(vec![table.get_keymatrix, *profile, *key_index, 0, 0]),
                checksum: Some(CheckSumType::Bit7 as i32),
                expects_read: true,
            }),
            KeymapCommands::Set {
                profile,
                key_index,
                layer,
                config_type,
                b1,
                b2,
                b3,
            } => Ok(CommandRequestPlan {
                operation: "keymap set".to_string(),
                request: Some(vec![
                    table.set_keymatrix,
                    *profile,
                    *key_index,
                    0,
                    0,
                    1,
                    *layer,
                    0,
                    *config_type,
                    *b1,
                    *b2,
                    *b3,
                ]),
                checksum: Some(CheckSumType::Bit7 as i32),
                expects_read: false,
            }),
        },
        Commands::Macro { command } => match command {
            MacroCommands::Get { macro_index, page } => Ok(CommandRequestPlan {
                operation: "macro get".to_string(),
                request: Some(vec![cmd::GET_MACRO, *macro_index, *page]),
                checksum: Some(CheckSumType::Bit7 as i32),
                expects_read: true,
            }),
            MacroCommands::Set {
                macro_index,
                page,
                is_last,
                data,
            } => {
                let chunk_len = u8::try_from(data.len())
                    .map_err(|_| anyhow!("macro set data length exceeds 255 bytes"))?;
                let mut frame = vec![
                    table.set_macro,
                    *macro_index,
                    *page,
                    chunk_len,
                    *is_last,
                    0,
                    0,
                ];
                frame.extend_from_slice(data);
                Ok(CommandRequestPlan {
                    operation: "macro set".to_string(),
                    request: Some(frame),
                    checksum: Some(CheckSumType::Bit7 as i32),
                    expects_read: false,
                })
            }
        },
        Commands::Raw { command } => match command {
            RawCommands::Send { bytes } => {
                if bytes.first().copied().unwrap_or(0) < 0x80 && !unsafe_mode {
                    bail!("raw write command rejected: pass --unsafe to send write opcodes");
                }
                Ok(CommandRequestPlan {
                    operation: "raw send".to_string(),
                    request: Some(bytes.clone()),
                    checksum: Some(CheckSumType::Bit7 as i32),
                    expects_read: false,
                })
            }
            RawCommands::Read => Ok(CommandRequestPlan {
                operation: "raw read".to_string(),
                request: None,
                checksum: None,
                expects_read: true,
            }),
        },
    }
}

#[derive(Debug, Clone, Default)]
pub struct FirmwarePreflightOptions {
    pub allow_unofficial: bool,
    pub assume_yes: bool,
    pub high_risk_ack: bool,
    pub typed_phrase: Option<String>,
    pub backup_attempted: bool,
    pub backup_ok: bool,
    pub allow_backup_failure: bool,
}

pub fn evaluate_firmware_preflight(
    target: &ResolvedTargetDevice,
    file: &Path,
    manifest: FirmwareManifest,
    options: FirmwarePreflightOptions,
) -> PreflightDecision {
    let firmware_source = manifest.source.clone();
    let request = PreflightRequest {
        device_id: target.device_id as u32,
        model_slug: preferred_model_slug(&target.definition),
        device_path: Some(target.path.clone()),
        firmware_source,
        firmware_path: file.to_path_buf(),
        manifest,
        allow_unofficial: options.allow_unofficial,
        assume_yes: options.assume_yes,
        high_risk_ack: options.high_risk_ack,
        typed_phrase: options.typed_phrase,
        backup_attempted: options.backup_attempted,
        backup_ok: options.backup_ok,
        allow_backup_failure: options.allow_backup_failure,
    };
    run_preflight(&request)
}

fn manifest_for_target(
    target: &ResolvedTargetDevice,
    source: FirmwareSource,
    firmware_version: Option<String>,
) -> FirmwareManifest {
    FirmwareManifest {
        format_version: Some("1".to_string()),
        firmware_version,
        target: FirmwareTarget {
            device_id: Some(target.device_id as u32),
            model_slug: Some(preferred_model_slug(&target.definition)),
            board: None,
        },
        source,
        compatibility: CompatibilityCheck {
            expected_device_id: Some(target.device_id as u32),
            expected_model_slug: Some(preferred_model_slug(&target.definition)),
            expected_revision: None,
            min_revision: None,
            max_revision: None,
        },
        metadata_checksum: None,
        image_size_bytes: None,
    }
}

fn preflight_error(decision: &PreflightDecision) -> anyhow::Error {
    anyhow!(decision.errors.join("; "))
}

#[derive(Debug, Clone)]
struct DeviceFirmwareState {
    device_id: u32,
    firmware_version: u16,
    revision: Option<u8>,
    usb_raw: Vec<u8>,
}

#[derive(Debug, Clone)]
struct ResolvedFirmwareInput {
    firmware_path: PathBuf,
    source: FirmwareSource,
    server_usb_version: Option<u16>,
    server_version_str: Option<String>,
    download_path: Option<String>,
    downloaded_bytes: Option<usize>,
}

async fn read_device_firmware_state<T: DriverTransport>(
    transport: &mut T,
    target: &ResolvedTargetDevice,
) -> Result<DeviceFirmwareState> {
    transport
        .send_msg(
            target.path.clone(),
            vec![cmd::GET_USB_VERSION],
            CheckSumType::Bit7 as i32,
        )
        .await?;

    let usb_raw = transport.read_msg(target.path.clone()).await?;
    let usb_info = UsbVersionInfo::parse(&usb_raw)
        .map_err(|err| anyhow!("failed to parse GET_USB_VERSION response: {err}"))?;

    transport
        .send_msg(
            target.path.clone(),
            vec![cmd::GET_REV],
            CheckSumType::Bit7 as i32,
        )
        .await?;
    let revision = if let Ok(rev_raw) = transport.read_msg(target.path.clone()).await {
        if rev_raw.len() >= 2 && rev_raw[0] == cmd::GET_REV {
            Some(rev_raw[1])
        } else {
            None
        }
    } else {
        None
    };

    Ok(DeviceFirmwareState {
        device_id: usb_info.device_id,
        firmware_version: usb_info.firmware_version,
        revision,
        usb_raw,
    })
}

fn default_download_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("firmware")
        .join("downloads")
}

fn make_absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(path)
}

fn canonical_or_absolute(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| make_absolute(path))
}

fn suggested_vendor_filename(
    device_id: u32,
    server_usb_version: Option<u16>,
    download_path: &str,
) -> String {
    let from_path = download_path
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty() && name.contains('.'))
        .map(ToString::to_string);

    from_path.unwrap_or_else(|| {
        if let Some(version) = server_usb_version {
            format!("device-{device_id}-usb-0x{version:04X}.bin")
        } else {
            format!("device-{device_id}-firmware.bin")
        }
    })
}

fn format_usb_version(version: u16) -> String {
    format!("0x{version:04X}")
}

fn map_vendor_error(err: VendorApiError, device_id: u32) -> anyhow::Error {
    match err {
        VendorApiError::Server { code: 500, .. } => anyhow!(
            "vendor firmware index has no entry for device_id={device_id}; server returned errCode=500"
        ),
        other => anyhow!("vendor firmware API failed for device_id={device_id}: {other}"),
    }
}

fn emit_firmware_status(message: impl AsRef<str>) {
    if std::io::stderr().is_terminal() {
        eprintln!("[firmware] {}", message.as_ref());
    }
}

async fn resolve_firmware_input(
    target: &ResolvedTargetDevice,
    file: Option<&PathBuf>,
    download_dir: Option<&PathBuf>,
    allow_unofficial: bool,
) -> Result<ResolvedFirmwareInput> {
    if let Some(file) = file {
        if !allow_unofficial {
            bail!(
                "local firmware file requires --allow-unofficial; default behavior is vendor auto-download"
            );
        }

        let firmware_path = canonical_or_absolute(file);
        emit_firmware_status(format!(
            "using local firmware file {}",
            firmware_path.display()
        ));
        return Ok(ResolvedFirmwareInput {
            source: FirmwareSource::LocalFile {
                path: firmware_path.display().to_string(),
            },
            firmware_path,
            server_usb_version: None,
            server_version_str: None,
            download_path: None,
            downloaded_bytes: None,
        });
    }

    let api_device_id = target.device_id as u32;
    emit_firmware_status(format!(
        "querying official firmware metadata for device_id={api_device_id}"
    ));
    let response = check_vendor_firmware(api_device_id)
        .await
        .map_err(|err| map_vendor_error(err, api_device_id))?;

    let download_path = response.versions.download_path.clone().ok_or_else(|| {
        anyhow!("vendor firmware API returned no download path for device_id={api_device_id}")
    })?;
    let output_dir = if let Some(dir) = download_dir {
        make_absolute(dir)
    } else {
        default_download_dir()
    };
    std::fs::create_dir_all(&output_dir).map_err(|err| {
        anyhow!(
            "failed to create firmware download directory {}: {err}",
            output_dir.display()
        )
    })?;

    let filename = suggested_vendor_filename(api_device_id, response.versions.usb, &download_path);
    let output_path = output_dir.join(filename);
    emit_firmware_status(format!(
        "downloading official firmware to {}",
        output_path.display()
    ));
    let progress_enabled = std::io::stderr().is_terminal();
    let mut last_render = Instant::now();
    let mut last_percent = 0u64;
    let downloaded_bytes =
        download_vendor_firmware_with_progress(&download_path, &output_path, |progress| {
            if !progress_enabled {
                return;
            }

            let now = Instant::now();
            let should_render = now.duration_since(last_render).as_millis() >= 120;
            if !should_render && progress.total_bytes.is_some() {
                return;
            }
            last_render = now;

            render_download_progress(progress, &mut last_percent);
        })
        .await
        .map_err(|err| map_vendor_error(err, api_device_id))?;
    if progress_enabled {
        eprintln!();
    }
    emit_firmware_status(format!("download complete ({} bytes)", downloaded_bytes));
    let firmware_path = canonical_or_absolute(&output_path);

    Ok(ResolvedFirmwareInput {
        source: FirmwareSource::VendorDownload {
            url: format!("{DOWNLOAD_BASE}{download_path}"),
            channel: Some("official".to_string()),
        },
        firmware_path,
        server_usb_version: response.versions.usb,
        server_version_str: if response.versions.raw_version.is_empty() {
            None
        } else {
            Some(response.versions.raw_version)
        },
        download_path: Some(download_path),
        downloaded_bytes: Some(downloaded_bytes),
    })
}

async fn execute_firmware_command<T: DriverTransport>(
    transport: &mut T,
    target: &ResolvedTargetDevice,
    command: &FirmwareCommands,
) -> Result<CommandExecution> {
    match command {
        FirmwareCommands::Version => {
            let current = read_device_firmware_state(transport, target).await?;
            let mut detail = format!(
                "device_id={} firmware_version={}",
                current.device_id,
                format_usb_version(current.firmware_version)
            );
            if let Some(revision) = current.revision {
                detail.push_str(&format!(" revision=0x{revision:02X}"));
            }

            Ok(CommandExecution {
                operation: "firmware version".to_string(),
                request: Some(vec![cmd::GET_USB_VERSION]),
                checksum: Some(checksum_name(CheckSumType::Bit7 as i32).to_string()),
                response: Some(current.usb_raw),
                detail: Some(detail),
            })
        }
        FirmwareCommands::Validate {
            file,
            download_dir,
            allow_unofficial,
        } => {
            emit_firmware_status("probing current keyboard firmware version");
            let current = read_device_firmware_state(transport, target).await?;
            emit_firmware_status(format!(
                "current_usb={} device_id={}",
                format_usb_version(current.firmware_version),
                current.device_id
            ));
            emit_firmware_status("resolving candidate firmware image");
            let resolved = resolve_firmware_input(
                target,
                file.as_ref(),
                download_dir.as_ref(),
                *allow_unofficial,
            )
            .await?;

            emit_firmware_status("running firmware preflight checks");
            let manifest = manifest_for_target(
                target,
                resolved.source.clone(),
                resolved.server_version_str.clone().or_else(|| {
                    resolved
                        .server_usb_version
                        .map(|version| format_usb_version(version))
                }),
            );
            let decision = evaluate_firmware_preflight(
                target,
                &resolved.firmware_path,
                manifest,
                FirmwarePreflightOptions {
                    allow_unofficial: *allow_unofficial,
                    assume_yes: true,
                    high_risk_ack: true,
                    typed_phrase: Some(REQUIRED_TYPED_PHRASE.to_string()),
                    backup_attempted: false,
                    backup_ok: true,
                    allow_backup_failure: false,
                },
            );
            if !decision.allowed {
                return Err(preflight_error(&decision));
            }
            emit_firmware_status("preflight checks passed");

            let summary = decision
                .manifest_summary
                .ok_or_else(|| anyhow!("preflight summary missing"))?;
            Ok(CommandExecution {
                operation: "firmware validate".to_string(),
                request: None,
                checksum: None,
                response: None,
                detail: Some(format!(
                    "preflight ok: size={} chunks={} checksum24=0x{:06X} current_usb={} target_usb={} update_available={} firmware_path={}{}{}",
                    summary.size_bytes,
                    summary.chunk_count,
                    summary.checksum_24,
                    format_usb_version(current.firmware_version),
                    resolved
                        .server_usb_version
                        .map(format_usb_version)
                        .unwrap_or_else(|| "unknown".to_string()),
                    resolved
                        .server_usb_version
                        .map(|server| server > current.firmware_version)
                        .map(|v| if v { "yes" } else { "no" })
                        .unwrap_or("n/a"),
                    resolved.firmware_path.display(),
                    resolved
                        .downloaded_bytes
                        .map(|size| format!(" downloaded_bytes={size}"))
                        .unwrap_or_default(),
                    resolved
                        .download_path
                        .as_ref()
                        .map(|path| format!(" vendor_path={path}"))
                        .unwrap_or_default(),
                )),
            })
        }
        FirmwareCommands::Flash {
            file,
            download_dir,
            allow_unofficial,
            yes,
            i_understand_firmware_risk,
            typed_phrase,
            allow_backup_failure,
        } => {
            emit_firmware_status("probing current keyboard firmware version");
            let current = read_device_firmware_state(transport, target).await?;
            emit_firmware_status(format!(
                "current_usb={} device_id={}",
                format_usb_version(current.firmware_version),
                current.device_id
            ));
            emit_firmware_status("resolving candidate firmware image");
            let resolved = resolve_firmware_input(
                target,
                file.as_ref(),
                download_dir.as_ref(),
                *allow_unofficial,
            )
            .await?;

            if let Some(server_usb_version) = resolved.server_usb_version
                && server_usb_version <= current.firmware_version
            {
                bail!(
                    "firmware approval rejected: current_usb={} target_usb={} (already up to date or newer). downloaded_firmware_path={}",
                    format_usb_version(current.firmware_version),
                    format_usb_version(server_usb_version),
                    resolved.firmware_path.display(),
                );
            }

            emit_firmware_status("running firmware preflight checks");
            let manifest = manifest_for_target(
                target,
                resolved.source.clone(),
                resolved.server_version_str.clone().or_else(|| {
                    resolved
                        .server_usb_version
                        .map(|version| format_usb_version(version))
                }),
            );
            let decision = evaluate_firmware_preflight(
                target,
                &resolved.firmware_path,
                manifest,
                FirmwarePreflightOptions {
                    allow_unofficial: *allow_unofficial,
                    assume_yes: *yes,
                    high_risk_ack: *i_understand_firmware_risk,
                    typed_phrase: typed_phrase.clone(),
                    backup_attempted: false,
                    backup_ok: true,
                    allow_backup_failure: *allow_backup_failure,
                },
            );
            if !decision.allowed {
                return Err(preflight_error(&decision));
            }
            emit_firmware_status("preflight checks passed");

            bail!(
                "firmware flash transport path is not implemented yet; preflight passed and no bootloader commands were sent; current_usb={} target_usb={} downloaded_firmware_path={}",
                format_usb_version(current.firmware_version),
                resolved
                    .server_usb_version
                    .map(format_usb_version)
                    .unwrap_or_else(|| "unknown".to_string()),
                resolved.firmware_path.display(),
            );
        }
    }
}

fn render_download_progress(progress: DownloadProgress, last_percent: &mut u64) {
    if let Some(total) = progress.total_bytes {
        if total > 0 {
            let pct = ((progress.downloaded_bytes as f64 / total as f64) * 100.0).floor() as u64;
            if pct != *last_percent || pct == 100 {
                *last_percent = pct;
                eprint!(
                    "\rDownloading firmware: {} / {} bytes ({}%)",
                    progress.downloaded_bytes, total, pct
                );
                let _ = std::io::stderr().flush();
            }
            return;
        }
    }

    eprint!(
        "\rDownloading firmware: {} bytes",
        progress.downloaded_bytes
    );
    let _ = std::io::stderr().flush();
}

fn checksum_name(value: i32) -> &'static str {
    if value == CheckSumType::Bit7 as i32 {
        "Bit7"
    } else if value == CheckSumType::Bit8 as i32 {
        "Bit8"
    } else if value == CheckSumType::None as i32 {
        "None"
    } else {
        "Unknown"
    }
}
