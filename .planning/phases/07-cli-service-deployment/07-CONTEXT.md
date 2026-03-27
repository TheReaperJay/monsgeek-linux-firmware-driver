# Phase 07: CLI & Service Deployment - Context

**Gathered:** 2026-03-27
**Status:** Ready for planning

<domain>
## Phase Boundary

Deliver a production CLI for keyboard operations and deploy bridge/input services via systemd, while staying fully aligned with the existing web app contract and central registry model. This phase clarifies operation and packaging; it does not add new keyboard capabilities beyond what bridge/protocol already support.

</domain>

<decisions>
## Implementation Decisions

### CLI Execution Path
- **D-01:** `monsgeek-cli` defaults to calling `DriverGrpc` on `127.0.0.1:3814` instead of re-implementing transport logic.
- **D-02:** CLI and web app must share behavior via the same bridge command path and policy gates (bounds/capability/synthetic-read behavior).
- **D-03:** Direct-transport mode is not a Phase 07 requirement; keep one primary path to avoid divergence from web compatibility.

### CLI Surface & Safety
- **D-04:** CLI ships typed subcommands for core operations: `devices list`, `info`, `led get/set`, `debounce get/set`, `poll get/set`, `profile get/set`, `keymap get/set`, `macro get/set`, `raw send/read`.
- **D-05:** `raw` writes are explicitly gated behind an unsafe flag (e.g. `--unsafe`) and keep current policy enforcement semantics.
- **D-06:** Non-raw typed commands are safe-by-default and must use existing bridge-side validation/policy behavior.

### Device Selection UX
- **D-07:** If exactly one supported online device is available, CLI auto-selects it.
- **D-08:** If multiple supported devices are online, command must fail with a clear selector prompt/list (no implicit random selection).
- **D-09:** Device selectors include:
  - `--model <slug>` (first-class; examples: `monsgeek-m5w`, `akko-<model>`)
  - `--path <bridge-path>`
  - `--device-id <firmware-id>`
- **D-10:** Model selector resolves through central registry/profile data (no hardcoded per-model command tables in CLI logic).

### Service Deployment Topology
- **D-11:** Ship `monsgeek-driver.service` with boot auto-start and restart-on-failure.
- **D-12:** Ship `monsgeek-inputd.service` (or template unit) in the same phase so userspace input ownership is managed consistently with bridge lifecycle.
- **D-13:** Service packaging and docs must preserve current ownership split: bridge on control/vendor path, input daemon on input path.

### the agent's Discretion
- CLI output formatting details (table vs compact), as long as machine-readable options remain script-friendly.
- Unit file placement specifics and install helper scripting, as long as enable/start flow is documented and deterministic.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase Scope & Acceptance
- `.planning/ROADMAP.md` — Phase 7 boundary, goal, success criteria, and dependency constraints.
- `.planning/REQUIREMENTS.md` — `CLI-01`, `CLI-02`, and `GRPC-09` requirement definitions.
- `.planning/PROJECT.md` — project-level constraints: web compatibility target, registry-first architecture, and Linux-only scope.

### Bridge Contract & Runtime Behavior
- `crates/monsgeek-driver/proto/driver.proto` — canonical gRPC contract that CLI-facing bridge behavior must remain compatible with.
- `.planning/phases/03-grpc-web-bridge/03-CONTEXT.md` — locked bridge contract and service-behavior decisions.
- `.planning/phases/06-macros-device-specific-advanced-features/06-02-SUMMARY.md` — runtime alias/discovery stability decisions that CLI device targeting must respect.

### Input Daemon & Service Lifecycle
- `.planning/phases/05.1-userspace-input-daemon/05.1-CONTEXT.md` — existing input-daemon lifecycle and coexistence decisions.
- `crates/monsgeek-inputd/src/main.rs` — current daemon CLI entrypoints and flags.
- `crates/monsgeek-driver/src/main.rs` — bridge startup/shutdown/takeover behavior relevant to service unit design.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/monsgeek-driver/src/service/mod.rs`: established policy enforcement and device path resolution semantics usable by CLI-through-bridge workflows.
- `crates/monsgeek-protocol/src/registry.rs`: runtime-aware registry lookups (`find_by_runtime_vid_pid`, `supports_runtime_vid_pid`) for model-aware selection.
- `crates/monsgeek-inputd/src/main.rs` + `src/config.rs`: existing CLI/config pattern for daemon flags and precedence.

### Established Patterns
- Registry is single source of truth for device/model capabilities and runtime aliases.
- Bridge service enforces safety/policy centrally; clients should not fork policy logic.
- Service lifecycle and graceful shutdown are already implemented in driver/inputd binaries and should be packaged, not re-architected.

### Integration Points
- Add a dedicated CLI crate/binary that talks to `DriverGrpc`.
- Add systemd unit files and install docs/scripts in repo packaging path.
- Extend test harness (`tools/test.sh` and/or new CLI smoke script) to validate CLI+service startup/selection behavior.

</code_context>

<specifics>
## Specific Ideas

- Device targeting should be human-readable by model slug, e.g. `monsgeek-m5w` and `akko-<model>`.
- Keep compatibility strict: the web app remains source-of-truth surface; CLI should align to the same backend behavior.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 07-cli-service-deployment*
*Context gathered: 2026-03-27*
