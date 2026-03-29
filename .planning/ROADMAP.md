# Roadmap: Linux FEA Keyboard Framework & Configurator Bridge

## Overview

This roadmap takes the project from protocol groundwork to a reusable Linux framework for FEA-based MonsGeek/Akko keyboards, with the MonsGeek M5W as the first fully verified target and the MonsGeek configurator bridge as the MVP. The approach remains bottom-up by dependency: protocol and transport first (Phases 1-2), then the gRPC-Web bridge that makes the configurator work on Linux (Phase 3), then systematic hardware verification of feature categories through that bridge (Phases 4-6), then CLI/service packaging (Phase 7), and finally firmware update capability (Phase 8). Phase 3 completion is still the MVP.

This roadmap supersedes earlier planning assumptions that turned out to be wrong on real hardware. The corrected transport facts are:

- M5W wired USB identity is VID `0x3151`, PID `0x4015`; the 2.4GHz dongle uses PID `0x4011`
- USB bus and address are runtime-discovered and must never be treated as stable identity
- USB PID alone is not a trustworthy device identity; the framework should identify devices primarily by firmware-reported device ID from `GET_USB_VERSION`
- `GET_USB_VERSION` reports the device ID as a 32-bit little-endian field
- `libusb` hotplug arrival was not reliable in this Linux environment; `udev` monitoring is the practical hot-plug mechanism
- Userspace sessions must hand `IF0` back to the kernel when they are done, unless a full userspace-input mode is intentionally active

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Project Scaffolding & Device Registry** - Rust workspace structure, FEA protocol constants, and JSON-driven device/profile registry with M5W as the first verified target
- [x] **Phase 2: FEA Protocol & HID Transport** - Protocol framing with checksums, raw HID I/O with safety guards, firmware-ID-aware discovery, non-root access via udev, and control-only default ownership
- [x] **Phase 3: gRPC-Web Bridge** - tonic-web server on localhost:3814 implementing the full iot_driver proto contract with CORS for browser access
- [ ] **Phase 4: Bridge Integration & Key Remapping** - End-to-end web configurator connection with verified key mapping and profile operations on real M5W hardware
- [x] **Phase 5: LED Control & Tuning** - RGB/LED modes and debounce/polling configuration verified on hardware, addressing the ghosting/double-letter issue (completed 2026-03-26)
- [x] **Phase 5.1: Userspace Input Daemon** (INSERTED) - Persistent daemon claiming IF0 for software debounce, correct key ordering, and uinput injection, bypassing compositor jitter (completed 2026-03-27)
- [ ] **Phase 6: Macros & Device-Specific Advanced Features** - Macro programming plus device-specific advanced switch features verified where supported by the target profile
- [x] **Phase 7: CLI & Service Deployment** - Command-line interface for all keyboard operations, systemd service for auto-start (completed 2026-03-28)
- [ ] **Phase 8: Firmware Update** - Firmware validation, bootloader entry with safety gates, chunk transfer with CRC-24 verification

## Phase Details

### Phase 1: Project Scaffolding & Device Registry
**Goal**: Establish the Rust workspace and data-driven device registry so all subsequent phases build on a shared foundation with correct device metadata and transport facts
**Depends on**: Nothing (first phase)
**Requirements**: REG-01, REG-02
**Success Criteria** (what must be TRUE):
  1. Cargo workspace compiles with three crates (monsgeek-protocol, monsgeek-transport, monsgeek-driver) and all dependencies resolve
  2. M5W device definition is loadable from JSON and contains the correct canonical wired USB identity (VID `0x3151`, PID `0x4015`), firmware device ID (`1308`), and key matrix identifier (`Common108_MG108B`)
  3. A new supported keyboard profile can be added primarily through registry/profile data rather than hardcoded runtime constants
  4. FEA command constants and protocol types (command opcodes, report structure, Bit7 checksum) are defined and unit-tested
**Plans**: 2 plans

Plans:
- [x] 01-01-PLAN.md — Workspace scaffolding, device types, M5W JSON definition, and device registry
- [x] 01-02-PLAN.md — FEA protocol constants, checksum algorithms, and protocol family detection

### Phase 2: FEA Protocol & HID Transport
**Goal**: Reliable, safe HID communication with FEA keyboards, first verified on the wired M5W, that handles real Linux and firmware quirks instead of assumed ones
**Depends on**: Phase 1
**Requirements**: HID-01, HID-02, HID-03, HID-04, HID-05, HID-06
**Success Criteria** (what must be TRUE):
  1. Driver detects the M5W dynamically on any USB bus/address and reports runtime transport info plus firmware device ID `1308`
  2. A `GET_USB_VERSION` command sent to the M5W returns a valid response whose 32-bit device ID field matches `1308`
  3. Commands sent faster than 100ms apart are automatically throttled by the transport layer (no firmware crash on rapid command sequences)
  4. Reset-then-reopen plus echo-matched query handling recover from the device's stale/PIPE error states
  5. Running the driver as a non-root user with udev rules installed successfully opens the HID device
  6. Userspace transport cleanup does not leave the keyboard non-functional; `IF0` is returned to the kernel unless a full userspace-input mode is intentionally active
