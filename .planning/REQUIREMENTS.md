# Requirements: MonsGeek Linux Driver & Configurator Bridge

**Defined:** 2026-03-19
**Core Value:** The MonsGeek configurator must work on Linux — enabling the user to configure, tune, and flash their keyboard without ever needing a Windows machine.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### HID Transport

- [ ] **HID-01**: Driver detects and enumerates all yc3121-based MonsGeek keyboards (VID 0x3141) connected via USB
- [ ] **HID-02**: Driver sends FEA commands and receives responses using 64-byte HID Feature Reports with Bit7 checksums
- [ ] **HID-03**: Driver enforces mandatory 100ms inter-command delay to prevent yc3121 firmware crash/stall
- [ ] **HID-04**: Driver handles Linux hidraw stale read issue via retry-and-match loop (echo byte verification)
- [ ] **HID-05**: Driver validates all write indices against key matrix bounds before sending to prevent firmware OOB corruption
- [ ] **HID-06**: udev rules enable non-root HID access for yc3121 keyboards on Linux

### gRPC Bridge

- [ ] **GRPC-01**: Server listens on localhost:3814 and accepts gRPC-Web connections from browser
- [ ] **GRPC-02**: Server implements `sendRawFeature` RPC to forward raw HID commands to keyboard
- [ ] **GRPC-03**: Server implements `readRawFeature` RPC to read raw HID responses from keyboard
- [ ] **GRPC-04**: Server implements `watchDevList` RPC to stream device connect/disconnect events
- [ ] **GRPC-05**: Server implements `getVersion` RPC returning driver version info
- [ ] **GRPC-06**: Server implements `insertDb` and `getItemFromDb` RPCs for web app key-value storage
- [ ] **GRPC-07**: Server sends correct CORS headers so MonsGeek web configurator can connect from browser
- [ ] **GRPC-08**: Server matches the Windows `iot_driver.exe` proto contract exactly (including field names like `VenderMsg`, `DangleDevType`)
- [ ] **GRPC-09**: Systemd service unit enables auto-start on boot with managed lifecycle

### Key Remapping

- [ ] **KEYS-01**: User can read current key mapping for any profile via GET_KEYMATRIX
- [ ] **KEYS-02**: User can remap any key on any layer via SET_KEYMATRIX
- [ ] **KEYS-03**: User can switch between 4 profiles via SET_PROFILE / GET_PROFILE

### RGB/LED Control

- [ ] **LED-01**: User can read current LED mode, brightness, speed, and color via GET_LEDPARAM
- [ ] **LED-02**: User can set LED mode, brightness, speed, and color via SET_LEDPARAM

### Debounce & Polling

- [ ] **TUNE-01**: User can read and set debounce value via GET_DEBOUNCE / SET_DEBOUNCE
- [ ] **TUNE-02**: User can read and set polling rate via GET_REPORT / SET_REPORT

### Macros

- [ ] **MACR-01**: User can read existing macros via GET_MACRO
- [ ] **MACR-02**: User can program macros (key sequences with delays) via SET_MACRO

### Magnetic Switch / Rapid Trigger

- [ ] **MAG-01**: User can read magnetic switch calibration via GET_MAGNETISM_CAL
- [ ] **MAG-02**: User can calibrate magnetic switches via SET_MAGNETISM_CAL
- [ ] **MAG-03**: User can read per-key Rapid Trigger configuration via GET_MULTI_MAGNETISM
- [ ] **MAG-04**: User can set per-key Rapid Trigger actuation/reset points via SET_MULTI_MAGNETISM

### Firmware Management

- [ ] **FW-01**: User can read keyboard firmware version via GET_USB_VERSION and GET_REV
- [ ] **FW-02**: User can flash firmware via bootloader entry (0x7F + magic word), chunk transfer, and CRC-24 verification
- [ ] **FW-03**: Firmware flashing requires explicit user confirmation before entering bootloader (destructive: erases app region before USB init)
- [ ] **FW-04**: Firmware flashing validates firmware image integrity (size, CRC) before initiating bootloader entry

### CLI

- [ ] **CLI-01**: User can perform all keyboard operations (query, set, flash) via command-line interface
- [ ] **CLI-02**: CLI uses JSON-driven device registry for extensible keyboard definitions (adding keyboards requires data, not code)

### Device Registry

- [ ] **REG-01**: Device registry contains M5W definition (VID 0x3141, PID 0x4005, key matrix Common108_MG108B, device ID 1308)
- [ ] **REG-02**: Device registry is extensible — adding a new yc3121 keyboard requires only a JSON definition file

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Wireless Transport

- **WIRE-01**: 2.4GHz dongle transport with flow control (poll GET_DONGLE_STATUS, retrieve GET_CACHED_RESPONSE)
- **WIRE-02**: Bluetooth LE transport via GATT HOGP

### Kernel Driver

- **KERN-01**: eBPF HID driver to fix ghosting/double-letter issues at kernel level (if configurator-based debounce tuning is insufficient)

### Advanced Features

- **ADV-01**: TUI (terminal UI) for interactive keyboard control
- **ADV-02**: Audio-reactive LED effects
- **ADV-03**: Screen color sync for LED effects

## Out of Scope

| Feature | Reason |
|---------|--------|
| Custom GUI application | gRPC bridge enables existing MonsGeek web configurator — no custom GUI needed |
| Windows/macOS support | Linux-only solution by design |
| Akko keyboard support | Different VID (0x3151), different firmware — handled by monsgeek-akko-linux project |
| Bluetooth LE transport | M5W is wired; BLE deferred to v2 |
| 2.4GHz dongle transport | Wired USB first; dongle deferred to v2 |
| eBPF HID kernel driver | Only if configurator debounce tuning fails to fix typing issues |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| HID-01 | — | Pending |
| HID-02 | — | Pending |
| HID-03 | — | Pending |
| HID-04 | — | Pending |
| HID-05 | — | Pending |
| HID-06 | — | Pending |
| GRPC-01 | — | Pending |
| GRPC-02 | — | Pending |
| GRPC-03 | — | Pending |
| GRPC-04 | — | Pending |
| GRPC-05 | — | Pending |
| GRPC-06 | — | Pending |
| GRPC-07 | — | Pending |
| GRPC-08 | — | Pending |
| GRPC-09 | — | Pending |
| KEYS-01 | — | Pending |
| KEYS-02 | — | Pending |
| KEYS-03 | — | Pending |
| LED-01 | — | Pending |
| LED-02 | — | Pending |
| TUNE-01 | — | Pending |
| TUNE-02 | — | Pending |
| MACR-01 | — | Pending |
| MACR-02 | — | Pending |
| MAG-01 | — | Pending |
| MAG-02 | — | Pending |
| MAG-03 | — | Pending |
| MAG-04 | — | Pending |
| FW-01 | — | Pending |
| FW-02 | — | Pending |
| FW-03 | — | Pending |
| FW-04 | — | Pending |
| CLI-01 | — | Pending |
| CLI-02 | — | Pending |
| REG-01 | — | Pending |
| REG-02 | — | Pending |

**Coverage:**
- v1 requirements: 36 total
- Mapped to phases: 0
- Unmapped: 36

---
*Requirements defined: 2026-03-19*
*Last updated: 2026-03-19 after initial definition*
