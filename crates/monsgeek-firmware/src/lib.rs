pub mod engine;
pub mod manifest;
pub mod preflight;
pub mod progress;
pub mod vendor_api;

pub use engine::{
    CHUNK_SIZE, DefaultFirmwareEngine, FirmwareEngine, FirmwareIo, TRANSFER_COMPLETE_MARKER,
    TRANSFER_START_MARKER, lower_24_bits, padded_checksum_64,
};
pub use manifest::{CompatibilityCheck, FirmwareManifest, FirmwareSource, FirmwareTarget};
pub use preflight::{
    ManifestSummary, PreflightDecision, PreflightRequest, REQUIRED_TYPED_PHRASE, run_preflight,
};
pub use progress::{ProgressEvent, ProgressPhase};
pub use vendor_api::{
    API_BASE, DOWNLOAD_BASE, DownloadProgress, FirmwareCheckResponse, FirmwareVersions,
    VendorApiError, check_vendor_firmware, download_vendor_firmware,
    download_vendor_firmware_with_progress,
};
