use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::engine::{lower_24_bits, padded_checksum_64};
use crate::manifest::FirmwareManifest;
use crate::manifest::ManifestValidationError;
use crate::manifest::FirmwareSource;

pub const REQUIRED_TYPED_PHRASE: &str = "FLASH M5W";

#[derive(Debug, Clone)]
pub struct PreflightRequest {
    pub device_id: u32,
    pub model_slug: String,
    pub device_path: Option<String>,
    pub firmware_source: FirmwareSource,
    pub firmware_path: PathBuf,
    pub manifest: FirmwareManifest,
    pub allow_unofficial: bool,
    pub assume_yes: bool,
    pub high_risk_ack: bool,
    pub typed_phrase: Option<String>,
    pub backup_attempted: bool,
    pub backup_ok: bool,
    pub allow_backup_failure: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestSummary {
    pub size_bytes: usize,
    pub chunk_count: usize,
    pub checksum_24: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightDecision {
    pub allowed: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub manifest_summary: Option<ManifestSummary>,
}

impl PreflightDecision {
    pub fn rejected(errors: Vec<String>) -> Self {
        Self {
            allowed: false,
            errors,
            warnings: Vec::new(),
            manifest_summary: None,
        }
    }
}

pub fn validate_image(path: &Path) -> Result<Vec<u8>> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to read firmware image {}", path.display()))?;
    if bytes.is_empty() {
        anyhow::bail!("firmware image is empty");
    }
    Ok(bytes)
}

pub fn summarize_manifest_bytes(bytes: &[u8]) -> ManifestSummary {
    ManifestSummary {
        size_bytes: bytes.len(),
        chunk_count: bytes.len().div_ceil(64),
        checksum_24: lower_24_bits(padded_checksum_64(bytes)),
    }
}

pub fn run_preflight(request: &PreflightRequest) -> PreflightDecision {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let image = match validate_image(&request.firmware_path) {
        Ok(image) => Some(image),
        Err(err) => {
            errors.push(err.to_string());
            None
        }
    };

    if let Err(err) = request.manifest.validate_compatibility_fields() {
        errors.push(manifest_compatibility_error_message(err));
    }

    if let Some(image) = image.as_ref() {
        if let Err(message) = metadata_mismatch_error(request) {
            errors.push(message);
        }

        if !request.assume_yes {
            if request.typed_phrase.as_deref() != Some(REQUIRED_TYPED_PHRASE) {
                errors.push(format!(
                    "typed phrase mismatch: expected '{REQUIRED_TYPED_PHRASE}'"
                ));
            }
        } else if !request.high_risk_ack {
            errors.push("high-risk flag required for non-interactive flashing".to_string());
        }

        if request.backup_attempted && !request.backup_ok {
            if request.allow_backup_failure {
                warnings.push(
                    "backup attempt failed and override is enabled; flashing remains high risk"
                        .to_string(),
                );
            } else {
                errors.push(
                    "backup failed; pass allow_backup_failure override to continue".to_string(),
                );
            }
        }

        let summary = summarize_manifest_bytes(image);
        return PreflightDecision {
            allowed: errors.is_empty(),
            errors,
            warnings,
            manifest_summary: Some(summary),
        };
    }

    PreflightDecision {
        allowed: false,
        errors,
        warnings,
        manifest_summary: None,
    }
}

fn metadata_mismatch_error(request: &PreflightRequest) -> std::result::Result<(), String> {
    if request.allow_unofficial {
        return Ok(());
    }

    if let Some(expected_id) = request.manifest.compatibility.expected_device_id {
        if expected_id != request.device_id {
            return Err(format!(
                "metadata mismatch: expected device_id {expected_id}, got {}",
                request.device_id
            ));
        }
    }

    if let Some(expected_model) = request
        .manifest
        .compatibility
        .expected_model_slug
        .as_ref()
    {
        if expected_model.trim().to_ascii_lowercase() != request.model_slug.to_ascii_lowercase() {
            return Err(format!(
                "metadata mismatch: expected model '{expected_model}', got '{}'",
                request.model_slug
            ));
        }
    }

    Ok(())
}

fn manifest_compatibility_error_message(error: ManifestValidationError) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{CompatibilityCheck, FirmwareTarget};

