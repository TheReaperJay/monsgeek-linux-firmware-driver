# Phase 8: Firmware Update - Research

**Researched:** 2026-03-28
**Domain:** Safe firmware update orchestration over existing MonsGeek Linux transport/bridge stack
**Confidence:** HIGH

## Summary

Phase 8 should be implemented as a shared firmware-update engine consumed by both `monsgeek-cli` and the bridge `upgradeOTAGATT` RPC, with strict safety policy enforced before any bootloader transition. The current codebase already provides key foundations: reliable `GET_USB_VERSION` parsing in transport, deterministic command pacing, bridge command policy gates, and deterministic device-selection behavior in the CLI.

Reference implementation artifacts (`references/monsgeek-akko-linux/...`) already encode the expected bootloader flow: `ISP_PREPARE` + boot-entry command, bootloader re-enumeration wait, transfer start marker (`0xBA 0xC0`), 64-byte chunk transfer, transfer complete marker (`0xBA 0xC2`), and checksum semantics including padding bytes. The safest path is to adapt those rules into a local engine module while keeping Linux-first guardrails from Phase 2 and the explicit confirmation decisions from `08-CONTEXT.md`.

**Primary recommendation:** build one reusable flash engine crate/module, call it from both CLI and bridge streaming API, and gate all writes with metadata/device match + typed high-risk confirmations + strict post-flash verification.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Ship both paths: CLI firmware update commands and bridge `upgradeOTAGATT` support.
- Use one shared firmware-update engine for CLI + bridge (no duplicated logic).
- `upgradeOTAGATT` must stream phase-oriented progress plus numeric progress.
- Bridge OTA support is runtime-gated and disabled by default.
- Official-first intake: implement vendor/Electron-style metadata match before flash.
- Unknown/unmatched metadata is blocked by default.
- Unofficial override is allowed only behind explicit high-risk opt-in.
- Support local file input and vendor-download input through the same validation pipeline.
- Pre-bootloader confirmation requires typed phrase confirmation.
- Non-interactive mode requires both `--yes` and dedicated high-risk flag.
- Run best-effort preflash backup; if backup fails, require extra explicit confirmation.
- Multi-device environment must require explicit target selection and identity re-check.
- Bootloader entry retry policy: one controlled retry, then fail with recovery guidance.
- Integrity mismatch/incomplete transfer is hard failure with explicit recovery steps.
- Success requires strict post-flash checks: normal-mode re-enumeration + `GET_USB_VERSION`, and `GET_REV` when available.

### the agent's Discretion
- Exact CLI flag names and phrasing while preserving required guard behavior.
- Progress phase label text for streamed bridge updates.
- Backup artifact file format and storage location.

### Deferred Ideas (OUT OF SCOPE)
- None.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| FW-01 | Read firmware version via GET_USB_VERSION and GET_REV where available | `UsbVersionInfo::parse` already supports firmware identity/version extraction; CLI/bridge can add explicit query/read flow using existing send/read path. |
| FW-02 | Flash via bootloader entry, chunk transfer, and CRC/integrity validation | Reference flow and constants are documented in `references/monsgeek-akko-linux/iot_driver_linux/src/flash.rs` and protocol docs section 8. |
| FW-03 | Require explicit user confirmation before bootloader entry | Existing CLI unsafe-gate patterns provide a baseline for typed confirmations + non-interactive dual-flag gate. |
| FW-04 | Validate image integrity before bootloader entry | Reference checksum/chunk metadata validation exists; local engine should enforce size/chunk/checksum compatibility and metadata/device match before any boot command. |
</phase_requirements>

## Standard Stack

### Core
| Component | Source | Purpose | Why Standard Here |
|-----------|--------|---------|-------------------|
| Shared firmware-update engine module/crate | new local module in workspace | Single authoritative flash state machine | Avoids CLI/bridge drift and duplicated high-risk logic |
| `monsgeek-transport` command/query primitives | existing crate | Command pacing + low-level HID command path | Already validated on target hardware with 100ms safety pacing |
| Driver proto `upgradeOTAGATT` stream | existing `driver.proto` | Bridge progress surface | Matches expected web/Electron contract shape |
| `monsgeek-cli` selector/gating patterns | existing CLI crate | Safe target selection + high-risk UX | Already established in prior phases |

