use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::engine::{lower_24_bits, padded_checksum_64};
use crate::manifest::FirmwareManifest;

pub const REQUIRED_TYPED_PHRASE: &str = "FLASH M5W";

#[derive(Debug, Clone)]
pub struct PreflightRequest {
    pub firmware_path: PathBuf,
    pub manifest: FirmwareManifest,
    pub allow_unofficial: bool,
    pub assume_yes: bool,
    pub high_risk_ack: bool,
    pub typed_phrase: Option<String>,
    pub expected_device_id: Option<u32>,
    pub expected_model_slug: Option<String>,
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