    fn test_manifest() -> FirmwareManifest {
        FirmwareManifest {
            format_version: Some("1".to_string()),
            firmware_version: Some("2.3.4".to_string()),
            target: FirmwareTarget {
                device_id: Some(1308),
                model_slug: Some("m5w".to_string()),
                board: Some("yc3121".to_string()),
            },
            source: FirmwareSource::LocalFile {
                path: "/tmp/fw.bin".to_string(),
            },
            compatibility: CompatibilityCheck {
                expected_device_id: Some(1308),
                expected_model_slug: Some("m5w".to_string()),
                expected_revision: None,
                min_revision: None,
                max_revision: None,
            },
            metadata_checksum: None,
            image_size_bytes: None,
        }
    }

    fn write_temp_image(bytes: &[u8]) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be monotonic here")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("monsgeek-preflight-{nanos}.bin"));
        std::fs::write(&path, bytes).expect("should write image");
        path
    }

    fn base_request(path: PathBuf) -> PreflightRequest {
        PreflightRequest {
            device_id: 1308,
            model_slug: "m5w".to_string(),
            device_path: Some("3151-4015-001-001".to_string()),
            firmware_source: FirmwareSource::LocalFile {
                path: path.display().to_string(),
            },
            firmware_path: path,
            manifest: test_manifest(),
            allow_unofficial: false,
            assume_yes: false,
            high_risk_ack: false,
            typed_phrase: Some(REQUIRED_TYPED_PHRASE.to_string()),
            backup_attempted: false,
            backup_ok: false,
            allow_backup_failure: false,
        }
    }

    #[test]
    fn preflight_requires_typed_phrase_when_interactive() {
        let path = write_temp_image(&[1, 2, 3, 4]);
        let mut request = base_request(path.clone());
        request.typed_phrase = Some("WRONG".to_string());

        let decision = run_preflight(&request);
        assert!(!decision.allowed);
        assert!(
            decision
                .errors
                .iter()
                .any(|error| error.contains("typed phrase mismatch"))
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn preflight_requires_dual_non_interactive_flags() {
        let path = write_temp_image(&[1, 2, 3, 4]);
        let mut request = base_request(path.clone());
        request.assume_yes = true;
        request.high_risk_ack = false;

        let decision = run_preflight(&request);
        assert!(!decision.allowed);
        assert!(
            decision
                .errors
                .iter()
                .any(|error| error.contains("high-risk flag required"))
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn preflight_blocks_metadata_mismatch_without_override() {
        let path = write_temp_image(&[1, 2, 3, 4]);
        let mut request = base_request(path.clone());
        request.model_slug = "m1".to_string();
        request.allow_unofficial = false;

        let decision = run_preflight(&request);
        assert!(!decision.allowed);
        assert!(
            decision
                .errors
                .iter()
                .any(|error| error.contains("metadata mismatch"))
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn preflight_checksum_uses_ff_padding() {
        let path = write_temp_image(&[1, 2, 3]);
        let request = base_request(path.clone());
        let decision = run_preflight(&request);
        let summary = decision.manifest_summary.expect("summary should exist");
        assert_eq!(summary.size_bytes, 3);
        assert_eq!(summary.chunk_count, 1);
        assert_eq!(summary.checksum_24, 0x3CC9);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn preflight_backup_failure_requires_override() {
        let path = write_temp_image(&[1, 2, 3, 4]);
        let mut request = base_request(path.clone());
        request.backup_attempted = true;
        request.backup_ok = false;
        request.allow_backup_failure = false;

        let decision = run_preflight(&request);
        assert!(!decision.allowed);
        assert!(
            decision
                .errors
                .iter()
                .any(|error| error.contains("allow_backup_failure"))
        );

        let _ = std::fs::remove_file(path);
    }
}
