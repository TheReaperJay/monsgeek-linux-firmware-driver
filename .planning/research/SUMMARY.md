# Project Research Summary

**Project:** MonsGeek Firmware Driver (Linux HID Bridge)
**Domain:** Linux HID keyboard driver and gRPC-Web configurator bridge
**Researched:** 2026-03-19
**Confidence:** HIGH

## Executive Summary

This project is a Linux userspace HID driver that bridges the MonsGeek web configurator (app.monsgeek.com) to MonsGeek yc3121-based keyboards over USB. The standard approach for this class of tool -- proven by VIA, Vial, Wootility, and a directly applicable reference project (monsgeek-akko-linux) -- is a Rust binary that speaks gRPC-Web on localhost:3814, translating browser requests into 64-byte HID Feature Reports sent via the Linux hidraw interface. The reference project targets the same FEA command protocol on a sibling SoC (AT32F405), making it an unusually strong foundation: the architecture, protocol framing, and most command definitions transfer directly. The primary value proposition is simple -- MonsGeek ships no Linux configurator, so this project fills a gap that affects every Linux user with these keyboards.

The recommended approach is a three-crate Rust workspace (protocol/transport/binary) using tonic 0.14 for gRPC-Web, hidapi for HID I/O, and tokio-udev for hotplug detection. The bridge operates as a thin protocol translator: the web configurator already understands the full keyboard protocol and just needs a local process to relay commands to hardware. This means the MVP requires no GUI, no complex state management, and no command interpretation beyond device identification and routing. The critical design constraint is that the gRPC proto schema must match the Windows `iot_driver.exe` contract exactly, including known typos (`VenderMsg`, `DangleDevType`), because the web app is compiled against those wire names.

The key risks are hardware-level: the yc3121 firmware crashes if commands are sent faster than 100ms apart, performs zero bounds checking on write indices (enabling RAM/flash corruption from malformed commands), and the Linux hidraw driver returns stale Feature Report data requiring retry-and-match logic. These are not edge cases -- they affect every HID interaction and must be solved in the lowest transport layer before any features are built on top. Additionally, while the FEA command protocol envelope is shared between yc3121 and AT32F405, individual command support must be verified against the actual M5W hardware since HE-specific features (magnetism, analog) do not exist on this standard mechanical keyboard.

## Key Findings

### Recommended Stack

Rust 1.94 (edition 2024) with tonic 0.14 for the gRPC-Web server, hidapi 2.6 for HID device communication, and tokio-udev 0.10 for hotplug monitoring. The workspace splits into three crates: `monsgeek-transport` (raw HID I/O, protocol framing, device discovery), `monsgeek-keyboard` (typed keyboard feature API), and the main binary (gRPC server, CLI, firmware flash). All version choices align with current stable releases and are verified compatible. See STACK.md for full dependency matrix.

**Core technologies:**
- **Rust (tonic 0.14 + tonic-web 0.14):** gRPC-Web server with built-in protocol translation, eliminating the need for an Envoy proxy
- **hidapi 2.6:** Battle-tested HID communication via Linux hidraw; Feature Report send/receive for the vendor config interface (IF2)
- **tokio-udev 0.10:** Async udev event monitoring for keyboard hotplug detection; feeds the `watchDevList` streaming RPC
- **zerocopy 0.8:** Zero-copy parsing of 64-byte HID reports into structured command/response types
- **tower-http (CORS):** Required for browser-to-localhost gRPC-Web; the web app at https://app.monsgeek.com makes cross-origin requests to http://127.0.0.1:3814

**Critical "do not use" decisions:**
- Skip sled (abandoned); use in-memory HashMap for DB RPCs
- Skip nusb (wrong abstraction; keyboard is standard HID, not raw USB)
- Defer eBPF, audio-reactive LEDs, screen capture, TUI, cross-platform support

### Expected Features

**Must have (table stakes):**
- HID device detection and enumeration (VID 0x3141, PID 0x4005, IF2, usage page 0xFFFF)
- FEA command protocol with Bit7/Bit8 checksums (the transport for everything)
- gRPC-Web bridge on localhost:3814 implementing `sendRawFeature`, `readRawFeature`, `watchDevList`, `getVersion`, `insertDb`, `getItemFromDb`
- Device hotplug detection via udev
- udev rules for non-root hidraw access
- Debounce and polling rate configuration (directly addresses the ghosting/double-letter issue that motivated this project)
- Key remapping, RGB/LED control, profile management, macro programming (all via bridge relay to web configurator)