**Plans**: 3 plans

Plans:
- [x] 02-01-PLAN.md — Transport error types, USB session with rusb control transfers, key matrix bounds validation, and udev rules
- [x] 02-02-PLAN.md — Flow control with echo matching, device discovery, transport thread with command channel and hot-plug detection
- [x] 02-03-PLAN.md — Hardware integration tests, ownership-mode closeout, and human verification on real M5W keyboard

### Phase 3: gRPC-Web Bridge
**Goal**: MonsGeek web configurator at app.monsgeek.com can connect to the locally running bridge and see the current keyboard through the reusable transport layer
**Depends on**: Phase 2
**Requirements**: GRPC-01, GRPC-02, GRPC-03, GRPC-04, GRPC-05, GRPC-06, GRPC-07, GRPC-08
**Success Criteria** (what must be TRUE):
  1. Server starts on localhost:3814 and accepts gRPC-Web connections from a browser at https://app.monsgeek.com
  2. Opening the MonsGeek web configurator in a browser shows the connected M5W keyboard in the device picker
  3. The web configurator can send a raw command (via sendRawFeature) and receive the keyboard's response (via readRawFeature)
  4. Plugging in or unplugging the keyboard triggers a device list update in the web configurator in real time
  5. The web app's key-value storage operations (insertDb/getItemFromDb) persist data across page reloads within a session
**Plans**: TBD

Plans:
- [x] 03-01: TBD
- [ ] 03-02: TBD
- [ ] 03-03: TBD

### Phase 4: Bridge Integration & Key Remapping
**Goal**: Users can read and modify key mappings and switch between profiles using the MonsGeek web configurator on Linux
**Depends on**: Phase 3
**Requirements**: KEYS-01, KEYS-02, KEYS-03
**Success Criteria** (what must be TRUE):
  1. User opens the web configurator, selects a key, and sees its current mapping for the active profile
  2. User remaps a key (e.g., Caps Lock to Ctrl) via the web configurator and the change takes effect immediately on the keyboard
  3. User switches between all 4 profiles and each profile retains its independent key mappings
**Plans**: 2 plans

Plans:
- [ ] 04-01-PLAN.md — Cache DeviceDefinition in ConnectedDevice, add SET_KEYMATRIX bounds validation at gRPC service boundary, unit tests
- [ ] 04-02-PLAN.md — Hardware integration tests for GET_KEYMATRIX, GET/SET_PROFILE, SET_KEYMATRIX roundtrip, browser verification checkpoint

### Phase 5: LED Control & Tuning
**Goal**: Users can control RGB lighting and tune debounce/polling to fix ghosting issues, all via the web configurator on Linux
**Depends on**: Phase 3
**Requirements**: LED-01, LED-02, TUNE-01, TUNE-02
**Success Criteria** (what must be TRUE):
  1. User reads current LED mode, brightness, speed, and color from the web configurator and values match what the keyboard displays
  2. User changes LED effect mode via the web configurator and the keyboard's lighting changes immediately
  3. User reads and adjusts debounce value via the web configurator
  4. User reads and adjusts polling rate via the web configurator
  5. After tuning debounce/polling, ghosting or double-letter input during normal typing is reduced (full resolution may require Phase 5.1 userspace input daemon)
**Plans**: 2 plans

Plans:
- [ ] 05-01-PLAN.md — CommandSchemaMap audit: add missing GET_REPORT/SET_REPORT shared entries, document checksum types, unit tests
- [ ] 05-02-PLAN.md — Hardware integration tests for LED read/write, polling rate probe, and browser verification checkpoint

### Phase 05.1: Userspace Input Daemon (INSERTED)
**Goal**: Persistent daemon that claims IF0 from the kernel, reads raw HID boot protocol reports, applies software debounce and correct key ordering, and injects cleaned events via uinput — eliminating compositor jitter and switch bounce from the keyboard input path
**Depends on**: Phase 2 (transport layer with IF0 claiming, InputProcessor, keymap infrastructure)
**Requirements**: INPUT-01, INPUT-02, INPUT-03, INPUT-04
**Success Criteria** (what must be TRUE):
  1. Daemon binary starts, claims IF0 from kernel, and keyboard input continues to work through uinput virtual device
  2. Software debounce filters switch bounce (6-12ms spacebar bounces measured in testing) without adding perceptible latency
  3. Same-report multi-key presses are delivered in deterministic order (releases before presses)
  4. Daemon coexists with gRPC bridge — bridge on IF2, daemon on IF0, both active simultaneously
  5. Compositor latency tracer shows reduced jitter compared to kernel usbhid path (p95 < 1ms target)
