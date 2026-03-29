# Phase 08: firmware-update - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md - this log preserves the alternatives considered.

**Date:** 2026-03-28T14:28:30+07:00
**Phase:** 08-firmware-update
**Areas discussed:** Update entrypoint and UX, Firmware file acceptance and validation, Safety gates and confirmation flow, Bootloader transfer behavior and failure policy

---

## Update Entrypoint and UX

### Q1. Primary flashing entrypoint for Phase 08

| Option | Description | Selected |
|--------|-------------|----------|
| CLI-first only | Implement CLI firmware flow now; keep bridge OTA stubbed | |
| Bridge RPC-first | Implement `upgradeOTAGATT` first and make CLI a wrapper | |
| Both fully in Phase 08 | Ship CLI and bridge OTA flow together | ✓ |

**User's choice:** Both fully in Phase 08.

### Q2. Shared implementation architecture

| Option | Description | Selected |
|--------|-------------|----------|
| One core shared flash engine | Shared implementation used by both CLI and bridge | ✓ |
| Core logic in driver only | CLI calls gRPC only | |
| Separate implementations | Independent CLI and bridge flash logic | |

**User's choice:** One core shared flash engine.

### Q3. Bridge OTA progress stream shape

| Option | Description | Selected |
|--------|-------------|----------|
| Phased plus percent | Stream phase labels plus numeric progress | ✓ |
| Percent only | Single numeric progress feed | |
| Completion only | Minimal eventing until done/error | |

**User's choice:** Phased plus numeric progress.

### Q4. Runtime safety gate for bridge OTA

| Option | Description | Selected |
|--------|-------------|----------|
| Disabled by default | OTA requires explicit startup flag | ✓ |
| Always enabled | OTA available whenever bridge runs | |
| Dev-only default | Enabled in dev builds only | |

**User's choice:** Disabled by default with explicit enablement.

---

## Firmware File Acceptance and Validation

### Q1. Input firmware trust model

| Option | Description | Selected |
|--------|-------------|----------|
| Official-first with guarded override | Validate via vendor-source logic, allow explicit unsafe override | ✓ |
| Strict official-only | Reject unverifiable firmware with no override path | |
| Basic file checks only | Defer official-source verification | |

**User's choice:** Official-first with guarded override.
**Notes:** User asked to recreate decompiled Electron/web firmware-selection logic so installs are sourced from official firmware links/metadata.

### Q2. First-priority official validation mechanism

| Option | Description | Selected |
|--------|-------------|----------|
| Recreate vendor metadata matching | Parse and verify metadata like Electron/web flow | ✓ |
| Basic binary checks only | Chip-id/version/shape checks without metadata recreation | |
| Hash allowlist only | Strict hash pinning first | |

**User's choice:** Recreate vendor metadata matching.

### Q3. Mismatch or unverifiable metadata default behavior

| Option | Description | Selected |
|--------|-------------|----------|
| Hard block by default | Only bypass with guarded unofficial override | ✓ |
| Warn then continue | Continue with one warning confirmation | |
| Auto-continue in non-interactive | No hard stop in scripted mode | |

**User's choice:** Hard block by default.

### Q4. Retrieval path scope in Phase 08

| Option | Description | Selected |
|--------|-------------|----------|
| Local file only | Validated local file path pipeline | |
| Direct download only | Vendor URL flow only | |
| Both local + direct download | Support both with same validation rules | ✓ |

**User's choice:** Both local validated files and direct vendor-download flow.

---

## Safety Gates and Confirmation Flow

### Q1. Pre-bootloader confirmation strength

| Option | Description | Selected |
|--------|-------------|----------|
| Typed phrase confirmation | Require explicit typed phrase | ✓ |
| Yes/No prompt | Single confirmation prompt | |
| No prompt with `--yes` | Fully bypass interactive confirmation | |

**User's choice:** Typed phrase confirmation.

### Q2. Non-interactive automation gate

| Option | Description | Selected |
|--------|-------------|----------|
| `--yes` plus dedicated high-risk flag | Two explicit automation gates required | ✓ |
| `--yes` only | Single flag sufficient | |
| No non-interactive flashing | Interactive-only always | |

**User's choice:** Require both `--yes` and dedicated high-risk flag.

### Q3. Safety snapshot before bootloader entry

| Option | Description | Selected |
|--------|-------------|----------|
| Best-effort backup plus restore hooks | Capture restorable state when possible | ✓ |
| Warn only, no backup | No snapshot effort in Phase 08 | |
| Backup required | Abort if snapshot fails | |

**User's choice:** Best-effort backup and restore hooks.

### Q4. Behavior when backup fails

| Option | Description | Selected |
|--------|-------------|----------|
| Warn plus extra confirmation | Continue only after additional explicit confirmation | ✓ |
| Warn and continue | No extra gate after warning | |
| Hard block | Abort flashing if backup fails | |

**User's choice:** Warn plus extra confirmation.

---

## Bootloader Transfer Behavior and Failure Policy

### Q1. Device selection policy when multiple eligible devices are present

| Option | Description | Selected |
|--------|-------------|----------|
| Fail on ambiguity | Require explicit selector flags | |
| Auto-pick first | Implicit first-device selection | |
| Interactive prompt | Prompt user to choose target device | ✓ |

**User's choice:** Interactive prompt.
**Notes:** User explicitly required post-selection verification using device metadata and `GET_USB_VERSION` to ensure selected hardware and firmware metadata match before transfer.

### Q2. Bootloader entry/discovery retry policy

| Option | Description | Selected |
|--------|-------------|----------|
| Bounded retry | One controlled retry then fail with guidance | ✓ |
| Aggressive retry loop | Keep retrying until success | |
| No retry | Fail immediately on first timeout | |

**User's choice:** Bounded retry.

### Q3. Integrity failure behavior

| Option | Description | Selected |
|--------|-------------|----------|
| Hard fail with recovery guidance | Treat mismatch/incomplete transfer as failed flash | ✓ |
| Auto-retry until success | Reattempt transfer repeatedly | |
| Soft success path | Continue if reboot happens | |

**User's choice:** Hard fail with recovery guidance.

### Q4. Post-flash success criteria

| Option | Description | Selected |
|--------|-------------|----------|
| Strict verification | Re-enumerate normal mode + `GET_USB_VERSION` (+ `GET_REV` when available) | ✓ |
| Transfer-complete only | No post-reboot checks | |
| Reconnect-only check | Connectivity check without command verification | |

**User's choice:** Strict verification.

---

## the agent's Discretion

- Exact naming of high-risk flags and progress phase labels.
- Exact formatting of operator prompts and progress output.
- Exact structure of backup artifact contents and persistence path.

## Deferred Ideas

- None recorded during this discussion.
