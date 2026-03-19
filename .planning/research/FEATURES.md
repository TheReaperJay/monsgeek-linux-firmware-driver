# Feature Research

**Domain:** Linux HID keyboard driver and configurator bridge (MonsGeek yc3121 keyboards)
**Researched:** 2026-03-19
**Confidence:** HIGH

## Feature Landscape

### Table Stakes (Users Expect These)

Features users assume exist. Missing these = product feels incomplete. For a Linux keyboard configurator bridge, "users" are Linux enthusiasts who bought a MonsGeek keyboard and discovered it has no Linux configuration support. They need parity with the Windows/macOS experience or they return the keyboard.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| HID device detection and enumeration | Cannot do anything without finding the keyboard. Every comparable project (VIA, Vial, Wootility, OpenRGB) does this automatically. | LOW | VID 0x3141 match, hidapi device scan. Reference project has this fully working. |
| gRPC-Web bridge on localhost:3814 | This IS the product. The MonsGeek web configurator (app.monsgeek.com) and Electron app expect this exact endpoint. Without it, none of the downstream features work. | MEDIUM | Must implement `sendRawFeature`, `readRawFeature`, `watchDevList`, `getVersion`, `insertDb`, `getItemFromDb` RPCs. Protocol buffer definitions derivable from reference project. |
| FEA command protocol (send/receive with Bit7 checksums) | The transport layer for all keyboard communication. Every feature depends on this. | MEDIUM | 64-byte feature reports. Bit7 checksum algorithm documented in reference. Same protocol as AT32F405 (Akko) keyboards per PROJECT.md. |
| Key remapping (SET/GET_KEYMATRIX) | Primary reason people use keyboard configurators. VIA, Vial, QMK Configurator, Keychron Launcher all support this. Users who cannot remap keys have zero reason to use the tool. | MEDIUM | Must support per-profile remapping across the Common108_MG108B layout. Reference project has full implementation. |
| RGB/LED control (SET/GET_LEDPARAM) | Second most common reason for using a configurator. Every competitor supports LED mode selection, brightness, speed, color. MonsGeek keyboards ship with 26 LED modes. | MEDIUM | Mode selection, brightness, speed, color parameters. Reference project supports all 26 modes. |
| Profile management (4 profiles, SET/GET_PROFILE) | MonsGeek keyboards support 4 hardware profiles. Users expect to switch between them. QMK/VIA support layers, Wooting supports profiles, Keychron Launcher supports profiles. | LOW | Simple get/set commands. Low complexity because it is just profile index switching. |
| Polling rate configuration (SET/GET_REPORT) | Gamers buying the M5W expect to tune polling rate. Wooting, Razer, every gaming keyboard configurator exposes this. The M5W supports 125-8000Hz. | LOW | Single command, low complexity. |
| Debounce configuration (SET/GET_DEBOUNCE) | Directly addresses the known ghosting/double-letter Linux issue documented in PROJECT.md. This is the primary user pain point. MonsGeek's own troubleshooting page acknowledges double-click as a common issue fixable via debounce. | LOW | Single command. Critical for the "fix ghosting" use case that motivated this project. |
| udev rules for non-root HID access | Standard requirement for all Linux HID tools. VIA, Vial, Wooting, OpenRGB all ship udev rules. Without this, users must run as root, which is a security antipattern. | LOW | Single rules file matching VID 0x3141. TAG+="uaccess" is the modern approach (avoids security issues of MODE="0666"). |
| Device hotplug detection | Users plug/unplug keyboards. The bridge must detect this without restart. The web configurator's `watchDevList` RPC streams device connection events. VIA, Vial, Wootility all handle hotplug. | LOW | udev/inotify on hidraw devices, or periodic hidapi re-enumeration. Reference project has this. |
| Macro programming (SET/GET_MACRO) | Expected feature in any keyboard configurator. VIA, Vial, QMK, Keychron Launcher all support macros. MonsGeek's web configurator has a macro editor. | MEDIUM | Text macros are straightforward. Complex macros (delays, mouse clicks, key combos) are harder but the web configurator handles the complex editing -- the bridge just needs to relay the bytes. |