### Supporting
| Component | Purpose | When to Use |
|-----------|---------|-------------|
| `references/monsgeek-akko-linux/iot_driver_linux/src/flash.rs` | Bootloader flow reference | Mapping phases/acks/retry logic |
| `references/monsgeek-akko-linux/iot_driver_linux/src/protocol.rs` (`firmware_update` module) | Transfer constants/checksum helpers | Implementing transfer framing and checksum behavior |
| `references/monsgeek-akko-linux/docs/PROTOCOL.md` section 8 | Safety and recovery semantics | User-facing warnings + recovery instructions |
| local firmware artifact `firmware/m5w_firmware_v103.bin` | Known test payload | Dry-run/validation and guarded integration tests |

## Architecture Patterns

### Pattern 1: Shared Engine + Thin Adapters
**What:** Implement one phase-driven flash engine and expose adapter wrappers for CLI and bridge RPC.
**When to use:** All firmware update operations.
**Key details:**
- Engine owns preflight, transfer, verification, and recovery classification.
- CLI adapter handles interactive confirmations and local-file/download options.
- Bridge adapter maps engine events to `Progress` stream (`progress` float + phase/error text).

### Pattern 2: Preflight Gate Before Any Boot Command
**What:** Hard gate pipeline before `ISP_PREPARE` or boot-entry.
**When to use:** Every flash request.
**Preflight checks:**
- Device selection resolved and unambiguous.
- Connected device metadata/id match expected firmware metadata.
- Firmware file basic validity (length/chunk count/checksum derivation feasibility).
- Optional backup attempted and result persisted.
- User confirmation policy satisfied (`typed phrase` or dual non-interactive flags).

### Pattern 3: Deterministic Phase State Machine
**What:** Explicit states and transitions for scanning, boot entry, transfer, completion, and post-check.
**When to use:** Engine internal control flow and bridge progress stream.
**Recommended phases:**
1. `preflight`
2. `enter_bootloader`
3. `wait_bootloader`
4. `transfer_start`
5. `transfer_chunks`
6. `transfer_complete`
7. `wait_reboot`
8. `post_verify`
9. `done` / `failed`

### Pattern 4: Strict Post-Flash Verification Contract
**What:** Success only after normal-mode return and version queries pass.
**When to use:** Finalization.
**Checks:**
- Device leaves bootloader and re-enumerates in normal mode.
- `GET_USB_VERSION` succeeds and returns valid identity data.
- `GET_REV` queried when supported and captured for session summary.
- Failure at any step is terminal with explicit recovery guidance.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Bootloader framing details | Ad-hoc bytes in CLI handler | Shared constants/helpers modeled after reference `firmware_update` module | Avoids header/checksum/size mismatches |
| Duplicate flashing logic | Separate CLI and bridge implementations | One reusable engine with adapters | Prevents behavioral drift in safety-critical flow |
| Device selection heuristics | Random auto-pick in multi-device scenarios | Existing deterministic selector contract + explicit prompt | Reduces wrong-device flash risk |
| Ambiguous success criteria | “No error means success” | Mandatory post-flash re-enumeration + version verification | Prevents false-positive completion when device remains in bootloader |

## Common Pitfalls

### Pitfall 1: Bootloader checksum mismatch due padding omission
**What goes wrong:** Transfer completes but bootloader rejects image and remains in bootloader mode.
**Why it happens:** Host checksum ignores 0xFF padding bytes for final partial chunk.
**How to avoid:** Compute checksum across full chunked wire payload semantics (including padding bytes).
**Warning signs:** Transfer appears complete but no return to normal-mode PID/path.

### Pitfall 2: Wrong target device in multi-device setups
**What goes wrong:** Firmware is flashed to unintended device.
**Why it happens:** Insufficient selector contract or stale path assumptions.
**How to avoid:** Require explicit selection when >1 supported device, then re-validate identity with `GET_USB_VERSION` pre-transfer.
**Warning signs:** Selected path PID/id does not match expected metadata.

### Pitfall 3: Insufficient confirmation gating in non-interactive mode
**What goes wrong:** Automation accidentally triggers destructive flash.
**Why it happens:** Single `--yes` style bypass without dedicated high-risk opt-in.
**How to avoid:** Require dual-flag non-interactive gate and preserve typed phrase requirement for interactive mode.
**Warning signs:** Flash command can proceed with only one generic confirmation flag.

