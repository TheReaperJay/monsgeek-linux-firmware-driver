---
phase: 03
slug: grpc-web-bridge
status: draft
nyquist_compliant: false
wave_0_complete: true
created: 2026-03-23
---

# Phase 03 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness (`cargo test`) |
| **Config file** | none |
| **Quick run command** | `cargo test -p monsgeek-driver grpc_server_starts_http1 grpc_watch_dev_list_init_add_remove -- --nocapture` |
| **Full suite command** | `cargo test -p monsgeek-driver -- --nocapture` |
| **Estimated runtime** | ~120 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p monsgeek-driver grpc_get_version_shape -- --nocapture`
- **After every plan wave:** Run `cargo test -p monsgeek-driver -- --nocapture`
- **Before `$gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 03-01-01 | 01 | 1 | GRPC-01 | integration | `cargo test -p monsgeek-driver grpc_server_starts_http1 -- --nocapture` | ✅ | ✅ green |
| 03-01-02 | 01 | 1 | GRPC-07 | integration | `cargo test -p monsgeek-driver grpc_cors_headers_present -- --nocapture` | ✅ | ✅ green |
| 03-01-03 | 01 | 1 | GRPC-08 | compile/integration | `cargo test -p monsgeek-driver grpc_full_service_contract_present -- --nocapture` | ✅ | ✅ green |
| 03-02-01 | 02 | 2 | GRPC-02 | integration | `cargo test -p monsgeek-driver grpc_send_raw_feature_forwards -- --nocapture` | ✅ | ✅ green |
| 03-02-02 | 02 | 2 | GRPC-03 | integration | `cargo test -p monsgeek-driver grpc_read_raw_feature_returns_data -- --nocapture` | ✅ | ✅ green |
| 03-02-03 | 02 | 2 | GRPC-04 | integration stream | `cargo test -p monsgeek-driver grpc_watch_dev_list_init_add_remove -- --nocapture` | ✅ | ✅ green |
| 03-02-04 | 02 | 2 | GRPC-05 | unit | `cargo test -p monsgeek-driver grpc_get_version_shape -- --nocapture` | ✅ | ✅ green |
| 03-02-05 | 02 | 2 | GRPC-06 | integration | `cargo test -p monsgeek-driver grpc_db_insert_get_roundtrip -- --nocapture` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `crates/monsgeek-driver/tests/grpc_contract_tests.rs` - contract and full-service registration checks
- [x] `crates/monsgeek-driver/tests/grpc_watch_stream_tests.rs` - `watchDevList` stream behavior checks
- [x] `crates/monsgeek-driver/tests/grpc_db_tests.rs` - `insertDb/getItemFromDb` compatibility checks
- [x] `crates/monsgeek-driver/tests/mock_transport.rs` - deterministic bridge transport adapter tests
- [x] `crates/monsgeek-driver/build.rs` and generated proto module wiring

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Browser at `https://app.monsgeek.com` can connect to localhost bridge | GRPC-01, GRPC-07, GRPC-08 | Requires real browser + extension/runtime context | Start bridge, open configurator, verify no CORS/preflight failure and device list appears. **Status: pending user checkpoint** |
| Device list updates visible in UI after plug/unplug | GRPC-04 | UI observation needed in addition to stream unit tests | With configurator open, unplug/replug M5W and confirm add/remove behavior. **Status: pending user checkpoint** |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [ ] Feedback latency < 30s (watch stream tests currently ~55s due runtime startup probe)
- [ ] `nyquist_compliant: true` set in frontmatter (blocked on manual browser checkpoint)

**Approval:** pending user browser checkpoint for `https://app.monsgeek.com`
