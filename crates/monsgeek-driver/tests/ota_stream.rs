use monsgeek_driver::pb::driver::driver_grpc_server::DriverGrpc;
use monsgeek_driver::pb::driver::{OtaUpgrade, Progress};
use monsgeek_driver::{DriverFlags, DriverService};
use tokio_stream::StreamExt;
use tonic::Request;

async fn collect_progress(
    service: &DriverService,
    payload: Vec<u8>,
) -> Result<Vec<Progress>, tonic::Status> {
    let response = DriverGrpc::upgrade_otagatt(
        service,
        Request::new(OtaUpgrade {
            dev_path: "3151-4015-ffff-0002-1@id1308-b003-a003-n1".to_string(),
            file_buf: payload,
        }),
    )
    .await?;
    let mut stream = response.into_inner();
    let mut out = Vec::new();
    while let Some(item) = stream.next().await {
        out.push(item.expect("progress item should be Ok"));
    }
    Ok(out)
}

#[tokio::test]
async fn upgrade_otagatt_requires_enable_flag() {
    let service = DriverService::new();
    let response = DriverGrpc::upgrade_otagatt(
        &service,
        Request::new(OtaUpgrade {
            dev_path: "3151-4015-ffff-0002-1@id1308-b003-a003-n1".to_string(),
            file_buf: vec![1, 2, 3, 4],
        }),
    )
    .await;

    let err = match response {
        Ok(_) => panic!("OTA should be blocked when flag is disabled"),
        Err(err) => err,
    };

    assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    assert!(err.message().contains("--enable-ota"));
}

#[tokio::test]
async fn upgrade_otagatt_streams_phase_progress_in_order() {
    let service = DriverService::new_with_flags(DriverFlags { ota_enabled: true });
    let updates = collect_progress(&service, vec![1, 2, 3, 4, 5, 6, 7, 8])
        .await
        .expect("OTA stream should succeed");

    let phases: Vec<&str> = updates
        .iter()
        .filter(|item| !item.err.is_empty())
        .filter_map(|item| item.err.strip_prefix("phase="))
        .collect();

    let sequence = [
        "preflight",
        "enter_bootloader",
        "wait_bootloader",
        "transfer_start",
        "transfer_chunks",
        "transfer_complete",
        "post_verify",
    ];
    for phase in sequence {
        assert!(
            phases.iter().any(|candidate| candidate.starts_with(phase)),
            "expected phase {} in stream; got {:?}",
            phase,
            phases
        );
    }

    let last = updates.last().expect("stream should have final item");
    assert!(
        last.err.is_empty(),
        "terminal success item must have empty err field"
    );
}

#[tokio::test]
async fn upgrade_otagatt_retries_bootloader_once_then_fails() {
    let service = DriverService::new_with_flags(DriverFlags { ota_enabled: true });
    let updates = collect_progress(&service, b"BOOT_TIMEOUT_FAIL".to_vec())
        .await
        .expect("stream should return progress items");

    assert!(
        updates
            .iter()
            .any(|item| item.err.contains("retry=1/1") && item.err.contains("wait_bootloader")),
        "expected a single retry progress update"
    );
    let final_err = updates
        .last()
        .expect("stream should have terminal failure")
        .err
        .clone();
    assert!(final_err.contains("bootloader timeout"));
}

#[tokio::test]
async fn upgrade_otagatt_reports_recovery_guidance_on_integrity_failure() {
    let service = DriverService::new_with_flags(DriverFlags { ota_enabled: true });
    let updates = collect_progress(&service, b"INTEGRITY_FAIL".to_vec())
        .await
        .expect("stream should return progress items");

    let final_err = updates
        .last()
        .expect("stream should end with failure")
        .err
        .clone();
    assert!(final_err.contains("device may still be in bootloader mode"));
    assert!(final_err.contains("re-run with a known-good image"));
    assert!(final_err.contains("use physical recovery path if device no longer enumerates"));
}

