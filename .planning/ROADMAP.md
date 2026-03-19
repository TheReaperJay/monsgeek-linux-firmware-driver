# Roadmap: MonsGeek Linux Driver & Configurator Bridge

## Overview

This roadmap takes the MonsGeek Linux driver from bare metal to full configurator bridge. The approach is bottom-up by dependency: protocol and transport first (Phases 1-2), then the gRPC bridge that makes the web configurator work (Phase 3), then systematic hardware verification of each keyboard feature category through the bridge (Phases 4-6), then power-user tooling (Phase 7), and finally the high-risk firmware update capability (Phase 8). Phase 3 completion is the MVP: the MonsGeek web configurator works on Linux. Phases 4-6 verify and harden every feature category against real yc3121 hardware. Phase 8 is intentionally last due to the destructive nature of bootloader entry.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [ ] **Phase 1: Project Scaffolding & Device Registry** - Rust workspace structure, FEA protocol constants, and JSON-driven device registry with M5W definition
- [ ] **Phase 2: FEA Protocol & HID Transport** - Protocol framing with checksums, raw HID I/O with safety guards, and non-root access via udev
- [ ] **Phase 3: gRPC-Web Bridge** - tonic-web server on localhost:3814 implementing the full iot_driver proto contract with CORS for browser access
- [ ] **Phase 4: Bridge Integration & Key Remapping** - End-to-end web configurator connection with verified key mapping and profile operations on real M5W hardware
- [ ] **Phase 5: LED Control & Tuning** - RGB/LED modes and debounce/polling configuration verified on hardware, addressing the ghosting/double-letter issue
- [ ] **Phase 6: Macros & Magnetic Switches** - Macro programming and magnetic switch calibration/Rapid Trigger verified on hardware
- [ ] **Phase 7: CLI & Service Deployment** - Command-line interface for all keyboard operations, systemd service for auto-start
- [ ] **Phase 8: Firmware Update** - Firmware validation, bootloader entry with safety gates, chunk transfer with CRC-24 verification

## Phase Details

### Phase 1: Project Scaffolding & Device Registry
**Goal**: Establish the Rust workspace and device registry so all subsequent phases build on a shared foundation with correct M5W device constants
**Depends on**: Nothing (first phase)
**Requirements**: REG-01, REG-02
**Success Criteria** (what must be TRUE):
  1. Cargo workspace compiles with three crates (monsgeek-protocol, monsgeek-transport, monsgeek-driver) and all dependencies resolve
  2. M5W device definition is loadable from JSON and contains correct VID (0x3141), PID (0x4005), device ID (1308), and key matrix identifier (Common108_MG108B)
  3. A new yc3121 keyboard can be added by creating a JSON file without modifying any Rust source code
  4. FEA command constants and protocol types (command opcodes, report structure, Bit7 checksum) are defined and unit-tested
**Plans**: 2 plans

Plans:
- [ ] 01-01-PLAN.md — Workspace scaffolding, device types, M5W JSON definition, and device registry
- [ ] 01-02-PLAN.md — FEA protocol constants, checksum algorithms, and protocol family detection

### Phase 2: FEA Protocol & HID Transport
**Goal**: Reliable, safe HID communication with yc3121 keyboards that handles all known hardware quirks
**Depends on**: Phase 1
**Requirements**: HID-01, HID-02, HID-03, HID-04, HID-05, HID-06
**Success Criteria** (what must be TRUE):
  1. Driver detects the M5W keyboard when connected via USB and reports its VID, PID, and device ID
  2. A GET_USB_VERSION command sent to the M5W returns a valid response with device ID 1308
  3. Commands sent faster than 100ms apart are automatically throttled by the transport layer (no firmware crash on rapid command sequences)
  4. SET followed by GET for the same parameter returns the updated value (stale-read retry logic works)
  5. Running the driver as a non-root user with udev rules installed successfully opens the HID device
**Plans**: TBD

Plans:
- [ ] 02-01: TBD
- [ ] 02-02: TBD
- [ ] 02-03: TBD

### Phase 3: gRPC-Web Bridge
**Goal**: MonsGeek web configurator at app.monsgeek.com can connect to the locally running bridge and see the keyboard
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
- [ ] 03-01: TBD
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
**Plans**: TBD

Plans:
- [ ] 04-01: TBD

### Phase 5: LED Control & Tuning
**Goal**: Users can control RGB lighting and tune debounce/polling to fix ghosting issues, all via the web configurator on Linux
**Depends on**: Phase 3
**Requirements**: LED-01, LED-02, TUNE-01, TUNE-02
**Success Criteria** (what must be TRUE):
  1. User reads current LED mode, brightness, speed, and color from the web configurator and values match what the keyboard displays
  2. User changes LED effect mode via the web configurator and the keyboard's lighting changes immediately
  3. User reads and adjusts debounce value via the web configurator
  4. User reads and adjusts polling rate via the web configurator
  5. After tuning debounce/polling, ghosting or double-letter input during normal typing is resolved or significantly reduced
**Plans**: TBD

Plans:
- [ ] 05-01: TBD
- [ ] 05-02: TBD

### Phase 6: Macros & Magnetic Switches
**Goal**: Users can program macros and configure magnetic switch behavior via the web configurator on Linux
**Depends on**: Phase 3
**Requirements**: MACR-01, MACR-02, MAG-01, MAG-02, MAG-03, MAG-04
**Success Criteria** (what must be TRUE):
  1. User reads existing macros from the keyboard via the web configurator
  2. User programs a new macro (key sequence with delays) and it executes correctly when triggered
  3. User reads magnetic switch calibration state via the web configurator
  4. User configures per-key Rapid Trigger actuation and reset points and the keyboard responds to the new thresholds
**Plans**: TBD

Plans:
- [ ] 06-01: TBD
- [ ] 06-02: TBD

### Phase 7: CLI & Service Deployment
**Goal**: Users can perform all keyboard operations from the command line and the bridge runs as a managed system service
**Depends on**: Phase 3
**Requirements**: CLI-01, CLI-02, GRPC-09
**Success Criteria** (what must be TRUE):
  1. User can query keyboard info, read/set LED mode, adjust debounce, change polling rate, switch profiles, and remap keys entirely from the command line
  2. CLI loads device definitions from the JSON registry (same registry as the bridge) without hardcoded device constants
  3. Systemd service starts the bridge on boot and the MonsGeek web configurator connects without user intervention after login
  4. Systemd service restarts the bridge automatically if it crashes
**Plans**: TBD

Plans:
- [ ] 07-01: TBD
- [ ] 07-02: TBD

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
- [ ] 08-01: TBD
- [ ] 08-02: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 -> 4 -> 5 -> 6 -> 7 -> 8
Note: Phases 4, 5, and 6 all depend on Phase 3 and are independent of each other. They may execute in any order after Phase 3 completes.

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Project Scaffolding & Device Registry | 0/2 | Planning complete | - |
| 2. FEA Protocol & HID Transport | 0/3 | Not started | - |
| 3. gRPC-Web Bridge | 0/3 | Not started | - |
| 4. Bridge Integration & Key Remapping | 0/1 | Not started | - |
| 5. LED Control & Tuning | 0/2 | Not started | - |
| 6. Macros & Magnetic Switches | 0/2 | Not started | - |
| 7. CLI & Service Deployment | 0/2 | Not started | - |
| 8. Firmware Update | 0/2 | Not started | - |
