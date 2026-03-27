# Feature Research

**Domain:** Linux FEA keyboard framework and configurator bridge  
**Researched:** 2026-03-19  
**Corrected:** 2026-03-23  
**Confidence:** HIGH after transport validation on real M5W hardware

## Table Stakes

| Feature | Why It Matters | Complexity | Current Position |
|---------|----------------|------------|------------------|
| USB discovery and enumeration | Nothing works until the keyboard can be found reliably | Low | Wired M5W path proven |
| Raw FEA command transport | Foundation for every keyboard feature | Medium | Proven on wired M5W |
| 100ms-safe flow control | Required to avoid firmware stalls | Low | Implemented |
| Firmware-ID-aware model resolution | Avoids PID-only coupling and transport confusion | Medium | Implemented for wired probe path |
| gRPC-Web bridge on `localhost:3814` | This is the user-facing compatibility layer | Medium | Not started |
| Debounce / polling configuration | Directly targets the Linux double-letter / ghosting pain point | Low | Pending higher-level feature work |
| Key remapping | Core configurator functionality | Medium | Pending |
| RGB / LED control | Core configurator functionality | Medium | Pending |
| Profile management | Core configurator functionality | Low | Pending |
| Macro support | Expected configurator functionality | Medium | Pending |
| Non-root access via udev | Required for normal Linux usability | Low | Implemented |
| Hot-plug detection | Required for device list updates | Low | Implemented via `udev` |

## Differentiators

| Feature | Value | Complexity | Notes |
|---------|-------|------------|-------|
| Data-driven profile registry | Makes the project a framework instead of a one-off hack | Medium | Central design goal |
| CLI | Useful for Linux power users and automation | Medium | Planned after bridge |
| Systemd service | Makes the bridge feel native on Linux | Low | Planned after bridge |
| 2.4GHz dongle transport | Important for broader framework utility | High | Deferred until wired path is stable |
| Additional validated device profiles | Expands from “M5W only” to framework-level usefulness | Medium | Planned, but must follow real validation |
| Firmware update | High user value, high risk | High | Intentionally late |

## Anti-Features

| Feature | Why Not Now |
|---------|-------------|
| Custom GUI | The existing configurator already solves the UI problem |
| Broad unsupported device claims | Architecture should be general, but support must be validated device by device |
| Bluetooth LE transport | Not part of the current milestone |
| Kernel/input-layer work before configurator tuning | Debounce/polling via configurator should be tested first |
| Fancy LED streaming features | Not required for configurator compatibility MVP |

## Dependency Notes

- Everything depends on a correct Phase 2 transport layer
- The bridge depends on transport plus hot-plug
- Feature-family work depends on the bridge only if the web configurator is the primary UI path
- CLI and service work should reuse the same transport and profile abstractions

## MVP Definition

The MVP is:

- wired M5W discovery
- stable FEA transport
- gRPC-Web bridge
- MonsGeek configurator compatibility on Linux

The MVP is not:

- full support for every FEA-family device
- firmware flashing
- dongle support
- custom UI work

## Priority Order

1. Transport correctness
2. Bridge compatibility
3. Feature verification on real hardware
4. CLI / service packaging
5. Firmware / dongle / broader expansion

---
*Feature research corrected after hardware validation on 2026-03-23*
