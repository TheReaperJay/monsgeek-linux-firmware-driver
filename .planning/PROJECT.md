# MonsGeek Linux Driver & Configurator Bridge

## What This Is

A standalone Linux driver and configuration bridge for MonsGeek keyboards using the yc3121 SoC (VID `0x3141`). It enables full keyboard configuration — key remapping, RGB lighting, macros, firmware updates, debounce tuning, and profile management — on Linux, where MonsGeek officially supports only Windows and macOS. The primary target is the MonsGeek M5W, with extensibility for all yc3121-based MonsGeek boards.

## Core Value

The MonsGeek configurator must work on Linux — enabling the user to configure, tune, and flash their keyboard without ever needing a Windows machine.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] gRPC-Web bridge server that serves HID commands on localhost:3814, allowing the MonsGeek web/Electron configurator to communicate with the keyboard
- [ ] HID device detection and enumeration for yc3121-based keyboards (VID 0x3141)
- [ ] Full FEA command protocol implementation (send/receive with Bit7 checksums)
- [ ] Key remapping via SET/GET_KEYMATRIX commands
- [ ] RGB/LED control via SET/GET_LEDPARAM commands
- [ ] Macro programming via SET/GET_MACRO commands
- [ ] Profile management (4 profiles) via SET/GET_PROFILE commands
- [ ] Polling rate and debounce configuration via SET/GET_REPORT and SET/GET_DEBOUNCE
- [ ] Firmware update capability (bootloader entry, chunk transfer, CRC-24 verification)
- [ ] Fix ghosting/double-letter issues on Linux (likely via debounce/polling config; eBPF driver if needed)
- [ ] Extensible device registry for adding other yc3121-based MonsGeek keyboards
- [ ] udev rules for non-root HID access on Linux

### Out of Scope

- Bluetooth LE transport — M5W is wired/2.4GHz; BLE support deferred
- 2.4GHz dongle transport — wired USB first; dongle support is a future milestone
- GUI application — the bridge enables the existing MonsGeek web configurator; no custom GUI
- Windows/macOS support — this is a Linux-only solution
- Akko keyboard support — different VID (0x3151), different firmware; the Akko reference project handles those
- Audio-reactive LEDs, screen color sync — advanced features from reference project; not in v1

## Context

**Reference materials available in this repo:**

- `references/monsgeek-akko-linux/` — A complete Rust implementation for Akko keyboards (AT32F405/VID 0x3151) using the same FEA command protocol. This project demonstrates the gRPC-Web bridge architecture, transport abstractions, eBPF HID driver, and full protocol implementation. It is reference only — not a dependency.

- `firmware/MonsGeek_v4_setup_500.2.13_WIN2026032/` — The extracted MonsGeek Windows Electron app (v500.2.13). Contains:
  - `iot_driver.exe` — the Windows HID communication binary
  - `dist/index.eb7071d5.js` — 41MB minified React app with all device definitions, FEA commands, and key matrices
  - `APPVersion.json`, `CurrentCompany.json`, device configuration

- `firmware/m5w_firmware_v103.bin` — M5W firmware binary (v1.03, 277KB)

**M5W device specifics:**
- Device ID: 1308
- VID: 0x3141, PID: 0x4005
- SoC: yc3121 ("yc3121_m5w_soc")
- Key layout: Common108_MG108B
- HID interface: IF2 vendor config (Feature Reports, 64 bytes)

**Known Linux issues:**
- Ghosting and double-letter input during normal typing — likely a debounce/polling rate issue solvable via configuration
- No HID access for vendor-specific features — Linux kernel's generic HID driver handles basic input but cannot access IF2 for configuration
- MonsGeek web configurator cannot reach the keyboard without the `iot_driver.exe` bridge

**Protocol compatibility:**
- The yc3121 keyboards use the same FEA command protocol structure as the AT32F405 (Akko) keyboards: same command opcodes, same Bit7 checksum, same 64-byte report format
- Key differences: different VID (0x3141 vs 0x3151), different device IDs, potentially different key matrices and feature sets

## Constraints

- **Platform**: Linux only — no Windows machine available; must work on Fedora (kernel 6.19+)
- **Architecture**: Standalone — must not depend on the monsgeek-akko-linux project at runtime; reference for protocol knowledge only
- **Extensibility**: Device registry must support adding new yc3121 keyboards by adding device definitions, not code changes
- **Compatibility**: Must work with the existing MonsGeek web configurator (app.monsgeek.com) or Electron app via the gRPC-Web bridge on localhost:3814
- **Safety**: Firmware flashing is destructive (bootloader erases app region before USB init); must have explicit user confirmation and validation before entering bootloader mode

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Rust for implementation | Same language as reference project; direct HID access via hidapi; gRPC via tonic; proven approach | — Pending |
| gRPC-Web bridge on :3814 | The MonsGeek web app expects this endpoint; matching it enables zero-modification browser compatibility | — Pending |
| Configurator-first priority | Fixing typing issues (ghosting/double-letters) likely achievable by tuning debounce/polling via configurator; eliminates need for kernel driver if successful | — Pending |
| eBPF HID driver deferred | Only needed if configurator-based debounce/polling adjustments don't resolve typing issues; reference project shows how to implement if needed | — Pending |

---
*Last updated: 2026-03-19 after initialization*
