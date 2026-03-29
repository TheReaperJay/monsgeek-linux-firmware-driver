# Phase 7: CLI & Service Deployment - Research

**Researched:** 2026-03-27
**Domain:** Rust CLI over existing DriverGrpc contract, registry-driven device targeting, and Linux systemd service packaging
**Confidence:** HIGH

## Summary

Phase 7 should be implemented as packaging and operation-layer work, not protocol invention. The bridge already exposes the canonical command path (`sendMsg`/`readMsg`) with policy checks and normalization, and `monsgeek-protocol` already contains command tables plus JSON registry loading. The safest approach is a bridge-first CLI (`monsgeek-cli`) that talks to `127.0.0.1:3814`, builds typed command payloads using registry-derived command bytes, and keeps raw writes behind an explicit unsafe gate.

For deployment, ship both `monsgeek-driver.service` and `monsgeek-inputd.service` in the same phase with deterministic install steps and restart-on-failure defaults. This satisfies GRPC-09 while preserving the IF2 (bridge) / IF0 (input daemon) split that is already validated in previous phases.

**Primary recommendation:**
1. Build `monsgeek-cli` with typed subcommands mapped to existing command bytes and checksums.
2. Implement deterministic device selection (`--model`, `--path`, `--device-id`, auto-single only).
3. Package systemd units + install script + smoke verification flow that includes auto-start and restart behavior.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- CLI defaults to calling `DriverGrpc` on `127.0.0.1:3814`; do not re-implement direct USB transport in this phase.
- CLI and web app behavior must stay aligned via the same bridge path and policy gates.
- Direct transport mode is out of scope for Phase 7.
- Required typed subcommands: `devices list`, `info`, `led get/set`, `debounce get/set`, `poll get/set`, `profile get/set`, `keymap get/set`, `macro get/set`, `raw send/read`.
- Raw writes must require explicit unsafe acknowledgment flag (for example `--unsafe`).
- Device selectors: `--model <slug>`, `--path <bridge-path>`, `--device-id <firmware-id>`.
- Device selection behavior:
  - Exactly one supported online device -> auto-select
  - Multiple supported devices -> fail with clear selector guidance (no random implicit selection)
- Model selector must resolve through JSON registry/profile data (no hardcoded per-model command tables in CLI).
- Phase must ship bridge and input daemon systemd units together.
- Service packaging/docs must preserve ownership split: bridge on control/vendor path, input daemon on input path.

### the agent's Discretion
- Human-readable output layout (table vs compact)
- JSON output shape for script mode
- Unit file directory layout and helper script structure

### Deferred Ideas (OUT OF SCOPE)
- Direct USB transport CLI mode
- Additional interactive TUI layer
- Non-systemd init system support
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CLI-01 | User can perform core keyboard operations via command-line interface | Driver proto already exposes `watchDevList`, `sendMsg`, and `readMsg`. Command constants and per-device command table resolution are already in `monsgeek-protocol`. Typed subcommands can be translated into existing command bytes/checksum rules without touching transport internals. |
| CLI-02 | CLI uses the same JSON-driven registry/profile data as the bridge | `DeviceRegistry::load_from_directory()` and `DeviceDefinition::commands()` already provide runtime model resolution and per-device command overrides. CLI can load the same registry directory and avoid hardcoded model logic. |
| GRPC-09 | Systemd service unit enables auto-start on boot with managed lifecycle | Existing binaries (`monsgeek-driver`, `monsgeek-inputd`) can be wrapped with service units using `Restart=on-failure`, `WantedBy=multi-user.target`, and deterministic install/enable commands. |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `clap` | 4.x | CLI argument parsing + typed subcommands | Already used in `monsgeek-inputd`; fits project style |
| `tonic` | 0.12 | gRPC client to `DriverGrpc` | Same ecosystem as bridge server crate |
| `tokio` | 1.x | async runtime for client RPC calls | Existing project runtime standard |
| `monsgeek-protocol` | workspace | registry loading + command constants/tables | Single source of truth for model/command mapping |

### Supporting
| Tooling | Purpose | When to Use |
|---------|---------|-------------|
| `systemd` units | managed daemon startup/restart | bridge + input daemon deployment |
| `grpcurl` (tests/docs) | quick RPC smoke verification | post-install verification and troubleshooting |
| `tools/test.sh` | existing repo smoke harness | extend for Phase 7 CLI/service checks |

## Architecture Patterns

### Pattern 1: Bridge-First CLI Command Path

**What:** `monsgeek-cli` calls `DriverGrpc/sendMsg` and `DriverGrpc/readMsg` over localhost.

**When to use:** All typed operations in Phase 7.

**Key details:**
- Keep one command path to prevent behavior drift from web configurator behavior.
- Build command frame as `[cmd, payload...]` and send via `sendMsg` using proto checksum enum.
- Readbacks use `readMsg` and decode response bytes in CLI layer.
- Let bridge-side policy (`evaluate_outbound_command`) remain the safety boundary.