**Plans**: 3 plans

Plans:
- [x] 05.1-01-PLAN.md — SessionMode::InputOnly transport extension and monsgeek-inputd crate scaffold with config and uinput modules
- [x] 05.1-02-PLAN.md — Daemon main loop with IF0 polling, lifecycle management, signal handling, and sd_notify integration
- [x] 05.1-03-PLAN.md — Hardware integration tests and human verification of daemon on real M5W keyboard

### Phase 6: Macros & Device-Specific Advanced Features
**Goal**: Users can program macros and configure device-specific advanced switch features via the web configurator on Linux where the target profile supports them
**Depends on**: Phase 3
**Requirements**: MACR-01, MACR-02, MAG-01, MAG-02, MAG-03, MAG-04
**Success Criteria** (what must be TRUE):
  1. User reads existing macros from the keyboard via the web configurator
  2. User programs a new macro (key sequence with delays) and it executes correctly when triggered
  3. For devices that support magnetic or Hall-effect features, the user reads switch calibration state via the web configurator
  4. For devices that support Rapid Trigger or equivalent features, the user configures per-key actuation/reset points and the keyboard responds to the new thresholds
**Plans**: 2 plans

Plans:
- [ ] 06-01-PLAN.md — Extend validate_dangerous_write with SET_MACRO bounds, SET_FN bounds, and magnetic command gating, with unit tests
- [ ] 06-02-PLAN.md — Hardware macro round-trip tests, magnetic wire format unit tests, and browser macro verification checkpoint

### Phase 7: CLI & Service Deployment
**Goal**: Users can perform all keyboard operations from the command line and the bridge runs as a managed system service
**Depends on**: Phase 3
**Requirements**: CLI-01, CLI-02, GRPC-09
**Success Criteria** (what must be TRUE):
  1. User can query keyboard info, read/set LED mode, adjust debounce, change polling rate, switch profiles, and remap keys entirely from the command line
  2. CLI loads device definitions from the JSON registry (same registry as the bridge) without hardcoded device constants
  3. Systemd service starts the bridge on boot and the MonsGeek web configurator connects without user intervention after login
  4. Systemd service restarts the bridge automatically if it crashes
**Plans**: 2 plans

Plans:
- [x] 07-01-PLAN.md — Build bridge-first `monsgeek-cli` with typed commands, selector resolution, and unsafe raw-write gating
- [x] 07-02-PLAN.md — Ship systemd deployment artifacts, operator scripts/docs, and service lifecycle verification

### Phase 8: Firmware Update
**Goal**: Users can safely flash new firmware to their M5W keyboard from Linux with full safety validation
**Depends on**: Phase 2
**Requirements**: FW-01, FW-02, FW-03, FW-04
**Success Criteria** (what must be TRUE):
  1. User can query the current firmware version of their keyboard
  2. Before flashing, the tool validates the firmware image (file size, CRC integrity) and rejects invalid images with a clear error
  3. User must explicitly confirm before the tool enters bootloader mode (with clear warning that this erases the app region)
  4. Firmware flashing completes successfully: chunks are transferred, CRC-24 is verified, keyboard reboots with the new firmware version
**Plans**: TBD

Plans:
- [x] 08-01: TBD
- [ ] 08-02: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 -> 4 -> 5 -> 6 -> 7 -> 8
Note: Phases 4, 5, and 6 all depend on Phase 3 and are independent of each other. They may execute in any order after Phase 3 completes.

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Project Scaffolding & Device Registry | 2/2 | Complete | 2026-03-19 |
| 2. FEA Protocol & HID Transport | 3/3 | Complete | 2026-03-23 |
| 3. gRPC-Web Bridge | 3/3 | Complete | 2026-03-25 |
| 4. Bridge Integration & Key Remapping | 0/2 | Not started | - |
| 5. LED Control & Tuning | 2/2 | Complete   | 2026-03-26 |
| 5.1. Userspace Input Daemon (INSERTED) | 3/3 | Complete   | 2026-03-27 |
| 6. Macros & Device-Specific Advanced Features | 0/2 | Not started | - |
| 7. CLI & Service Deployment | 2/2 | Complete | 2026-03-28 |
| 8. Firmware Update | 0/2 | Not started | - |
