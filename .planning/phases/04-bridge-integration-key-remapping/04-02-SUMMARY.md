---
phase: 04-bridge-integration-key-remapping
plan: 02
subsystem: api
tags: [grpc, hardware-tests, key-remapping, browser-verification, protocol-normalization]

# Dependency graph
requires:
  - phase: 04-bridge-integration-key-remapping
    plan: 01
    provides: validate_dangerous_write, ConnectedDevice with DeviceDefinition, bounds validation
provides:
  - Hardware integration tests for GET_KEYMATRIX, GET/SET_PROFILE, SET_KEYMATRIX roundtrip
  - SET_KEYMATRIX_SIMPLE (0x13) and SET_FN_SIMPLE (0x15) in protocol command catalog
  - Config normalization for SIMPLE keymatrix commands (reset encoding fix)
  - Bounds validation extended to cover SET_KEYMATRIX_SIMPLE
  - Browser-verified key remapping and profile switching on real M5W
affects: [05-led-tuning]
---

## Summary

Added hardware integration tests for key remapping and profile switching, then discovered and fixed a protocol-level encoding mismatch between the web app and the YiChip firmware.

## What Changed

### Hardware Integration Tests (Task 1)
- `test_get_keymatrix_profile_0` — verifies GET_KEYMATRIX echo and response structure on M5W
- `test_get_set_profile` — profile switch round-trip with restore
- `test_set_keymatrix_roundtrip_dangerous` — SET_KEYMATRIX write verification (dangerous gate)

### Protocol Command Catalog (discovered during browser verification)
- Added SET_KEYMATRIX_SIMPLE (0x13), GET_KEYMATRIX_SIMPLE (0x93), SET_FN_SIMPLE (0x15), GET_FN_SIMPLE (0x95) to cmd.rs, CommandTable, and CommandOverrides
- YiChip devices get these commands; RY5088 gets None

### Config Normalization
- `normalize_simple_keymatrix()` fixes the web app's reset encoding: the app sends `[0, 0, keycode, 0]` (keycode at config[2]) but the firmware's SIMPLE handler reads config[1]. The bridge normalizes to `[0, keycode, 0, 0]`.
- Applied to both SET_KEYMATRIX_SIMPLE and SET_FN_SIMPLE
- Bounds validation extended to cover SIMPLE commands (profile + key_index, no layer)

### Browser Checkpoint (Task 2)
Verified on real M5W hardware via app.monsgeek.com:
- Key remap (SET_KEYMATRIX_SIMPLE) — key produces new output ✓
- Key reset to default — normalization fixes dead-key bug ✓
- Layer/profile switch (SET_PROFILE) — no crash, independent mappings ✓
- Debounce adjustment (SET_DEBOUNCE) — works without crash ✓

## Commits

- `bfbcf5d` test(04-02): add hardware integration tests for key remapping and profile switching
- `2bc782a` fix: add SIMPLE keymatrix commands and normalize reset config bytes
- `007c0ca` fix: restore send_msg/read_msg logging to info level

## Key Decisions

- SET_KEYMATRIX_SIMPLE and SET_FN_SIMPLE use a different config byte interpretation than SET_KEYMATRIX — config[1] is the primary keycode for SIMPLE, config[2] for the full command
- The bridge normalizes SIMPLE configs rather than translating SIMPLE→full commands, because the two commands have incompatible config interpretations
- The web app's "Layer" UI maps to the firmware's SET_PROFILE (0x05) command

## Self-Check: PASSED
