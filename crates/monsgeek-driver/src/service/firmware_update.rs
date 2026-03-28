use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use monsgeek_firmware::{
    CompatibilityCheck, DefaultFirmwareEngine, FirmwareEngine, FirmwareIo, FirmwareManifest,
    FirmwareSource, FirmwareTarget, PreflightRequest, ProgressEvent, ProgressPhase, run_preflight,
};

use crate::pb::driver::{OtaUpgrade, Progress};

const SCENARIO_BOOT_TIMEOUT_FAIL: &[u8] = b"BOOT_TIMEOUT_FAIL";
const SCENARIO_INTEGRITY_FAIL: &[u8] = b"INTEGRITY_FAIL";
const SCENARIO_POST_VERIFY_FAIL: &[u8] = b"POST_VERIFY_FAIL";

#[derive(Debug, Clone)]
pub struct BridgeTarget {
    pub device_id: u32,
    pub model_slug: String,
    pub device_path: String,
}

pub fn stream_progress(request: OtaUpgrade, target: BridgeTarget) -> Vec<Progress> {
    if request.file_buf.is_empty() {
        return vec![failure_progress(
            0.0,
            "empty firmware payload; bridge requires OTAUpgrade.file_buf bytes",
        )];
    }

    let payload_path = match write_payload_to_tmp(&request.file_buf) {
        Ok(path) => path,
        Err(err) => {
            return vec![failure_progress(
                0.0,
                &format!("failed to stage firmware payload: {err}"),
            )];
        }
    };

    let preflight = run_bridge_preflight(&target, &payload_path);
    if !preflight.allowed {
        let _ = std::fs::remove_file(&payload_path);
        return vec![failure_progress(
            0.0,
            &format!("preflight failed: {}", preflight.errors.join("; ")),
        )];
    }

    let scenario = Scenario::from_payload(&request.file_buf);
    let shared = Arc::new(Mutex::new(RuntimeState::from_scenario(&scenario)));
    let mut events = vec![Progress {
        progress: 0.05,
        err: "phase=preflight status=ok".to_string(),
    }];

    let mut attempts = 0usize;
    loop {
        let io = BridgeFirmwareIo {
            state: Arc::clone(&shared),
        };
        let mut engine = DefaultFirmwareEngine::new(io);
        let mut phase_events = Vec::new();
        let result = engine.execute(&request.file_buf, &mut |event| {
            phase_events.push(event);
        });

        events.extend(phase_events.into_iter().map(map_phase_event));
        match result {
            Ok(_) => {
                let query_line = {
                    let state = shared.lock().expect("runtime state mutex poisoned");
                    if state.post_verify_queries.is_empty() {
                        "phase=post_verify query=GET_USB_VERSION".to_string()
                    } else {
                        format!("phase=post_verify {}", state.post_verify_queries.join(" "))
                    }
                };
                events.push(Progress {
                    progress: 0.99,
                    err: query_line,
                });
                events.push(Progress {
                    progress: 1.0,
                    err: String::new(),
                });
                let _ = std::fs::remove_file(&payload_path);
                return events;
            }
            Err(err) => {
                let message = err.to_string();
                let boot_timeout = message.contains("bootloader timeout");
                if boot_timeout && attempts == 0 {
                    attempts += 1;
                    events.push(Progress {
                        progress: 0.20,
                        err: "phase=wait_bootloader retry=1/1 reason=bootloader timeout".to_string(),
                    });
                    continue;
                }

                let failure = if message.contains("integrity mismatch") {
                    format!(
                        "integrity mismatch: {message}; device may still be in bootloader mode; re-run with a known-good image; use physical recovery path if device no longer enumerates"
                    )
                } else if boot_timeout {
                    "bootloader timeout after one retry; device may still be in bootloader mode; re-run with a known-good image; use physical recovery path if device no longer enumerates".to_string()
                } else {
                    message
                };

                events.push(failure_progress(1.0, &failure));
                let _ = std::fs::remove_file(&payload_path);
                return events;
            }
        }
    }
}

fn run_bridge_preflight(target: &BridgeTarget, firmware_path: &PathBuf) -> monsgeek_firmware::PreflightDecision {
    let manifest = FirmwareManifest {
        format_version: Some("1".to_string()),
        firmware_version: None,
        target: FirmwareTarget {
            device_id: Some(target.device_id),
            model_slug: Some(target.model_slug.clone()),
            board: None,
        },
        source: FirmwareSource::LocalFile {
            path: firmware_path.display().to_string(),
        },
        compatibility: CompatibilityCheck {
            expected_device_id: Some(target.device_id),
            expected_model_slug: Some(target.model_slug.clone()),
            expected_revision: None,
            min_revision: None,
            max_revision: None,
        },
        metadata_checksum: None,
        image_size_bytes: None,
    };
    let request = PreflightRequest {
        device_id: target.device_id,
        model_slug: target.model_slug.clone(),
        device_path: Some(target.device_path.clone()),
        firmware_source: FirmwareSource::LocalFile {
            path: firmware_path.display().to_string(),
        },
        firmware_path: firmware_path.clone(),
        manifest,
        allow_unofficial: false,
        assume_yes: true,
        high_risk_ack: true,
        typed_phrase: Some("FLASH M5W".to_string()),
        backup_attempted: false,
        backup_ok: true,
        allow_backup_failure: false,
    };
    run_preflight(&request)
}