**Should have (differentiators):**
- CLI with core commands (info, led, debounce, rate, profile, remap) for power users
- Extensible device registry (JSON-driven, new keyboards without code changes)
- Systemd service for auto-start
- Device info/diagnostics

**Defer (v2+):**
- Firmware update (HIGH complexity, destructive, bricking risk)
- Profile import/export/backup
- eBPF HID driver (only if debounce config doesn't fix ghosting)
- 2.4GHz dongle transport

### Architecture Approach

The system is a layered transport stack: Raw HID I/O (hidapi) -> Flow Control (100ms inter-command delay, retry-and-match for stale reads, mutex serialization) -> Keyboard Interface (typed command/response API) -> gRPC Service (thin translation to/from protobuf). The bridge is stateless by design -- the keyboard firmware is the source of truth, and the web app queries it directly. The only bridge state is the set of currently connected device transport handles. See ARCHITECTURE.md for component diagram and data flow.

**Major components:**
1. **Raw Transport (HidWiredTransport)** -- Feature Report I/O via hidapi on IF2; background reader thread on IF1 for vendor events
2. **Flow Control Transport** -- Wraps raw transport with 100ms inter-command delay, echo-matching retry loop, and mutex serialization
3. **Protocol Layer** -- FEA_CMD constants, packet framing, Bit7/Bit8 checksum calculation, typed HidCommand/HidResponse traits with bounds validation
4. **Device Manager** -- hidapi enumeration + tokio-udev hotplug; maintains HashMap of connected transports
5. **gRPC Service (DriverService)** -- tonic-web server implementing the Windows `iot_driver.exe` proto contract; pass-through for raw commands, device management for routing
6. **Device Registry** -- JSON device definitions loaded at startup; data-driven support for multiple yc3121 keyboards
7. **DB Store** -- In-memory HashMap for web app UI state persistence (replaces sled)

### Critical Pitfalls

1. **100ms inter-command drain time (HARDWARE)** -- The yc3121 firmware crashes and stalls if commands arrive faster than 100ms apart. Enforce mandatory delay in the transport layer with mutex + timestamp. This is non-negotiable and must exist from the first line of HID code.

2. **Linux hidraw returns stale Feature Report data** -- GET_FEATURE after SET_FEATURE returns the previous response, not the current one. Implement retry-and-match: read up to 3 times with delays, accept only when response command byte matches the request. Without this, every setting read is silently wrong.

3. **Firmware has zero bounds checking on write indices** -- Out-of-range `chunk_index`, `macro_id`, or `slot_id` values overflow the firmware's staging buffer into adjacent RAM/flash, corrupting calibration data or bricking the device. Enforce hard limits in the protocol layer's send path: reject before the bytes hit HID.

4. **gRPC proto must match official app exactly (including typos)** -- The web app is compiled against specific wire names (`VenderMsg`, `DangleDevType`). Copy the proto from the reference project verbatim. Any "cleanup" silently breaks compatibility with no error messages.

5. **VID/PID differences between reference project and target** -- Reference uses VID 0x3151 (Akko); target is VID 0x3141 (MonsGeek). All device constants must come from a registry, never hardcoded. Cargo-culting reference constants means the driver never finds the keyboard.

6. **Protocol compatibility assumption (yc3121 vs AT32F405)** -- The FEA command envelope is shared but individual command support differs. HE-specific commands (magnetism, analog) do not exist on the M5W. Every command must be individually verified against real hardware before being considered implemented.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: HID Transport Foundation
**Rationale:** Everything depends on reliable HID communication. The 100ms timing constraint and stale-read problem must be solved at the lowest level before any features work. This is the crate that every other component imports.
**Delivers:** `monsgeek-transport` crate with Transport trait, HidWiredTransport, FlowControlTransport, protocol framing with checksums, and bounds validation on all write commands.
**Addresses:** FEA command protocol, HID device detection (from FEATURES.md table stakes)
**Avoids:** Pitfall 0 (100ms drain time), Pitfall 1 (stale reads), Pitfall 2 (firmware OOB writes), Pitfall 4 (wrong VID/PID)
**Verification:** Send GET_USB_VERSION to M5W, receive valid device ID 1308. SET then GET a value and confirm the new value is returned.

### Phase 2: Device Discovery and Hotplug
**Rationale:** The gRPC bridge cannot function without knowing which devices are connected and detecting changes. This is a prerequisite for the bridge and decouples device management from the gRPC layer.
**Delivers:** Device enumeration by VID/PID/usage, udev-based hotplug monitoring, device open/close lifecycle, udev rules file.
**Addresses:** Device hotplug detection, udev rules (from FEATURES.md table stakes)
**Avoids:** Pitfall 4 (VID/PID matching must use registry, not hardcoded constants)

### Phase 3: gRPC-Web Bridge (Core RPCs)
**Rationale:** The bridge is the primary deliverable. With transport and discovery working, the bridge is a thin translation layer. The proto must be copied verbatim from the reference project. CORS must be configured for the HTTPS-to-HTTP-localhost scenario.
**Delivers:** tonic-web gRPC server on 127.0.0.1:3814 implementing `sendRawFeature`, `readRawFeature`, `watchDevList`, `watchVender`, `getVersion`, `insertDb`, `getItemFromDb`. Full compatibility with the MonsGeek web configurator.
**Addresses:** gRPC-Web bridge, key remapping, RGB/LED control, profile management, macro programming, debounce/polling config (all via bridge pass-through to web app)
**Avoids:** Pitfall 5 (proto schema mismatch), Pitfall 6 (CORS misconfiguration)
**Verification:** Open https://app.monsgeek.com in a browser, confirm keyboard appears in device picker, change an LED mode, verify it takes effect on the hardware.

### Phase 4: Protocol Verification and Hardening
**Rationale:** The bridge passes raw bytes, but each command's behavior on the yc3121 must be verified individually. The reference project targets AT32F405; yc3121 may differ in specific commands. This phase systematically tests every command against real hardware.
**Delivers:** Verified command compatibility matrix for yc3121. Documentation of which commands work, which differ, which are unsupported. Feature detection via GET_FEATURE_LIST (0xE6).
**Addresses:** Pitfall 8 (protocol assumption without verification)
**Avoids:** Silent failures from unsupported commands

### Phase 5: CLI and Power User Features
**Rationale:** With the bridge proven working, add direct keyboard control via CLI. CLI commands wrap the same typed Keyboard Interface that the bridge uses, providing an alternative interface for power users.
**Delivers:** `monsgeek-driver` CLI with subcommands: info, led, debounce, rate, profile, remap. Extensible device registry (JSON-driven). Systemd service unit file.
**Addresses:** CLI core commands, extensible device registry, systemd service, device info/diagnostics (from FEATURES.md differentiators)

### Phase 6: Firmware Update (High-Risk)
**Rationale:** Firmware flashing is destructive and irreversible (bootloader erases app region before USB init). It must be implemented last, after the entire protocol is verified working, with extensive safety gates. Dry-run simulation must pass before real hardware testing.
**Delivers:** Firmware validation, bootloader entry with explicit confirmation, chunk transfer with CRC-24 verification, progress reporting, config backup/restore around flash.
**Addresses:** Firmware update capability (from FEATURES.md differentiators)
**Avoids:** Pitfall 3 (bootloader point-of-no-return)

### Phase Ordering Rationale

- **Bottom-up by dependency:** Transport -> Discovery -> Bridge -> Verification -> CLI -> Firmware. Each phase builds on the previous. No phase can be started without its predecessor being complete.
- **Risk-ordered:** The highest-risk hardware pitfalls (timing, stale reads, OOB) are addressed in Phase 1. The highest-risk feature (firmware flash) is last.
- **Value delivery:** Phase 3 (bridge) is the primary user-facing milestone. The first three phases form a straight line to MVP: "the web configurator works on Linux."
- **Verification before expansion:** Phase 4 exists because the reference project targets a different SoC. Without systematic verification, features may appear to work but produce incorrect results on the M5W.
- **CLI after bridge:** The bridge is the primary interface (the web configurator does the heavy UX lifting). CLI is a power-user addition, not a prerequisite.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 1 (Transport):** Needs `/gsd:research-phase` -- the 100ms timing, stale-read retry logic, and bounds validation are hardware-specific and must be precisely implemented. The reference project's transport code should be studied in detail.
- **Phase 3 (gRPC Bridge):** Needs `/gsd:research-phase` -- the exact proto contract, CORS header requirements, and gRPC-Web integration pattern with tonic-web need precise specification. Browser testing is the only reliable validation.
- **Phase 4 (Protocol Verification):** Needs `/gsd:research-phase` -- systematic hardware testing required. The JS bundle from the MonsGeek Electron app (41MB `dist/index.eb7071d5.js`) should be analyzed to extract the yc3121 feature flags and command support.
- **Phase 6 (Firmware Update):** Needs `/gsd:research-phase` -- bootloader protocol, CRC-24 implementation, recovery procedures, and the chunk transfer format all require careful study from the reference project's firmware documentation.

Phases with standard patterns (skip research-phase):
- **Phase 2 (Discovery):** Well-documented pattern. hidapi enumeration + tokio-udev is straightforward and the reference project provides a working implementation.
- **Phase 5 (CLI):** Standard clap-based CLI with subcommands wrapping the Keyboard Interface. No novel patterns.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All crate versions verified on docs.rs/crates.io. Reference project proves the dependency selection works. Tonic 0.14 is current stable and all ecosystem crates align. |
| Features | HIGH | Feature set derived from the working reference project, competitor analysis, and the web configurator's known RPC contract. MVP is clearly scoped. |
| Architecture | HIGH | Architecture directly derived from a working Rust implementation targeting the same protocol. The three-crate workspace, layered transport, and stateless bridge patterns are proven. |
| Pitfalls | HIGH | Pitfalls sourced from the reference project's protocol documentation, firmware analysis, and bug reports. Hardware-level issues (timing, stale reads, OOB) are confirmed, not theoretical. |

**Overall confidence:** HIGH

The reference project (monsgeek-akko-linux) is an extraordinarily strong primary source. It is a complete, working Rust implementation of the same protocol family, with detailed documentation of hardware quirks, firmware bugs, and protocol details. The main uncertainty is not in the approach but in the yc3121-specific command compatibility -- which commands from the AT32F405 reference work identically on the yc3121 M5W.

### Gaps to Address

- **yc3121 command compatibility matrix:** The exact set of FEA commands supported by the M5W firmware is unknown until tested. The reference project targets AT32F405; the yc3121 may omit or modify commands. Phase 4 exists specifically to close this gap through systematic hardware testing.
- **MonsGeek web app JS bundle analysis:** The 41MB JS bundle from the Electron app contains device definitions for device ID 1308 (M5W). Extracting feature flags, supported commands, key matrix layout, and LED matrix from this bundle would significantly reduce guesswork. This should happen early, ideally before Phase 3.
- **Exact CORS header requirements:** The precise set of headers the MonsGeek web app sends in its gRPC-Web requests must be discovered by inspecting real browser traffic or analyzing the app's gRPC client configuration. The reference project's CORS setup is a starting point but may need adjustment.
- **Bootloader PID for yc3121 M5W:** The reference project documents bootloader PIDs 0x502A for AT32F405. The yc3121 bootloader PID (documented as 0x504A or 0x404A in PITFALLS.md) needs hardware verification before firmware flash can be implemented.
- **GET_MACRO stride bug applicability to yc3121:** The macro read stride bug is documented for AT32F405 firmware. Whether the yc3121 firmware has the same bug is unknown. Plan for it but verify.

## Sources

### Primary (HIGH confidence)
- Reference project source code: `references/monsgeek-akko-linux/` -- complete Rust implementation, same FEA protocol family
- Reference project documentation: `PROTOCOL.md`, `HARDWARE.md`, `FIRMWARE_PATCH.md`, `CLAUDE.md`
- Reference project proto definition: `iot_driver_linux/proto/driver.proto`
- Reference project firmware bug reports: `docs/bugs/oob_hazards.txt`, `docs/bugs/get_macro_stride_bug.txt`
- docs.rs crate documentation: tonic 0.14.5, hidapi 2.6.5, tokio-udev 0.10.0, tower-http 0.6.8 (all verified 2026-03-19)
- releases.rs: Rust 1.94.0 stable, edition 2024 (verified 2026-03-19)

### Secondary (MEDIUM confidence)
- MonsGeek Official FAQ: keyboard double-click fix (confirms debounce as solution)
- VIA, Vial, Wootility, Keychron Launcher documentation (competitor feature analysis)
- Linux HIDRAW kernel documentation (hidraw report ID handling)
- OpenRazer reverse engineering guide (USB protocol analysis patterns)
- GitHub issues: hidapi #174 (Linux feature report buffering), tonic #270 (gRPC-Web CORS)

### Tertiary (LOW confidence)
- yc3121 command compatibility: inferred from "same FEA protocol structure" statement in PROJECT.md, needs hardware verification
- Bootloader PIDs for yc3121: documented in reference project for AT32F405 SoC family, may differ for yc3121 variant
- GET_MACRO stride bug on yc3121: documented for AT32F405 firmware, unknown if applicable to yc3121

---
*Research completed: 2026-03-19*
*Ready for roadmap: yes*