#[tokio::test]
async fn upgrade_otagatt_success_requires_post_verify_queries() {
    let service = DriverService::new_with_flags(DriverFlags { ota_enabled: true });
    let updates = collect_progress(&service, vec![9, 8, 7, 6, 5, 4, 3, 2])
        .await
        .expect("stream should succeed");

    let post_verify = updates
        .iter()
        .find(|item| item.err.contains("phase=post_verify query=GET_USB_VERSION"))
        .expect("post-verify query line should exist");
    assert!(post_verify.err.contains("GET_USB_VERSION"));
    assert!(post_verify.err.contains("GET_REV"));

    let final_item = updates.last().expect("stream should include terminal item");
    assert!(
        final_item.err.is_empty(),
        "success should be emitted only after post-verify event"
    );
}

#[tokio::test]
async fn ota_upgrade_otagatt_requires_enable_flag() {
    let service = DriverService::new();
    let response = DriverGrpc::upgrade_otagatt(
        &service,
        Request::new(OtaUpgrade {
            dev_path: "3151-4015-ffff-0002-1@id1308-b003-a003-n1".to_string(),
            file_buf: vec![1, 2, 3, 4],
        }),
    )
    .await;
    let err = match response {
        Ok(_) => panic!("OTA should be blocked when flag is disabled"),
        Err(err) => err,
    };
    assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    assert!(err.message().contains("--enable-ota"));
}

#[tokio::test]
async fn ota_upgrade_otagatt_streams_phase_progress_in_order() {
    let service = DriverService::new_with_flags(DriverFlags { ota_enabled: true });
    let updates = collect_progress(&service, vec![1, 2, 3, 4, 5, 6, 7, 8])
        .await
        .expect("OTA stream should succeed");
    let phases: Vec<&str> = updates
        .iter()
        .filter(|item| !item.err.is_empty())
        .filter_map(|item| item.err.strip_prefix("phase="))
        .collect();
    assert!(phases.iter().any(|candidate| candidate.starts_with("preflight")));
    assert!(phases.iter().any(|candidate| candidate.starts_with("enter_bootloader")));
    assert!(phases.iter().any(|candidate| candidate.starts_with("wait_bootloader")));
    assert!(phases.iter().any(|candidate| candidate.starts_with("transfer_start")));
    assert!(phases.iter().any(|candidate| candidate.starts_with("transfer_chunks")));
    assert!(phases.iter().any(|candidate| candidate.starts_with("transfer_complete")));
    assert!(phases.iter().any(|candidate| candidate.starts_with("post_verify")));
    let last = updates.last().expect("stream should have final item");
    assert!(last.err.is_empty());
}

#[tokio::test]
async fn ota_upgrade_otagatt_retries_bootloader_once_then_fails() {
    let service = DriverService::new_with_flags(DriverFlags { ota_enabled: true });
    let updates = collect_progress(&service, b"BOOT_TIMEOUT_FAIL".to_vec())
        .await
        .expect("stream should return progress items");
    assert!(
        updates
            .iter()
            .any(|item| item.err.contains("retry=1/1") && item.err.contains("wait_bootloader"))
    );
    let final_err = updates.last().expect("stream should have terminal failure").err.clone();
    assert!(final_err.contains("bootloader timeout"));
}

#[tokio::test]
async fn ota_upgrade_otagatt_reports_recovery_guidance_on_integrity_failure() {
    let service = DriverService::new_with_flags(DriverFlags { ota_enabled: true });
    let updates = collect_progress(&service, b"INTEGRITY_FAIL".to_vec())
        .await
        .expect("stream should return progress items");
    let final_err = updates.last().expect("stream should end with failure").err.clone();
    assert!(final_err.contains("device may still be in bootloader mode"));
    assert!(final_err.contains("re-run with a known-good image"));
    assert!(final_err.contains("use physical recovery path if device no longer enumerates"));
}

#[tokio::test]
async fn ota_upgrade_otagatt_success_requires_post_verify_queries() {
    let service = DriverService::new_with_flags(DriverFlags { ota_enabled: true });
    let updates = collect_progress(&service, vec![9, 8, 7, 6, 5, 4, 3, 2])
        .await
        .expect("stream should succeed");
    let post_verify = updates
        .iter()
        .find(|item| item.err.contains("phase=post_verify query=GET_USB_VERSION"))
        .expect("post-verify query line should exist");
    assert!(post_verify.err.contains("GET_USB_VERSION"));
    assert!(post_verify.err.contains("GET_REV"));
    let final_item = updates.last().expect("stream should include terminal item");
    assert!(final_item.err.is_empty());
}