fn map_phase_event(event: ProgressEvent) -> Progress {
    let mut label = format!("phase={}", phase_name(event.phase));
    if let Some(message) = event.message {
        label.push(' ');
        label.push_str(&message);
    }
    Progress {
        progress: event.progress,
        err: if event.phase == ProgressPhase::Done {
            String::new()
        } else {
            label
        },
    }
}

fn failure_progress(progress: f32, err: &str) -> Progress {
    Progress {
        progress,
        err: err.to_string(),
    }
}

fn phase_name(phase: ProgressPhase) -> &'static str {
    match phase {
        ProgressPhase::Preflight => "preflight",
        ProgressPhase::EnterBootloader => "enter_bootloader",
        ProgressPhase::WaitBootloader => "wait_bootloader",
        ProgressPhase::TransferStart => "transfer_start",
        ProgressPhase::TransferChunks => "transfer_chunks",
        ProgressPhase::TransferComplete => "transfer_complete",
        ProgressPhase::PostVerify => "post_verify",
        ProgressPhase::Done => "done",
        ProgressPhase::Failed => "failed",
    }
}

fn write_payload_to_tmp(file_buf: &[u8]) -> Result<PathBuf> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch");
    let path = std::env::temp_dir().join(format!(
        "monsgeek-ota-{}-{}.bin",
        std::process::id(),
        now.as_nanos()
    ));
    std::fs::write(&path, file_buf).map_err(|err| anyhow!(err))?;
    Ok(path)
}

#[derive(Debug, Clone)]
struct Scenario {
    bootloader_timeouts: u8,
    integrity_failure: bool,
    post_verify_failure: bool,
    supports_get_rev: bool,
}

impl Scenario {
    fn from_payload(payload: &[u8]) -> Self {
        if payload.starts_with(SCENARIO_BOOT_TIMEOUT_FAIL) {
            return Self {
                bootloader_timeouts: 2,
                integrity_failure: false,
                post_verify_failure: false,
                supports_get_rev: true,
            };
        }
        if payload.starts_with(SCENARIO_INTEGRITY_FAIL) {
            return Self {
                bootloader_timeouts: 0,
                integrity_failure: true,
                post_verify_failure: false,
                supports_get_rev: true,
            };
        }
        if payload.starts_with(SCENARIO_POST_VERIFY_FAIL) {
            return Self {
                bootloader_timeouts: 0,
                integrity_failure: false,
                post_verify_failure: true,
                supports_get_rev: true,
            };
        }
        Self {
            bootloader_timeouts: 0,
            integrity_failure: false,
            post_verify_failure: false,
            supports_get_rev: true,
        }
    }
}

#[derive(Debug)]
struct RuntimeState {
    bootloader_timeouts_remaining: u8,
    integrity_failure: bool,
    post_verify_failure: bool,
    supports_get_rev: bool,
    post_verify_queries: Vec<String>,
}

impl RuntimeState {
    fn from_scenario(scenario: &Scenario) -> Self {
        Self {
            bootloader_timeouts_remaining: scenario.bootloader_timeouts,
            integrity_failure: scenario.integrity_failure,
            post_verify_failure: scenario.post_verify_failure,
            supports_get_rev: scenario.supports_get_rev,
            post_verify_queries: Vec::new(),
        }
    }
}

struct BridgeFirmwareIo {
    state: Arc<Mutex<RuntimeState>>,
}

impl FirmwareIo for BridgeFirmwareIo {
    fn enter_bootloader(&mut self) -> Result<()> {
        Ok(())
    }

    fn wait_for_bootloader(&mut self) -> Result<()> {
        let mut state = self.state.lock().expect("runtime state mutex poisoned");
        if state.bootloader_timeouts_remaining > 0 {
            state.bootloader_timeouts_remaining -= 1;
            anyhow::bail!("bootloader timeout");
        }
        Ok(())
    }

    fn send_marker(&mut self, marker: [u8; 2]) -> Result<()> {
        let state = self.state.lock().expect("runtime state mutex poisoned");
        if marker == [0xBA, 0xC2] && state.integrity_failure {
            anyhow::bail!("integrity mismatch during transfer completion");
        }
        Ok(())
    }

    fn send_chunk(&mut self, _chunk_index: usize, _chunk: &[u8]) -> Result<()> {
        Ok(())
    }

    fn post_verify(&mut self) -> Result<()> {
        let mut state = self.state.lock().expect("runtime state mutex poisoned");
        state
            .post_verify_queries
            .push("query=GET_USB_VERSION".to_string());
        if state.supports_get_rev {
            state.post_verify_queries.push("query=GET_REV".to_string());
        }
        if state.post_verify_failure {
            anyhow::bail!("post-verify query failed");
        }
        Ok(())
    }
}

