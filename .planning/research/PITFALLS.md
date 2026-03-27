# Pitfalls Research

**Domain:** Linux FEA keyboard framework and configurator bridge  
**Researched:** 2026-03-19  
**Corrected:** 2026-03-23  
**Confidence:** HIGH for the wired M5W transport path

## Critical Pitfalls

### Pitfall 1: Commands faster than 100ms can wedge the firmware

The yc3121 firmware cannot safely absorb back-to-back commands. The transport layer must serialize requests and enforce the timing rule globally.

### Pitfall 2: PID is not canonical device identity

USB PID changes across transports. If the framework keys devices by PID, it will couple transport identity to model identity and break as soon as a device has multiple transport forms.

### Pitfall 3: `GET_USB_VERSION` field width mistakes

The firmware device ID in `GET_USB_VERSION` is a 32-bit little-endian field. Treating it as 16-bit is a silent correctness bug that happens to “work” only when higher bytes are zero.

### Pitfall 4: Wrong USB IDs from secondary sources

Earlier planning used an incorrect M5W USB identity extracted from the Windows bundle. The verified wired M5W identity is `0x3151:0x4015`. Hardware and primary references must outrank bundle guesses.

### Pitfall 5: Broken kernel probing on IF1 / IF2

The M5W firmware has broken report-descriptor behavior on IF1/IF2. The Linux-side workaround on this host setup is `HID_QUIRK_IGNORE`, not optimistic assumptions about selective unbind being universally sufficient.

### Pitfall 6: Forgetting to hand `IF0` back to the kernel

Short-lived sessions that detach and claim `IF0` must restore it. Otherwise the keyboard stops typing after tests or one-off commands.

### Pitfall 7: Assuming `libusb` hot-plug is enough

In this environment, `libusb` arrival callbacks were not reliable enough. `udev` is the planning truth for add/remove events.

### Pitfall 8: Copying the reference project without checking the target

The reference projects are strong and should be relied on, but not copied blindly. Transport backend, field widths, PIDs, feature availability, and ownership choices must be validated against the actual target.

### Pitfall 9: Treating all FEA-family features as universally supported

Advanced features such as magnetic-switch behavior must be treated as profile-specific capabilities, not as guarantees across all devices.

### Pitfall 10: Proto “cleanup” in the bridge layer

The configurator bridge must match the upstream proto contract exactly. Renaming fields, fixing typos, or “simplifying” service definitions risks silent incompatibility.

## How To Avoid Repeating These Errors

- verify target-device facts on real hardware as early as possible
- prefer primary references over secondary extraction artifacts
- keep device identity in the registry/profile layer and transport identity in the runtime transport layer
- make transport ownership explicit
- annotate or retire stale planning docs instead of letting them silently persist as truth

---
*Pitfalls research corrected after hardware validation on 2026-03-23*