### Pitfall 4: Reporting success before post-verify
**What goes wrong:** UX says success while keyboard remains in bootloader or inconsistent state.
**Why it happens:** Completion tied only to chunk transfer and not to final runtime checks.
**How to avoid:** Treat post-flash re-enumeration + version queries as part of the success condition.
**Warning signs:** Missing final identity/version capture in session result.

## Codebase Readiness Snapshot

- `crates/monsgeek-driver/src/service/mod.rs`: `upgrade_otagatt` currently returns empty stream (stub), so bridge integration work is required.
- `crates/monsgeek-driver/proto/driver.proto`: RPC and `Progress` message already exist.
- `crates/monsgeek-cli/src/lib.rs`: no firmware command surface exists yet; new subcommands are needed.
- `crates/monsgeek-transport/src/usb.rs`: `UsbVersionInfo` parsing exists and can anchor identity/version verification.
- `crates/monsgeek-transport/src/controller.rs`: enforces the known 100ms pacing constraint already required for firmware safety.
- `crates/monsgeek-protocol/src/command_policy.rs`: strong write-policy pattern exists and can be extended/reused for firmware command guardrails.

## Validation Architecture

| Layer | Goal | Command/Method |
|------|------|----------------|
| Unit | Verify preflight policy behavior and parser logic | `cargo test -p monsgeek-cli -p monsgeek-driver` |
| Unit | Verify transfer framing/checksum math (including padding semantics) | targeted tests in new firmware engine module |
| Integration (mock/sim) | Verify phase state machine and retry/failure paths | engine integration tests with deterministic mocked transport |
| Integration (bridge) | Verify `upgradeOTAGATT` progress stream phases and error mapping | driver service tests around stream outputs |
| Manual hardware | Verify real-device flash and post-verify behaviors | guarded manual run against M5W with recovery checklist |

### Wave 0 gaps to close during planning
- [ ] Shared firmware-engine module location and API contract.
- [ ] Firmware intake abstraction (local file + vendor-download source) with metadata validation.
- [ ] Backup artifact format + persistence path.
- [ ] Test harness strategy for bootloader/re-enumeration simulation without requiring destructive writes in routine CI.

## Open Questions

1. **Exact official metadata source contract**
   - What we know: Context requires vendor/Electron-equivalent metadata matching.
   - What is unclear: concrete API payload contract and local caching behavior in this repo.
   - Planning recommendation: dedicate first plan objective to codify metadata schema + matcher with explicit test vectors.

2. **Bridge-side authorization surface for OTA**
   - What we know: OTA must be runtime-disabled by default and explicitly enabled.
   - What is unclear: whether enablement should be CLI flag only, env var only, or both.
   - Planning recommendation: treat as explicit config gate with testable startup behavior and clear operator docs.

3. **Non-destructive automated verification breadth**
   - What we know: full flash is destructive/high-risk and unsuitable for routine CI.
   - What is unclear: acceptable simulated coverage threshold for phase verification before manual hardware checkpoint.
   - Planning recommendation: formalize mock-engine coverage targets plus one guarded manual verification protocol.

## Sources

### Primary (project-local, high confidence)
- `.planning/phases/08-firmware-update/08-CONTEXT.md`
- `.planning/ROADMAP.md`
- `.planning/REQUIREMENTS.md`
- `.planning/STATE.md`
- `crates/monsgeek-driver/proto/driver.proto`
- `crates/monsgeek-driver/src/service/mod.rs`
- `crates/monsgeek-cli/src/lib.rs`
- `crates/monsgeek-cli/src/commands.rs`
- `crates/monsgeek-transport/src/controller.rs`
- `crates/monsgeek-transport/src/usb.rs`
- `references/monsgeek-akko-linux/iot_driver_linux/src/flash.rs`
- `references/monsgeek-akko-linux/iot_driver_linux/src/protocol.rs`
- `references/monsgeek-akko-linux/iot_driver_linux/src/commands/firmware.rs`
- `references/monsgeek-akko-linux/docs/PROTOCOL.md`

---
*Phase: 08-firmware-update*
*Research completed: 2026-03-28*
*Ready for planning: yes*
