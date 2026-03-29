# Requirements: Linux FEA Keyboard Framework & Configurator Bridge

**Defined:** 2026-03-19
**Corrected:** 2026-03-23
**Core Value:** The MonsGeek configurator must work on Linux, enabling users to configure, tune, and eventually flash supported keyboards without ever needing a Windows machine.

## v1 Requirements

Requirements for the current milestone. These focus on a working Linux transport and bridge, with the MonsGeek M5W as the first fully verified target.

### HID Transport

- [x] **HID-01**: Driver detects and enumerates supported FEA keyboards connected via USB using runtime discovery plus registry/profile matching
- [x] **HID-02**: Driver sends FEA commands and receives responses using 64-byte HID Feature Reports on the vendor interface
- [x] **HID-03**: Driver enforces mandatory 100ms inter-command delay to prevent yc3121 firmware crash/stall
- [x] **HID-04**: Driver handles stale, mismatched, or transient bad-response states using echo-matched retry logic and reset/reopen recovery where required
- [x] **HID-05**: Driver validates all write indices against key matrix bounds before sending to prevent firmware OOB corruption
- [x] **HID-06**: udev rules enable non-root USB access for supported keyboards on Linux

### gRPC Bridge

- [x] **GRPC-01**: Server listens on localhost:3814 and accepts gRPC-Web connections from browser clients
- [x] **GRPC-02**: Server implements `sendRawFeature` RPC to forward raw HID commands to the keyboard
- [x] **GRPC-03**: Server implements `readRawFeature` RPC to read raw HID responses from the keyboard
- [x] **GRPC-04**: Server implements `watchDevList` RPC to stream device connect/disconnect events
- [x] **GRPC-05**: Server implements `getVersion` RPC returning driver version info
- [x] **GRPC-06**: Server implements `insertDb` and `getItemFromDb` RPCs for web app key-value storage
- [x] **GRPC-07**: Server sends the correct CORS headers so the MonsGeek web configurator can connect from browser context
- [x] **GRPC-08**: Server matches the Windows `iot_driver.exe` proto contract exactly, including upstream field-name quirks
- [x] **GRPC-09**: Systemd service unit enables auto-start on boot with managed lifecycle

### Key Remapping

- [ ] **KEYS-01**: User can read the current key mapping for any profile via GET_KEYMATRIX
- [x] **KEYS-02**: User can remap any key on any supported layer via SET_KEYMATRIX
- [ ] **KEYS-03**: User can switch between the keyboard's supported profiles via SET_PROFILE / GET_PROFILE

### RGB / LED Control

- [x] **LED-01**: User can read current LED mode, brightness, speed, and color via GET_LEDPARAM
- [x] **LED-02**: User can set LED mode, brightness, speed, and color via SET_LEDPARAM

### Debounce & Polling

- [x] **TUNE-01**: User can read and set debounce value via GET_DEBOUNCE / SET_DEBOUNCE
- [x] **TUNE-02**: User can read and set polling rate via GET_REPORT / SET_REPORT where supported

### Macros

- [ ] **MACR-01**: User can read existing macros via GET_MACRO
- [x] **MACR-02**: User can program macros via SET_MACRO

### Device-Specific Advanced Features

- [x] **MAG-01**: For device profiles that support them, user can read advanced switch calibration state
- [x] **MAG-02**: For device profiles that support them, user can calibrate advanced switch behavior
- [x] **MAG-03**: For device profiles that support them, user can read per-key rapid-trigger style configuration
- [x] **MAG-04**: For device profiles that support them, user can set per-key actuation/reset points

### Userspace Input Daemon

- [x] **INPUT-01**: Persistent daemon claims IF0 from the kernel, reads HID boot protocol reports, applies software debounce, and injects cleaned key events via uinput
- [x] **INPUT-02**: Daemon corrects same-report key ordering by processing releases before presses and applying deterministic ordering
- [x] **INPUT-03**: Daemon runs as a separate binary from the gRPC bridge, with independent lifecycle (keyboard works regardless of whether the configurator is running)
- [x] **INPUT-04**: Daemon coexists with the gRPC bridge — bridge claims IF2 for vendor commands while daemon claims IF0 for input

