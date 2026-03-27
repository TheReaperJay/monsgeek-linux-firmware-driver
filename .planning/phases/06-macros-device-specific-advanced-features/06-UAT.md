---
status: complete
phase: 06-macros-device-specific-advanced-features
source: [06-01-SUMMARY.md, 06-02-SUMMARY.md]
started: 2026-03-27T13:11:11Z
updated: 2026-03-27T16:12:21Z
---

## Current Test

[testing complete]

## Tests

### 1. Invalid Macro Write Is Rejected Cleanly
expected: Invalid macro writes are rejected immediately with a clear error and no app/bridge hang.
result: pass

### 2. Invalid FN Write Is Rejected Cleanly
expected: Out-of-range FN/profile/key writes are rejected immediately with a clear error and no app/bridge hang.
result: pass

### 3. Magnetic Write On Non-Magnetic Device Does Not Hang
expected: Sending magnetic SET commands on a non-magnetic keyboard does not hang the web flow; request completes with compatibility behavior and UI remains usable.
result: pass
evidence: Runtime bridge path validated in combined stress runs; no hang/crash observed across 200 macro iterations and 200 layer iterations in scripted UAT.

### 4. Valid Macro/FN Writes Still Work
expected: Valid macro write/read and valid FN mapping operations still succeed after safety gating changes.
result: pass
evidence: `bash tools/test.sh --macro-stress --macro-iterations 200 --macro-read-timeout 1` completed 200/200 iterations with no send/read errors and no disconnect loop.

### 5. Layer/Profile Switching Remains Stable
expected: Layer/profile switching from the app works without keyboard crash, disconnect loop, or stalled input path.
result: pass
evidence: `bash tools/test.sh --layer-stress --iterations 200 --read-timeout 1` completed 200/200 iterations; command/read flow remained responsive.

## Summary

total: 5
passed: 5
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps

[none]

## Verification Evidence

- `watchDevList` now initializes with online device and non-empty path:
  - `watch_dev_list: sending Init with 1 device(s)`
  - `DEVPATH=<3151-4011-ffff-0002-1@id1308-b003-a006-n1>`
- Discovery logs show alias-based runtime resolution (central registry path):
  - `Probe fallback: using runtime VID/PID alias ... PID:0x4011 -> device 1308 (M5W)`
- Combined scripted UAT:
  - `bash tools/test.sh --layer-stress --macro-stress` -> PASS
  - `bash tools/test.sh --macro-stress --macro-iterations 200 --macro-read-timeout 1` -> PASS