### Differentiators (Competitive Advantage)

Features that set the product apart. Not required, but valuable. In this domain, the primary differentiator is simply *existing* -- there is no other Linux configurator for MonsGeek yc3121 keyboards. Beyond that, these features add real value.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Firmware update capability (bootloader entry, chunk transfer, CRC-24) | Users stuck on old firmware with no Windows machine cannot update. This is a significant pain point. The M5W ships with v1.03 but newer firmware may fix bugs. Keychron Launcher supports firmware updates via WebHID; the MonsGeek web app expects this via the bridge. | HIGH | Destructive operation: bootloader erases app region before USB re-init. Requires explicit user confirmation, CRC-24 verification, and robust error handling. Bricking risk if interrupted. Must validate firmware binary before flashing. Reference project has full implementation. |
| CLI for direct keyboard control | Power users on Linux strongly prefer CLI tools over GUIs. The reference project provides 60+ CLI commands. No other MonsGeek tool offers scriptable keyboard configuration. Enables automation (shell scripts, cron jobs for profile switching, etc). | MEDIUM | Can be built incrementally as protocol commands are implemented. Each command wraps one or more FEA protocol operations. |
| Extensible device registry | Supporting additional yc3121-based MonsGeek keyboards by adding device definitions (not code changes) makes this a platform rather than a single-device hack. PROJECT.md explicitly requires this. | MEDIUM | Device definitions include: device ID, PID, key matrix layout, supported features. Data-driven approach. Reference project supports multiple Akko keyboards this way. |
| Systemd service for auto-start | Bridge runs as a daemon so the web configurator works immediately when the user opens the browser. No manual "start the server first" step. Similar to how Wootility runs in the background. | LOW | Standard systemd unit file with WantedBy=multi-user.target and udev trigger for on-demand start. Reference project ships this. |
| Device info and diagnostics | Exposing firmware version, device ID, SoC info, connection state helps users troubleshoot. The reference project's `info` and `all` commands are useful for debugging and support requests. | LOW | Wraps basic FEA query commands. Useful for both CLI and the gRPC bridge. |
| Profile import/export/backup | Users who invest time in configuring profiles want to back them up. Keychron Launcher and LOFREE apps support profile export/import. Not commonly seen in Linux CLI tools for keyboards, which makes it a differentiator. | MEDIUM | Read full profile state (keymap, LED settings, macros) and serialize to JSON/TOML. Restore by writing back. Requires implementing all GET commands for a full profile dump. |

### Anti-Features (Commonly Requested, Often Problematic)