### Firmware Management

- [ ] **FW-01**: User can read keyboard firmware version via GET_USB_VERSION and GET_REV where available
- [ ] **FW-02**: User can flash firmware via bootloader entry, chunk transfer, and CRC validation
- [ ] **FW-03**: Firmware flashing requires explicit user confirmation before entering bootloader
- [ ] **FW-04**: Firmware flashing validates firmware image integrity before initiating bootloader entry

### CLI

- [x] **CLI-01**: User can perform core keyboard operations via command-line interface
- [x] **CLI-02**: CLI uses the same JSON-driven registry/profile data as the bridge

### Device Registry

- [x] **REG-01**: Device registry contains the corrected M5W definition: VID `0x3151`, PID `0x4015`, key matrix `Common108_MG108B`, device ID `1308`
- [x] **REG-02**: Device registry is extensible; adding a supported keyboard profile is primarily data-driven rather than scattered hardcoded constants

## v2 / Follow-On Requirements

Deferred until the core wired bridge is stable.

### Wireless Transport

- **WIRE-01**: 2.4GHz dongle transport with appropriate flow control and response retrieval
- **WIRE-02**: Bluetooth LE transport via GATT / HOGP where applicable

### Advanced Tooling

- **ADV-01**: TUI for interactive keyboard control
- **ADV-02**: Audio-reactive LED effects
- **ADV-03**: Screen color sync for LED effects

## Out of Scope For The Current Milestone

| Feature | Reason |
|---------|--------|
| Custom GUI application | The bridge exists specifically to reuse the existing configurator |
| Windows/macOS runtime support | Linux-only project by design |
| Broad promises for unvalidated keyboards | Architecture should be general, but support claims must follow real profile/transport validation |
| Bluetooth LE transport | Deferred until wired and dongle paths are stable |
| Audio-reactive LEDs / screen sync | Not part of the configurator-compatibility MVP |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| REG-01 | Phase 1 | Complete |
| REG-02 | Phase 1 | Complete |
| HID-01 | Phase 2 | Complete |
| HID-02 | Phase 2 | Complete |
| HID-03 | Phase 2 | Complete |
| HID-04 | Phase 2 | Complete |
| HID-05 | Phase 2 | Complete |
| HID-06 | Phase 2 | Complete |
| GRPC-01 | Phase 3 | Complete |
| GRPC-02 | Phase 3 | Complete |
| GRPC-03 | Phase 3 | Complete |
| GRPC-04 | Phase 3 | Complete |
| GRPC-05 | Phase 3 | Complete |
| GRPC-06 | Phase 3 | Complete |
| GRPC-07 | Phase 3 | Complete |
| GRPC-08 | Phase 3 | Complete |
| KEYS-01 | Phase 4 | Pending |
| KEYS-02 | Phase 4 | Complete |
| KEYS-03 | Phase 4 | Pending |
| LED-01 | Phase 5 | Complete |
| LED-02 | Phase 5 | Complete |
| TUNE-01 | Phase 5 | Complete |
| TUNE-02 | Phase 5 | Complete |
| MACR-01 | Phase 6 | Pending |
| MACR-02 | Phase 6 | Complete |
| MAG-01 | Phase 6 | Complete |
| MAG-02 | Phase 6 | Complete |
| MAG-03 | Phase 6 | Complete |
| MAG-04 | Phase 6 | Complete |
| INPUT-01 | Phase 5.1 | Complete |
| INPUT-02 | Phase 5.1 | Complete |
| INPUT-03 | Phase 5.1 | Complete |
| INPUT-04 | Phase 5.1 | Complete |
| CLI-01 | Phase 7 | Complete |
| CLI-02 | Phase 7 | Complete |
| GRPC-09 | Phase 7 | Complete |
| FW-01 | Phase 8 | Pending |
| FW-02 | Phase 8 | Pending |
| FW-03 | Phase 8 | Pending |
| FW-04 | Phase 8 | Pending |

**Coverage:**
- v1 requirements: 40 total
- Mapped to phases: 40
- Unmapped: 0

---
*Last updated: 2026-03-28 after Phase 07 closeout verification*