### Pattern 2: Registry-Driven Device + Command Resolution

**What:** Load JSON registry once, resolve model/device capabilities and command bytes from `DeviceDefinition`.

**When to use:** `--model` selection and family-specific commands (profile/debounce/keymatrix/macro).

**Key details:**
- Load from `MONSGEEK_DEVICE_REGISTRY_DIR` when set, else `crates/monsgeek-protocol/devices` in dev mode.
- For family-divergent bytes use `definition.commands()` (for example `get_profile`, `set_profile`, `set_debounce`).
- Keep shared commands from `cmd` constants when not family-divergent (for example `GET_USB_VERSION`, `GET_LEDPARAM`).

### Pattern 3: Deterministic Device Selection Contract

**What:** Resolve target device using explicit precedence and no random fallback.

**When to use:** Every CLI command that requires a device target.

**Selection precedence:**
1. `--path` exact match
2. `--device-id` exact firmware ID match
3. `--model` resolves to registry slug/name and then filters online devices
4. no selector:
   - one supported online device -> select automatically
   - more than one -> fail with actionable selector hints

### Pattern 4: Service Packaging with Operational Guardrails

**What:** Ship unit files and install helper script with explicit restart and boot behavior.

**When to use:** GRPC-09 implementation.

**Key details:**
- Driver service: `Restart=on-failure`, `RestartSec=2`, boot enable path documented.
- Input daemon service: same restart behavior; run independently from bridge process.
- Install script should run `daemon-reload`, `enable`, optional `start`, and `status` checks.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Device registry parsing | Manual JSON parsing in CLI | `DeviceRegistry::load_from_directory` | Keeps CLI and bridge behavior aligned |
| Family command resolution | hardcoded per-model command maps in CLI | `DeviceDefinition::commands()` | Handles protocol-family differences + overrides |
| Safety gating | ad-hoc CLI-side policy logic | bridge policy (`evaluate_outbound_command`) + explicit `--unsafe` for raw writes | Prevents duplicated policy drift |
| Service lifecycle loops | custom restart shell loops | `systemd` `Restart=on-failure` | OS-native lifecycle and observability |

## Common Pitfalls

### Pitfall 1: Ambiguous device selection with multiple keyboards
**What goes wrong:** CLI picks one device silently and mutates the wrong keyboard.
**How to avoid:** Fail fast when >1 supported online devices and no selector is given.
**Warning sign:** Commands “work” but affect a different physical board.

### Pitfall 2: Command bytes diverge across protocol families
**What goes wrong:** CLI hardcodes one command byte (for example debounce/profile) and fails on a different family.
**How to avoid:** Resolve command bytes from `DeviceDefinition::commands()` for family-divergent operations.
**Warning sign:** Command works on M5W but fails on another supported profile.

### Pitfall 3: Unsafe raw writes exposed by default
**What goes wrong:** Script accidentally issues dangerous writes through raw path.
**How to avoid:** Require `--unsafe` for raw write commands; keep raw read path safe-by-default.
**Warning sign:** Accidental state changes during read-only diagnostics.

### Pitfall 4: systemd units installed without deterministic enable/start flow
**What goes wrong:** Unit files exist but services are not enabled at boot.
**How to avoid:** Include install helper and explicit verify commands (`is-enabled`, `is-active`, restart test).
**Warning sign:** Works manually after reboot only when user restarts services by hand.

## Code Examples

### CLI gRPC send/read pattern
```rust
let mut client = DriverGrpcClient::connect("http://127.0.0.1:3814").await?;
client.send_msg(SendMsg {
    device_path: target_path,
    msg: vec![cmd::GET_USB_VERSION],
    check_sum_type: CheckSumType::Bit7 as i32,
    dangle_dev_type: 0,
}).await?;
let response = client.read_msg(ReadMsg { device_path: target_path }).await?;
```

### systemd driver unit shape
```ini
[Unit]
Description=MonsGeek Driver Bridge
After=network-online.target

[Service]
Type=simple
ExecStart=/usr/bin/monsgeek-driver
Restart=on-failure
RestartSec=2

[Install]
WantedBy=multi-user.target
```

## Validation Architecture

Phase 7 validation should enforce both command-surface correctness and lifecycle behavior.

### Automated layers
1. CLI unit/integration tests for parser + selector resolution + unsafe gate.
2. CLI RPC smoke tests against live bridge using a mock or local bridge process.
3. Service unit static checks (`systemd-analyze verify`) and install-script dry-run checks.

### Manual/hardware layers
1. Boot/start behavior (`systemctl enable --now ...`) for both services.
2. Restart-on-failure behavior (`kill -9` service process and confirm auto-restart).
3. End-to-end CLI operations against real keyboard (`info`, `led`, `debounce`, `profile`, `keymap`).

### Recommended sampling cadence
- After every Phase 7 task commit: `cargo test -p monsgeek-cli -p monsgeek-driver`
- After each wave: `cargo test --workspace` + service unit verification command
- Before phase verification: full service start/reboot/restart smoke sequence on target Linux host