Features that seem good but create problems. Deliberately NOT building these.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Custom GUI application | "I want a native Linux configurator GUI" | PROJECT.md explicitly excludes this. The MonsGeek web configurator already exists and works via the bridge. Building a GUI duplicates massive effort (key matrix visual editor, color picker, macro editor) for zero benefit. VIA/Vial already demonstrated that web-based configurators are the standard approach. | The gRPC-Web bridge enables the existing MonsGeek web configurator. Zero UI development needed. |
| Audio-reactive LED streaming | Reference project implements it. Flashy demo feature. | Requires ALSA/JACK/PipeWire dependencies, real-time audio processing, FFT analysis. Massive scope increase for a niche feature. The keyboard firmware already has built-in audio-reactive modes that respond to keypress patterns. | Defer entirely. Listed as out-of-scope in PROJECT.md. If demanded later, it is a standalone add-on, not core bridge functionality. |
| Screen color sync | Reference project implements it. "Ambient lighting" is trendy. | Requires PipeWire screen capture, color extraction, continuous streaming. Heavy dependencies (libpipewire, libclang). Niche use case that adds build complexity for all users. | Defer entirely. Listed as out-of-scope in PROJECT.md. |
| GIF animation upload/streaming | Reference project implements it. Cool demos. | Per-frame RGB streaming is bandwidth-intensive and requires custom firmware patches in the reference project. The stock yc3121 firmware may not support the same streaming protocol. Investigating compatibility is wasted effort until core features work. | Defer. Stock firmware LED modes already include 26 animation patterns. |
| Bluetooth LE transport | M5W is wireless -- "shouldn't it support Bluetooth config?" | M5W uses 2.4GHz, not BLE for its wireless connection. BLE has severe limitations for HID vendor commands (timing constraints, limited bandwidth). PROJECT.md explicitly defers this. | USB wired configuration only. The keyboard stores settings in firmware; configure over USB, use wirelessly. |
| 2.4GHz dongle transport | Reference project supports the F7/FC dongle protocol. | Different transport layer, different PID, additional protocol complexity. The M5W wired connection provides full configurability. | Defer. Wire the keyboard for configuration. Use 2.4GHz for daily use. Settings persist in firmware. |
| Windows/macOS support | "Make it cross-platform" | MonsGeek already ships official Windows/macOS configurators. This project exists because Linux has no solution. Cross-platform support adds build complexity (different HID backends, different service management) for platforms that are already served. | Linux only. This is the explicit constraint in PROJECT.md. |
| eBPF HID driver (for input fix) | Fix ghosting/double-letter at kernel level. | Requires nightly Rust, bpf-linker, kernel 6.12+, complex eBPF struct_ops. The reference project shows this is achievable but it is an enormous complexity spike. The hypothesis (from PROJECT.md) is that configurator-based debounce/polling tuning will fix the issue without a kernel driver. | Try debounce/polling configuration first. Only pursue eBPF if configurator-based fixes fail. This is the explicit strategy in PROJECT.md. |
| Per-key RGB color editor in CLI | "I want to set individual key colors from the command line" | The MonsGeek web configurator has a full visual per-key color editor. Replicating this in a CLI with 108 keys is poor UX. The bridge already enables the web editor. | Use the web configurator for per-key RGB. The bridge relays the commands. CLI should support bulk LED mode/color, not per-key editing. |

## Feature Dependencies

```
[FEA Command Protocol]
    |
    +--requires--> [HID Device Detection]
    |
    +--enables--> [Key Remapping]
    +--enables--> [RGB/LED Control]
    +--enables--> [Macro Programming]
    +--enables--> [Profile Management]
    +--enables--> [Polling Rate Config]
    +--enables--> [Debounce Config]
    +--enables--> [Device Info/Diagnostics]
    +--enables--> [Firmware Update] (also requires bootloader protocol)

[gRPC-Web Bridge]
    +--requires--> [FEA Command Protocol]
    +--requires--> [HID Device Detection]
    +--requires--> [Device Hotplug Detection]
    +--enables--> [Web Configurator Compatibility]

[CLI]
    +--requires--> [FEA Command Protocol]
    +--requires--> [HID Device Detection]
    +--enhances--> all individual features (wraps each as a subcommand)

[udev Rules]
    +--enables--> [HID Device Detection] (non-root)

[Systemd Service]
    +--requires--> [gRPC-Web Bridge]
    +--requires--> [udev Rules]

[Firmware Update]
    +--requires--> [FEA Command Protocol]
    +--requires--> [Bootloader Entry Protocol]
    +--requires--> [CRC-24 Verification]

[Extensible Device Registry]
    +--enhances--> [HID Device Detection] (multi-device support)
    +--enhances--> [Key Remapping] (device-specific key matrices)

[Profile Import/Export]
    +--requires--> [Key Remapping] (GET all keys)
    +--requires--> [RGB/LED Control] (GET LED settings)
    +--requires--> [Macro Programming] (GET macros)
    +--requires--> [Profile Management] (GET/SET profile)
```

### Dependency Notes

