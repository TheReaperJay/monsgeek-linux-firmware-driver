---
phase: 07-cli-service-deployment
plan: 01
subsystem: cli-bridge
tags: [cli, grpc, registry, selector, safety-gate]

requires:
  - phase: 06-macros-device-specific-advanced-features
    provides: bridge contract and registry-driven command behavior
provides:
  - New `monsgeek-cli` bridge-first binary crate
  - Deterministic device target selection (`--path` > `--device-id` > `--model` > single-device auto-pick)
  - Typed command-to-proto framing that resolves family-divergent bytes from `DeviceDefinition::commands()`
  - Raw write gate requiring `--unsafe`
  - CLI smoke/parser contract tests
affects: [07-02-service-smoke, operator-workflows, bridge-runtime-usage]

tech-stack:
  added: [clap, tokio, tonic, serde, serde_json, anyhow]
  patterns: [bridge-first-cli, registry-driven-selection, explicit-unsafe-gating]

key-files:
  created:
    - crates/monsgeek-cli/Cargo.toml
    - crates/monsgeek-cli/src/main.rs
    - crates/monsgeek-cli/src/client.rs
    - crates/monsgeek-cli/src/device_select.rs
    - crates/monsgeek-cli/src/commands.rs
    - crates/monsgeek-cli/src/format.rs
    - crates/monsgeek-cli/tests/cli_smoke.rs
    - .planning/phases/07-cli-service-deployment/07-01-SUMMARY.md
  modified:
    - Cargo.lock

key-decisions:
  - "CLI stays bridge-first on `http://127.0.0.1:3814`; no direct USB mode in this phase."
  - "Selector ambiguity must fail with explicit guidance instead of implicit random target choice."
  - "Raw write opcodes (`< 0x80`) require `--unsafe`; raw read remains safe by default."

patterns-established:
  - "CLI command framing uses protocol constants + per-device command table overrides from registry definitions."
  - "Device model resolution is registry-driven and can use slug aliases."
  - "Every command supports both human output and JSON output."

requirements-completed: [CLI-01, CLI-02]

duration: in-session
completed: 2026-03-28
---

# Phase 07 Plan 01 Summary

**Implemented a production-ready bridge-first `monsgeek-cli` crate with typed command surface, deterministic selectors, and unsafe-raw protection aligned to the existing DriverGrpc and registry contracts.**

## Performance

- **Tasks completed:** 3/3 (static acceptance inspection)
- **Files created/modified:** 8
- **Execution mode:** mixed (executor + inline completion)

## Accomplishments

- Added `monsgeek-cli` crate and command surface:
  - `devices list`, `info`, `led get/set`, `debounce get/set`, `poll get/set`, `profile get/set`, `keymap get/set`, `macro get/set`, `raw send/read`
- Implemented gRPC client wrapper for `watch_dev_list`, `send_msg`, and `read_msg`.
- Added deterministic selector resolution precedence and clear ambiguity errors.
- Implemented typed request framing with protocol constants and `DeviceDefinition::commands()` for family divergence.
- Added JSON/human output formatting helpers for devices and command results.
- Added CLI smoke tests for parser coverage, selector behavior, raw gate, and command framing behavior with a stub transport.

## Verification Evidence

Static checks in this environment confirmed required artifacts/patterns:
- `name = "monsgeek-cli"` present in `crates/monsgeek-cli/Cargo.toml`
- `DriverGrpcClient` present in `src/client.rs`
- endpoint default `http://127.0.0.1:3814` present in `src/main.rs`
- `resolve_target_device` present with selector precedence and ambiguity guidance
- `definition.commands()` used in command mapping
- `multiple_devices_requires_selector` test present
- raw write gate emits `--unsafe` requirement

## Issues Encountered

- Shell environment is mounted read-only, which blocks:
  - `cargo check -p monsgeek-cli`
  - `cargo test -p monsgeek-cli`
  - atomic git commits and gsd-tools state/roadmap updates

Because of this, automated compile/test execution and commit hashes were not recorded in this run.

## Deviations from Plan

None in feature scope. Only execution-environment limitations prevented runtime verification and commit recording.

## Next Phase Readiness

- Plan 02 can consume the CLI immediately for service smoke checks.
- Human verification checkpoint remains required for service lifecycle approval.
