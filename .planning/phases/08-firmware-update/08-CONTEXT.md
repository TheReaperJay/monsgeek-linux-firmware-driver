# Phase 08: firmware-update - Context

**Gathered:** 2026-03-28
**Status:** Ready for planning

<domain>
## Phase Boundary

Deliver safe Linux firmware update support for the MonsGeek M5W: version query, firmware image validation, explicit pre-bootloader confirmation, bootloader transfer with integrity verification, and strict post-flash verification. This phase clarifies how to implement firmware update safety and execution; it does not add unrelated new capabilities.

</domain>

<decisions>
## Implementation Decisions

### Update Entrypoint and UX
- **D-01:** Phase 08 ships both paths: CLI firmware update commands and bridge `upgradeOTAGATT` support.
- **D-02:** Use one shared firmware-update engine implementation consumed by both CLI and bridge (no duplicated flashing logic).
- **D-03:** `upgradeOTAGATT` streams phase-oriented progress plus numeric progress (not percent-only or completion-only output).
- **D-04:** Bridge OTA support is runtime-gated and disabled by default; enabling requires an explicit startup flag.

### Firmware Intake and Authenticity
- **D-05:** Firmware update is official-first: replicate vendor Electron/web metadata matching logic so the updater validates that firmware maps to the connected device/model.
- **D-06:** If metadata cannot be confidently matched to the connected device, flashing is hard-blocked by default.
- **D-07:** A guarded override path is allowed for unofficial images (`--allow-unofficial` style), but only with additional high-risk confirmation.
- **D-08:** Phase 08 supports both validated local-file input and direct vendor-download input, with the same official-validation pipeline applied to both.

### Safety Gates and Confirmation Flow
- **D-09:** Pre-bootloader confirmation must require typed phrase confirmation (not a simple yes/no).
- **D-10:** Non-interactive flashing requires both `--yes` and a dedicated high-risk flag; `--yes` alone is insufficient.
- **D-11:** Perform a best-effort preflash backup of restorable settings/session data and store it with flash-session metadata.
- **D-12:** If backup fails, show warning and require an additional explicit confirmation before continuing.

### Bootloader Transfer and Failure Policy
- **D-13:** When multiple target devices are present, prompt interactively for device selection, then verify identity against device metadata using `GET_USB_VERSION` before transfer.
- **D-14:** Use bounded bootloader-entry retry policy: one controlled retry after timeout, then fail with recovery guidance.
- **D-15:** Integrity mismatch or incomplete transfer is a hard failure; report likely bootloader state and explicit recovery actions.
- **D-16:** Success is reported only after strict post-flash verification: normal-mode re-enumeration plus `GET_USB_VERSION` check, and `GET_REV` when available.

### the agent's Discretion
- Exact flag names and CLI text UX for high-risk confirmations (while preserving all required gates above).
- Exact phase labels emitted in bridge progress stream (while preserving phase + numeric progress semantics).
- Exact backup artifact format and storage path.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase Scope and Safety Requirements
- `.planning/ROADMAP.md` — Phase 08 goal/success criteria and explicit CRC/integrity expectations.
- `.planning/REQUIREMENTS.md` — `FW-01`, `FW-02`, `FW-03`, `FW-04` requirement definitions.
- `.planning/PROJECT.md` — project-level non-negotiables for safety gates and Linux-first behavior.

### Existing Local Contract and Runtime Integration
- `crates/monsgeek-driver/proto/driver.proto` — `upgradeOTAGATT` RPC contract and progress stream message shape.
- `crates/monsgeek-driver/src/service/mod.rs` — current bridge send/read behavior and existing OTA stub to replace.
- `crates/monsgeek-cli/src/commands.rs` — existing unsafe-gating pattern and typed command execution approach.
- `crates/monsgeek-transport/src/controller.rs` — command pacing model (100ms firmware safety delay).
- `crates/monsgeek-transport/src/usb.rs` — `UsbVersionInfo` parsing and `GET_USB_VERSION` identity/firmware fields.

### Firmware Update Protocol and Reference Implementation
- `references/monsgeek-akko-linux/docs/PROTOCOL.md` — bootloader sequence, transfer flow, checksum behavior, and recovery notes.
- `references/monsgeek-akko-linux/iot_driver_linux/src/flash.rs` — reference flash engine structure (bootloader discovery, transfer, progress).
- `references/monsgeek-akko-linux/iot_driver_linux/src/protocol.rs` — firmware-update constants and checksum helpers.
- `references/monsgeek-akko-linux/iot_driver_linux/src/commands/firmware.rs` — CLI-level warning/confirmation flow and operational UX.

### Local Firmware Artifact
- `firmware/m5w_firmware_v103.bin` — in-repo M5W firmware artifact for validation/testing pathways.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/monsgeek-driver/proto/driver.proto`: already defines `upgradeOTAGATT` and `Progress` stream types.
- `crates/monsgeek-driver/src/service/mod.rs`: already has bridge transport lifecycle and a placeholder OTA RPC hook.
- `crates/monsgeek-cli/src/lib.rs` + `crates/monsgeek-cli/src/commands.rs`: existing command scaffold, selector model, and unsafe-gating UX patterns.
- `crates/monsgeek-transport/src/controller.rs`: central command pacing and query/send operations suited for update sequencing.
- `crates/monsgeek-transport/src/usb.rs`: stable `GET_USB_VERSION` parsing for identity/version verification checkpoints.

### Established Patterns
- Safety-sensitive operations use explicit user intent and guardrails before write paths.
- Firmware identity is anchored on firmware-reported IDs (not transient runtime path alone).
- Multi-device ambiguity is treated as explicit-selection UX, not silent auto-pick.
- High-risk transport workflows preserve deterministic sequencing and explicit failure surfacing.

### Integration Points
- Add a shared firmware-update engine crate/module reusable by both `monsgeek-cli` and `monsgeek-driver` service layer.
- Wire CLI firmware commands to the shared engine and keep flag-driven safety policy consistent with existing CLI semantics.
- Replace `upgradeOTAGATT` stub in `DriverService` with bridge adapter over the shared engine and stream progress events.
- Reuse `GET_USB_VERSION`/`GET_REV` query path for preflight compatibility checks and post-flash validation.

</code_context>

<specifics>
## Specific Ideas

- Official firmware trust should come from reproducing the Electron/web metadata compatibility checks, so updates are tied to official firmware links/payload metadata.
- Ambiguous multi-device cases should prompt for interactive selection, then enforce metadata + `GET_USB_VERSION` compatibility checks on the chosen target before transfer.

</specifics>

<deferred>
## Deferred Ideas

None - discussion stayed within phase scope.

</deferred>

---

*Phase: 08-firmware-update*
*Context gathered: 2026-03-28*
