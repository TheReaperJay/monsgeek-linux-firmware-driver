use std::path::PathBuf;

use monsgeek_cli::commands::{self, FirmwarePreflightOptions};
use monsgeek_cli::device_select::{self, ResolvedTargetDevice};
use monsgeek_cli::{Commands, FirmwareCommands};
use monsgeek_firmware::{CompatibilityCheck, FirmwareManifest, FirmwareSource, FirmwareTarget};
use monsgeek_protocol::{DeviceDefinition, DeviceRegistry, cmd};

fn load_registry() -> DeviceRegistry {
    device_select::load_registry().expect("registry should load for firmware tests")
}

fn resolved_target(definition: &DeviceDefinition) -> ResolvedTargetDevice {
    ResolvedTargetDevice {
        path: "3151-4015-ffff-0002-1".to_string(),
        device_id: definition.id,
        vid: definition.vid,
        pid: definition.pid,
        definition: definition.clone(),
    }
}

fn temp_image(bytes: &[u8]) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time should be monotonic enough for test temp paths")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("monsgeek-cli-firmware-{nanos}.bin"));
    std::fs::write(&path, bytes).expect("should write temp firmware image");
    path
}

fn manifest_for(target: &ResolvedTargetDevice, expected_model_slug: &str) -> FirmwareManifest {
    FirmwareManifest {
        format_version: Some("1".to_string()),
        firmware_version: Some("2.3.4".to_string()),
        target: FirmwareTarget {
            device_id: Some(target.device_id as u32),
            model_slug: Some(device_select::preferred_model_slug(&target.definition)),
            board: None,
        },
        source: FirmwareSource::LocalFile {
            path: "fixture.bin".to_string(),
        },
        compatibility: CompatibilityCheck {
            expected_device_id: Some(target.device_id as u32),
            expected_model_slug: Some(expected_model_slug.to_string()),
            expected_revision: None,
            min_revision: None,
            max_revision: None,
        },
        metadata_checksum: None,
        image_size_bytes: None,
    }
}

#[test]
fn version_builds_get_usb_version_request() {
    let registry = load_registry();
    let definition = registry
        .find_by_id(1308)
        .expect("m5w definition exists")
        .clone();

    let plan = commands::build_command_request(
        &Commands::Firmware {
            command: FirmwareCommands::Version,
        },
        &definition,
        false,
    )
    .expect("firmware version should build request");

    assert_eq!(
        plan.request.expect("request should be set")[0],
        cmd::GET_USB_VERSION
    );
}

#[test]
fn flash_requires_typed_phrase_when_interactive() {
    let registry = load_registry();
    let definition = registry
        .find_by_id(1308)
        .expect("m5w definition exists")
        .clone();
    let target = resolved_target(&definition);
    let image = temp_image(&[1, 2, 3, 4]);

    let decision = commands::evaluate_firmware_preflight(
        &target,
        &image,
        manifest_for(&target, &device_select::preferred_model_slug(&definition)),
        FirmwarePreflightOptions {
            allow_unofficial: false,
            assume_yes: false,
            high_risk_ack: false,
            typed_phrase: None,
            backup_attempted: false,
            backup_ok: true,
            allow_backup_failure: false,
        },
    );

    assert!(!decision.allowed);
    assert!(
        decision
            .errors
            .iter()
            .any(|error| error.contains("typed phrase mismatch"))
    );

    let _ = std::fs::remove_file(image);
}

#[test]
fn flash_requires_dual_non_interactive_flags() {
    let registry = load_registry();
    let definition = registry
        .find_by_id(1308)
        .expect("m5w definition exists")
        .clone();
    let target = resolved_target(&definition);
    let image = temp_image(&[1, 2, 3, 4]);

    let decision = commands::evaluate_firmware_preflight(
        &target,
        &image,
        manifest_for(&target, &device_select::preferred_model_slug(&definition)),
        FirmwarePreflightOptions {
            allow_unofficial: false,
            assume_yes: true,
            high_risk_ack: false,
            typed_phrase: None,
            backup_attempted: false,
            backup_ok: true,
            allow_backup_failure: false,
        },
    );

    assert!(!decision.allowed);
    assert!(
        decision
            .errors
            .iter()
            .any(|error| error.contains("high-risk flag required"))
    );

    let _ = std::fs::remove_file(image);
}

#[test]
fn validate_blocks_metadata_mismatch_without_override() {
    let registry = load_registry();
    let definition = registry
        .find_by_id(1308)
        .expect("m5w definition exists")
        .clone();
    let target = resolved_target(&definition);
    let image = temp_image(&[1, 2, 3, 4]);

    let decision = commands::evaluate_firmware_preflight(
        &target,
        &image,
        manifest_for(&target, "different-model"),
        FirmwarePreflightOptions {
            allow_unofficial: false,
            assume_yes: true,
            high_risk_ack: true,
            typed_phrase: Some("FLASH M5W".to_string()),
            backup_attempted: false,
            backup_ok: true,
            allow_backup_failure: false,
        },
    );

    assert!(!decision.allowed);
    assert!(
        decision
            .errors
            .iter()
            .any(|error| error.contains("metadata mismatch"))
    );

    let _ = std::fs::remove_file(image);
}

#[test]
fn validate_allows_unofficial_when_flag_set() {
    let registry = load_registry();
    let definition = registry
        .find_by_id(1308)
        .expect("m5w definition exists")
        .clone();
    let target = resolved_target(&definition);
    let image = temp_image(&[1, 2, 3, 4]);

    let decision = commands::evaluate_firmware_preflight(
        &target,
        &image,
        manifest_for(&target, "different-model"),
        FirmwarePreflightOptions {
            allow_unofficial: true,
            assume_yes: true,
            high_risk_ack: true,
            typed_phrase: Some("FLASH M5W".to_string()),
            backup_attempted: false,
            backup_ok: true,
            allow_backup_failure: false,
        },
    );

    assert!(decision.allowed);
    assert!(decision.errors.is_empty());

    let _ = std::fs::remove_file(image);
}

#[test]
fn firmware_version_builds_get_usb_version_request() {
    version_builds_get_usb_version_request();
}

#[test]
fn firmware_flash_requires_typed_phrase_when_interactive() {
    flash_requires_typed_phrase_when_interactive();
}

#[test]
fn firmware_flash_requires_dual_non_interactive_flags() {
    flash_requires_dual_non_interactive_flags();
}

#[test]
fn firmware_validate_blocks_metadata_mismatch_without_override() {
    validate_blocks_metadata_mismatch_without_override();
}

#[test]
fn firmware_validate_allows_unofficial_when_flag_set() {
    validate_allows_unofficial_when_flag_set();
}
