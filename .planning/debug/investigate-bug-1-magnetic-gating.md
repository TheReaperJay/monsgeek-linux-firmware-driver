---
status: diagnosed
trigger: "Investigate Bug 1 in /home/whitebehemoth/Dev/Projects/monsgeek-firmware-driver.

Goal:
- Determine why magnetic command gating currently causes web app hangs.
- Produce a concrete patch strategy aligned with existing architecture.

Context:
- Current code path: crates/monsgeek-driver/src/service/mod.rs (`validate_dangerous_write`, `send_command_rpc`, `read_response_rpc`, gRPC `send_msg`/`read_msg`).
- Symptom: magnetic command gating on non-magnetic devices returns an error path instead of a synthetic empty read response, and browser flow hangs.
- Compare behavior with references:
  - references/monsgeek-akko-linux/iot_driver_linux/src/grpc.rs
  - references/monsgeek-hid-driver (if useful)

Deliverables:
1) Root-cause explanation (where hang is introduced in send/read sequence).
2) Minimal safe patch design, including exact structs/functions to add/modify.
3) Recommended tests to add in crates/monsgeek-driver (unit/integration) validating non-blocking behavior.
4) List file paths touched by your proposed fix.

Constraints:
- Do not implement code; analysis only.
- Be explicit about any assumptions."
created: 2026-03-27T15:00:03+07:00
updated: 2026-03-27T15:44:56+07:00
---

## Current Focus
<!-- OVERWRITE on each update - reflects NOW -->

hypothesis: Confirmed - send/read contract is broken when magnetic gating rejects in send path without guaranteeing a synthetic read completion.
test: Code-path tracing across `send_command_rpc` -> `send_msg` and `read_response_rpc` -> `read_msg`, plus transport read behavior and reference implementation comparison.
expecting: A blocked send must still result in a consumable read token; otherwise `read_msg` waits on hardware read path.
next_action: return diagnosis and concrete patch/test plan (analysis only, no code changes)

## Symptoms
<!-- Written during gathering, then IMMUTABLE -->

expected: Magnetic-gated writes on non-magnetic devices should not hang browser flow; read side should receive a synthetic empty response to preserve RPC sequence.
actual: Non-magnetic magnetic-gated writes return an error path instead of synthetic empty read response; browser flow hangs waiting for completion.
errors: Error path returned by dangerous write validation/gRPC write handling (exact message not provided by reporter).
reproduction: From web app, issue a magnetic command to non-magnetic device via service send/read RPC sequence.
started: Not explicitly provided by reporter (assumption: currently reproducible on latest local state).

## Eliminated
<!-- APPEND only - prevents re-investigating -->

## Evidence
<!-- APPEND only - facts discovered -->

- timestamp: 2026-03-27T15:03:16+07:00
  checked: `.planning/debug/knowledge-base.md`
  found: Knowledge base file does not exist in this repository.
  implication: No prior known-pattern match is available; investigation proceeds from primary code evidence.

- timestamp: 2026-03-27T15:09:42+07:00
  checked: `crates/monsgeek-driver/src/service/mod.rs` (`send_command_rpc`, `read_response_rpc`, gRPC `send_msg`/`read_msg`)
  found: `send_msg` delegates to `send_command_rpc`; `read_msg` delegates to `read_response_rpc`. Without a synthetic queue hit, `read_response_rpc` calls transport read.
  implication: Send and read are decoupled RPC steps; any send-side early rejection must still preserve read-side completion semantics to avoid blocking flows.

- timestamp: 2026-03-27T15:15:27+07:00
  checked: `crates/monsgeek-driver/src/bridge_transport.rs` and `crates/monsgeek-transport/src/lib.rs`
  found: `read_response_rpc` fallback path calls `read_feature_report()` via blocking task; this waits for transport thread result rather than returning immediate synthetic data.
  implication: If no synthetic response is queued for a blocked command, read path can stall while waiting on hardware response that may never arrive.

- timestamp: 2026-03-27T15:18:10+07:00
  checked: `crates/monsgeek-transport/src/controller.rs` and `crates/monsgeek-transport/src/usb.rs`
  found: transport read executes USB HID GET_REPORT with `USB_TIMEOUT = 1s`; repeated read/poll behavior in caller can appear as hang in browser flow.
  implication: Missing synthetic completion manifests as user-visible hangs/timeouts instead of immediate logical no-op response.

- timestamp: 2026-03-27T15:24:19+07:00
  checked: `references/monsgeek-akko-linux/iot_driver_linux/src/grpc.rs`
  found: Reference driver keeps straightforward send-then-read semantics; no dangerous-write gating layer or synthetic-read queue exists there.
  implication: Introducing validation gating in this repo adds a new send-side rejection path that must explicitly emulate read completion to preserve webapp contract.

- timestamp: 2026-03-27T15:31:54+07:00
  checked: `crates/monsgeek-driver/src/service/mod.rs` tests
  found: Unit tests cover `validate_dangerous_write` and `should_synthesize_empty_response` predicate, but there is no end-to-end send/read pairing test that asserts non-blocking behavior for blocked magnetic commands.
  implication: Regression risk exists: logic can drift so send returns error path again without automated guard catching browser-hang behavior.

## Resolution
<!-- OVERWRITE as understanding evolves -->

root_cause: Magnetic gating introduces an early send-side rejection path (`failed_precondition`) that can skip generation of the paired read completion token; subsequent `read_msg` falls through to blocking transport `read_feature_report`, which waits for a response that does not exist for blocked commands.
fix: Enqueue synthetic empty read when magnetic write is blocked (failed_precondition) and make read path consume synthetic queue before transport read; add regression tests asserting send success + non-blocking empty read on non-magnetic devices.
verification: Static code-path verification only in this session (no runtime execution against real web workflow).
files_changed:
  - crates/monsgeek-driver/src/service/mod.rs (proposed)
  - crates/monsgeek-driver/tests/grpc_contract_tests.rs (proposed)