- **Everything requires FEA Command Protocol:** This is the foundation. No feature works without the ability to send/receive 64-byte feature reports with Bit7 checksums. It must be implemented first and tested thoroughly.
- **gRPC-Web Bridge requires Hotplug Detection:** The `watchDevList` RPC is a streaming call that notifies the web configurator when devices connect/disconnect. Without hotplug, the web app cannot know when a keyboard appears.
- **Firmware Update is isolated and high-risk:** It shares the FEA protocol transport but has its own bootloader entry sequence and chunk transfer protocol. It should be implemented last among table-stakes features because (a) it is destructive and (b) all other features must be working to validate the keyboard is functional before and after flashing.
- **CLI and gRPC Bridge are parallel interfaces to the same protocol:** They share the underlying command implementations but differ in how they expose them (subcommands vs RPCs). Designing the protocol layer cleanly enables both.
- **udev Rules are a prerequisite for usability:** Without them, nothing works for non-root users. Ship them from day one.

## MVP Definition

### Launch With (v1)

Minimum viable product -- the bridge works with the MonsGeek web configurator on Linux.

- [ ] HID device detection (VID 0x3141, PID 0x4005) -- cannot do anything without finding the keyboard
- [ ] FEA command protocol (send/receive with Bit7 checksums) -- the transport for everything
- [ ] gRPC-Web bridge on localhost:3814 -- enables the web configurator
- [ ] `sendRawFeature` / `readRawFeature` / `watchDevList` / `getVersion` RPCs -- minimum RPCs for web app compatibility
- [ ] `insertDb` / `getItemFromDb` RPCs -- the web app stores local state through these
- [ ] Device hotplug detection -- web app needs device connect/disconnect events
- [ ] udev rules -- non-root access
- [ ] Debounce configuration (via bridge or CLI) -- fixes the primary user pain point (ghosting/double-letters)
- [ ] Polling rate configuration (via bridge or CLI) -- related to debounce, addresses input quality

### Add After Validation (v1.x)

Features to add once the bridge is proven working with the web configurator.

- [ ] CLI with core commands (info, led, debounce, rate, profile, remap) -- power user interface
- [ ] Key remapping verification via CLI -- confirm remaps work independently of web app
- [ ] RGB/LED control via CLI -- quick adjustments without opening browser
- [ ] Profile management via CLI -- switch profiles from terminal
- [ ] Macro read/verify via CLI -- inspect macro state
- [ ] Extensible device registry -- data-driven device definitions for other yc3121 boards
- [ ] Systemd service -- auto-start the bridge on boot/device plug
- [ ] Device info/diagnostics command -- troubleshooting and support

### Future Consideration (v2+)

Features to defer until core is battle-tested.

- [ ] Firmware update capability -- HIGH complexity, HIGH risk, requires extensive validation before shipping
- [ ] Profile import/export/backup -- useful but requires all GET commands to be verified first
- [ ] eBPF HID driver -- only if debounce/polling config does not fix ghosting
- [ ] 2.4GHz dongle transport -- after wired USB is solid
- [ ] Support for additional MonsGeek keyboards beyond M5W -- once device registry is proven

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| HID device detection | HIGH | LOW | P1 |
| FEA command protocol | HIGH | MEDIUM | P1 |
| gRPC-Web bridge (core RPCs) | HIGH | MEDIUM | P1 |
| udev rules | HIGH | LOW | P1 |
| Device hotplug detection | HIGH | LOW | P1 |
| Debounce configuration | HIGH | LOW | P1 |
| Polling rate configuration | HIGH | LOW | P1 |
| Key remapping (via bridge) | HIGH | MEDIUM | P1 |
| RGB/LED control (via bridge) | HIGH | MEDIUM | P1 |
| Profile management (via bridge) | MEDIUM | LOW | P1 |
| Macro programming (via bridge) | MEDIUM | MEDIUM | P1 |
| CLI (core commands) | MEDIUM | MEDIUM | P2 |
| Extensible device registry | MEDIUM | MEDIUM | P2 |
| Systemd service | MEDIUM | LOW | P2 |
| Device info/diagnostics | MEDIUM | LOW | P2 |
| Firmware update | HIGH | HIGH | P3 |
| Profile import/export | LOW | MEDIUM | P3 |
| eBPF HID driver | MEDIUM | HIGH | P3 |
| 2.4GHz dongle transport | LOW | HIGH | P3 |

