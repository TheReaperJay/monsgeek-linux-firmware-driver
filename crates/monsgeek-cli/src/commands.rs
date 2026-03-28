use std::future::Future;
use std::pin::Pin;
use std::path::Path;

use anyhow::{Result, anyhow, bail};
use monsgeek_firmware::{
    CompatibilityCheck, FirmwareManifest, FirmwareSource, FirmwareTarget, PreflightDecision,
    PreflightRequest, REQUIRED_TYPED_PHRASE, run_preflight,
};
use monsgeek_driver::pb::driver::CheckSumType;
use monsgeek_protocol::{DeviceDefinition, cmd};
use monsgeek_transport::usb::UsbVersionInfo;
use serde::Serialize;

use crate::client::DriverClient;
use crate::device_select::preferred_model_slug;
use crate::{
    Commands, DebounceCommands, KeymapCommands, LedCommands, MacroCommands, PollCommands,
    ProfileCommands, RawCommands, FirmwareCommands,
};
use crate::device_select::ResolvedTargetDevice;

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
        transport.send_msg(target.path.clone(), request, checksum).await?;
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
                let mut frame = vec![table.set_macro, *macro_index, *page, chunk_len, *is_last, 0, 0];
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
    let request = PreflightRequest {
        device_id: target.device_id as u32,
        model_slug: preferred_model_slug(&target.definition),
        device_path: Some(target.path.clone()),
        firmware_source: FirmwareSource::LocalFile {
            path: file.display().to_string(),
        },
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

fn manifest_for_target(target: &ResolvedTargetDevice, file: &Path) -> FirmwareManifest {
    FirmwareManifest {
        format_version: Some("1".to_string()),
        firmware_version: None,
        target: FirmwareTarget {
            device_id: Some(target.device_id as u32),
            model_slug: Some(preferred_model_slug(&target.definition)),
            board: None,
        },
        source: FirmwareSource::LocalFile {
            path: file.display().to_string(),
        },
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

async fn execute_firmware_command<T: DriverTransport>(
    transport: &mut T,
    target: &ResolvedTargetDevice,
    command: &FirmwareCommands,
) -> Result<CommandExecution> {
    match command {
        FirmwareCommands::Version => {
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

            let mut detail = format!(
                "device_id={} firmware_version=0x{:04X}",
                usb_info.device_id, usb_info.firmware_version
            );

            // GET_REV is optional and firmware-dependent; best-effort only.
            transport
                .send_msg(
                    target.path.clone(),
                    vec![cmd::GET_REV],
                    CheckSumType::Bit7 as i32,
                )
                .await?;
            if let Ok(rev_raw) = transport.read_msg(target.path.clone()).await
                && rev_raw.len() >= 2
                && rev_raw[0] == cmd::GET_REV
            {
                detail.push_str(&format!(" revision=0x{:02X}", rev_raw[1]));
            }

            Ok(CommandExecution {
                operation: "firmware version".to_string(),
                request: Some(vec![cmd::GET_USB_VERSION]),
                checksum: Some(checksum_name(CheckSumType::Bit7 as i32).to_string()),
                response: Some(usb_raw),
                detail: Some(detail),
            })
        }
        FirmwareCommands::Validate {
            file,
            allow_unofficial,
        } => {
            let manifest = manifest_for_target(target, file);
            let decision = evaluate_firmware_preflight(
                target,
                file,
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

            let summary = decision
                .manifest_summary
                .ok_or_else(|| anyhow!("preflight summary missing"))?;
            Ok(CommandExecution {
                operation: "firmware validate".to_string(),
                request: None,
                checksum: None,
                response: None,
                detail: Some(format!(
                    "preflight ok: size={} chunks={} checksum24=0x{:06X}",
                    summary.size_bytes, summary.chunk_count, summary.checksum_24
                )),
            })
        }
        FirmwareCommands::Flash {
            file,
            allow_unofficial,
            yes,
            i_understand_firmware_risk,
            typed_phrase,
            allow_backup_failure,
        } => {
            let manifest = manifest_for_target(target, file);
            let decision = evaluate_firmware_preflight(
                target,
                file,
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

            bail!(
                "firmware flash transport path is not implemented yet; preflight passed and no bootloader commands were sent"
            );
        }
    }
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
