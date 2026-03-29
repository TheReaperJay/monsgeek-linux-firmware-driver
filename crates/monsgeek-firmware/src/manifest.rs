use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FirmwareManifest {
    pub format_version: Option<String>,
    pub firmware_version: Option<String>,
    pub target: FirmwareTarget,
    pub source: FirmwareSource,
    pub compatibility: CompatibilityCheck,
    pub metadata_checksum: Option<String>,
    pub image_size_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FirmwareTarget {
    pub device_id: Option<u32>,
    pub model_slug: Option<String>,
    pub board: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FirmwareSource {
    LocalFile {
        path: String,
    },
    VendorDownload {
        url: String,
        channel: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompatibilityCheck {
    pub expected_device_id: Option<u32>,
    pub expected_model_slug: Option<String>,
    pub expected_revision: Option<String>,
    pub min_revision: Option<String>,
    pub max_revision: Option<String>,
}

impl FirmwareManifest {
    pub fn from_json_str(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("failed to parse firmware manifest JSON")
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read manifest file at {}", path.display()))?;
        Self::from_json_str(&raw)
    }

    pub fn validate_compatibility_fields(
        &self,
    ) -> std::result::Result<(), ManifestValidationError> {
        if self.compatibility.expected_device_id.is_none() {
            return Err(ManifestValidationError::MissingExpectedDeviceId);
        }

        let Some(model_slug) = self.compatibility.expected_model_slug.as_ref() else {
            return Err(ManifestValidationError::MissingExpectedModelSlug);
        };
        if model_slug.trim().is_empty() {
            return Err(ManifestValidationError::EmptyExpectedModelSlug);
        }

        Ok(())
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ManifestValidationError {
    #[error("manifest compatibility.expected_device_id is required")]
    MissingExpectedDeviceId,
    #[error("manifest compatibility.expected_model_slug is required")]
    MissingExpectedModelSlug,
    #[error("manifest compatibility.expected_model_slug cannot be empty")]
    EmptyExpectedModelSlug,
}