**Priority key:**
- P1: Must have for launch -- the bridge works with the web configurator
- P2: Should have, add when possible -- power user features and operational maturity
- P3: Nice to have, future consideration -- high risk/cost features deferred until core is proven

## Competitor Feature Analysis

| Feature | VIA/Vial (QMK) | Wootility (Wooting) | Keychron Launcher | MonsGeek Web App | Our Approach |
|---------|-----------------|---------------------|-------------------|------------------|--------------|
| Linux support | VIA: Chromium only. Vial: native app. | Native app for Win/Mac/Linux | Web-based (Chromium) | Windows/macOS only (Electron + iot_driver.exe) | gRPC-Web bridge enables existing web app on Linux |
| Key remapping | Full layer support | Full remap + analog | Full remap | Full remap per profile | Bridge relays web app commands; CLI for quick remaps |
| RGB control | Basic (QMK modes) | Full (per-key, effects) | Full (per-key, effects) | Full (26 modes, per-key) | Bridge relays; CLI for mode/brightness/color |
| Macros | Basic text macros | Full macro editor | Full macro editor | Full macro editor | Bridge relays; text macros via CLI |
| Firmware update | QMK Toolbox (separate tool) | In-app | In-app (WebHID) | In-app | Deferred to v2; bridge can relay when ready |
| Profile management | Layers (firmware) | Multiple profiles | Multiple profiles | 4 hardware profiles | Bridge relays; CLI for switching |
| Polling rate | N/A (firmware set) | 125-8000Hz | 125-8000Hz | 125-8000Hz | CLI + bridge, exposed immediately |
| Debounce | QMK config (compile-time) | In-app | N/A | In-app | CLI + bridge, critical for ghosting fix |
| Analog/HE features | N/A | Rapid Trigger, DKS, Mod-Tap | Rapid Trigger, DKS | Rapid Trigger, DKS, Mod-Tap, Snap-Tap | Not applicable to M5W (standard mechanical switches) |
| Hotplug | VIA detects reconnection | Auto-detect | Auto-detect | Auto-detect | udev-based hotplug with watchDevList streaming |
| Installation | Browser extension or desktop app | Installer | No install (web) | Installer (Windows) | `make install` + udev rules + optional systemd service |

## Sources

- [MonsGeek Official FAQ: Double-Click Fix](https://www.monsgeek.com/blog/keyboard-double-click-fix/) - Confirms debounce as solution for double-key issues
- [VIA Configurator](https://caniusevia.com/) - Web-based keyboard configurator using WebHID
- [Vial](https://get.vial.today/) - Open-source cross-platform QMK configurator
- [Wooting Rapid Trigger](https://wooting.io/post/what-is-wootings-rapid-trigger-for-analog-keyboards) - Analog keyboard configurator features
- [OpenRGB](https://openrgb.org/) - Open-source RGB lighting control for Linux
- [Keychron Launcher](https://www.keychron.com/blogs/archived/advantages-of-the-keychron-launcher-web-app) - Web-based keyboard configurator
- [hidapi Rust crate](https://docs.rs/hidapi) - Rust HID communication library
- [Vial Linux udev setup](https://get.vial.today/manual/linux-udev.html) - udev rules best practices for keyboard HID access
- [Wooting Linux udev rules](https://help.wooting.io/article/147-configuring-device-access-for-wootility-under-linux-udev-rules) - Competitor udev approach
- [Kanata Linux key debounce PR](https://github.com/jtroo/kanata/pull/1605) - Linux-specific key debounce handling
- [MonsGeek Download Center](https://www.monsgeek.com/download/) - Official MonsGeek software (Windows/macOS only)
- Reference project: `references/monsgeek-akko-linux/` - Complete Rust implementation for Akko keyboards demonstrating the full feature set and architecture

---
*Feature research for: Linux HID keyboard driver and configurator bridge (MonsGeek yc3121)*
*Researched: 2026-03-19*
