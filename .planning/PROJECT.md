# Linux FEA Keyboard Framework & Configurator Bridge

## What This Is

A Linux userspace framework and compatibility bridge for FEA-based keyboards, starting with the MonsGeek M5W as the first fully verified target. The immediate user-facing goal is to make the existing MonsGeek configurator work on Linux without requiring Windows. The broader engineering goal is a transport and profile architecture that can support other MonsGeek and Akko devices that share the FEA protocol once their transport details and feature profiles are validated.

The project is organized around three layers:

- `monsgeek-protocol`: protocol constants, checksums, command tables, device/profile data
- `monsgeek-transport`: raw USB HID transport, flow control, discovery, and hot-plug
- bridge / CLI layer: gRPC-Web compatibility bridge for the configurator and direct operator tooling

## Core Value

The MonsGeek configurator must work on Linux, enabling users to configure, tune, and eventually flash supported keyboards without ever needing a Windows machine.

## Requirements

- The project must provide a safe Linux transport layer for FEA keyboards that tolerates known firmware and kernel-probe quirks.
- The project must expose a local compatibility bridge that matches the MonsGeek configurator's expected gRPC-Web contract on `localhost:3814`.
- The registry and profile model must stay data-driven so adding supported devices does not require scattered runtime constants.
- Device identity must resolve by firmware device ID plus transport classification, not by USB PID alone.

## Validated So Far

- JSON-driven device/profile registry foundation exists and compiles
- Wired M5W transport works on Linux via `rusb` HID control transfers on `IF2`
- `GET_USB_VERSION` works on real hardware and returns device ID `1308`
- `GET_USB_VERSION` device identity is a 32-bit little-endian field, not 16-bit
- The transport layer enforces the 100ms firmware safety delay
- Hot-plug detection works via `udev`
- Short-lived transport sessions can return `IF0` to the kernel so the keyboard keeps typing after tests
- LED control works end-to-end: GET/SET_LEDPARAM verified on real M5W hardware and through web configurator (Validated in Phase 5)
- Debounce tuning works end-to-end: GET/SET_DEBOUNCE verified on real hardware and through web configurator (Validated in Phase 5)
- M5W supports GET_REPORT (polling rate query) at 8kHz despite device definition listing get_report: None (Discovered in Phase 5)
- GET_LEDPARAM uses Bit7 checksum; only SET_LEDPARAM uses Bit8 (Corrected in Phase 5)
- Userspace input daemon (`monsgeek-inputd`) works end-to-end: claims IF0/IF1 via `SessionMode::InputOnly`, processes HID boot protocol through `InputProcessor` with 15ms debounce, injects events via uinput, coexists with gRPC bridge on IF2 (Validated in Phase 5.1)
- Daemon survives disconnect/reconnect cycles via udev monitoring with 2s firmware settle time (Validated in Phase 5.1)
- M5W presents as two USB devices on the same bus: PID 0x4011 (receiver/secondary) and PID 0x4015 (keyboard). Probing PID 0x4011 poisons the firmware's command state for PID 0x4015. Non-probing VID/PID discovery avoids this (Discovered in Phase 5.1)
- `monsgeek-cli` provides typed operations (`devices/info/led/debounce/poll/profile/keymap/macro/raw`) against DriverGrpc with deterministic selectors and unsafe raw-write gating (Validated in Phase 07)
- Systemd deployment is operational for `monsgeek-driver` and `monsgeek-inputd`, including enable/start, CLI smoke against managed services, and restart-on-failure recovery (`NRestarts` increment) (Validated in Phase 07)

## Active Work

- Phase 07 complete — CLI + service deployment verified on host with managed services
- Next: Phase 08 — Firmware update
- Keep the registry/profile system data-driven so new supported keyboards do not require hardcoded runtime constants

## Deferred But Planned

- 2.4GHz dongle transport for M5W and related devices
- Additional validated device profiles beyond the M5W
- Firmware flashing with explicit safety gates

## Out of Scope

- A custom GUI application
- Windows or macOS runtime support
- Bluetooth LE transport in the current milestone
- Making unsupported promises for unverified devices or transports

## Context

### Key References In This Repo

- `references/monsgeek-hid-driver/`
  - Primary reference for the exact M5W hardware target
  - Shows reset-then-reopen, IF0/IF1/IF2 claiming, and raw `rusb` control transfers

- `references/monsgeek-akko-linux/`
  - Stronger architectural reference for the broader FEA family
  - Shows transport layering, gRPC-Web bridge structure, profile registry patterns, and dongle support concepts
  - Must be relied on for architecture and protocol-family modeling, not blindly copied

- `firmware/MonsGeek_v4_setup_500.2.13_WIN2026032/`
  - Extracted Windows/Electron application
  - Useful for device data, protocol behavior, and app/bridge expectations

- `firmware/m5w_firmware_v103.bin`
  - M5W firmware image for later analysis and firmware-management work

### Verified M5W Facts

- Firmware device ID: `1308`
- Wired USB identity: VID `0x3151`, PID `0x4015`
- 2.4GHz dongle identity: VID `0x3151`, PID `0x4011`
- SoC family: `yc3121`
- Key layout: `Common108_MG108B`
- Transport interface: `IF2` vendor HID feature reports, 64-byte payloads

### Verified Linux / Firmware Constraints

- The firmware crashes or stalls if commands arrive faster than 100ms apart
- IF1 and IF2 have broken report-descriptor behavior during kernel probing
- `GET_USB_VERSION` is the right identity probe and carries a 32-bit device ID
- USB bus and address are runtime-discovered and must never be hardcoded
- USB PID is transport identity, not canonical model identity
- `libusb` arrival callbacks were not reliable enough on this Linux setup; `udev` is the practical hot-plug source

## Constraints

- **Platform:** Linux only, validated on Fedora
- **Architecture:** standalone project, no runtime dependency on the reference projects
- **Identity model:** supported devices should resolve by firmware device ID plus transport classification, not PID alone
- **Extensibility:** new supported keyboards should be added through profile/registry data plus bounded transport classification, not scattered constants
- **Compatibility:** the local bridge must match the MonsGeek configurator's expected gRPC-Web contract
- **Safety:** firmware update remains high-risk and must stay behind explicit validation and user confirmation

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| `rusb` for MonsGeek transport | The M5W's Linux behavior is defined by raw USB/HID control transfers and broken descriptor probing; `hidapi`/hidraw assumptions are not the reliable source of truth here | Active |
| gRPC-Web bridge on `127.0.0.1:3814` | The configurator expects a local bridge that matches the Windows `iot_driver.exe` behavior | Planned |
| Firmware device ID is canonical identity | USB PID varies by transport and is not sufficient as the framework-wide model identifier | Active |
| `udev` for hot-plug | Verified to be more reliable than `libusb` arrival callbacks in this environment | Active |
| Preserve kernel typing unless intentionally taking ownership | Short-lived transport sessions must not leave the keyboard dead; long-lived sessions need an explicit mode choice | Active |
| M5W first, framework-general architecture | The M5W is the first verified target, but the code should not hardcode MonsGeek-only assumptions if the FEA family can share abstractions | Active |

---
*Last updated: 2026-03-28 — Phase 07 closeout (CLI + systemd service deployment verified)*
