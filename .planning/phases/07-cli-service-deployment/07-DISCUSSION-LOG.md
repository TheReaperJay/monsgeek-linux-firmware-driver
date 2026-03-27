# Phase 07: CLI & Service Deployment - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-03-27
**Phase:** 07-cli-service-deployment
**Areas discussed:** CLI execution path, CLI surface & safety, device selection UX, service deployment topology

---

## CLI Execution Path

| Option | Description | Selected |
|--------|-------------|----------|
| Bridge-first CLI | CLI calls `DriverGrpc` on `127.0.0.1:3814`, reusing existing policy and behavior | ✓ |
| Direct transport CLI | CLI talks to transport directly and bypasses bridge | |
| Hybrid mode | Support both from day one | |

**User's choice:** Bridge-first CLI path.
**Notes:** Keep CLI aligned with web app behavior; avoid behavior drift from duplicate stacks.

---

## CLI Surface & Safety

| Option | Description | Selected |
|--------|-------------|----------|
| Typed commands + unsafe raw gate | Typed subcommands for common operations; raw writes require explicit unsafe acknowledgement | ✓ |
| Fully raw CLI | Expose only raw send/read and push responsibility to user | |
| Typed commands without unsafe gate | Keep command UX simple but allow raw writes without explicit gate | |

**User's choice:** Typed command surface plus unsafe gating for raw writes.
**Notes:** Reuse existing bridge-side policy validation and preserve safe defaults.

---

## Device Selection UX

| Option | Description | Selected |
|--------|-------------|----------|
| Auto-single + explicit selector for multi-device | Auto-select only when one supported device is online; require selector otherwise | ✓ |
| Always explicit selector | Require selector even for single-device setups | |
| Auto-best-guess always | Always pick one automatically | |

**User's choice:** Auto-single with explicit selection when multiple devices exist.
**Notes:** Added model selector requirement: support `--model` (e.g. `monsgeek-m5w`, `akko-<model>`) alongside `--path` and `--device-id`.

---

## Service Deployment Topology

| Option | Description | Selected |
|--------|-------------|----------|
| Bridge service only | Package/enable only `monsgeek-driver.service` | |
| Bridge + input daemon services together | Package both `monsgeek-driver.service` and `monsgeek-inputd.service` in this phase | ✓ |
| Manual run only | No systemd deployment in this phase | |

**User's choice:** Deploy both bridge and input daemon service units in this phase.
**Notes:** Keep existing ownership split and restart behavior explicit in unit definitions/documentation.

---

## the agent's Discretion

- Exact CLI output layout and optional machine-readable mode details.
- Unit file directory structure and install helper scripting mechanics.

## Deferred Ideas

None.
