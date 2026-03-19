# Pitfalls Research

**Domain:** Linux HID keyboard driver and configurator bridge (reverse-engineered proprietary protocol)
**Researched:** 2026-03-19
**Confidence:** HIGH (primary source: reference project with identical protocol, fully reverse-engineered firmware)

## Critical Pitfalls

### Pitfall 0: yc3121 Firmware Requires 100ms Inter-Command Drain Time (HARDWARE CONFIRMED)

**What goes wrong:**
The yc3121 firmware has no command queuing. If a second HID command is sent before the firmware has fully drained its internal buffer from the previous command, the firmware **crashes and stalls**. The keyboard becomes unresponsive and requires a physical reconnect. This is not a Linux-specific issue — it's a firmware limitation.

**Why it happens:**
The yc3121 SoC processes HID feature reports synchronously in a single-threaded firmware loop. There is no interrupt-driven command queue. The firmware reads from the USB endpoint, processes the command, writes the response, and only then is ready for the next command. Sending data during this cycle corrupts the firmware's internal state.

**How to avoid:**
Enforce a **mandatory minimum 100ms delay** between every HID send/receive cycle in the transport layer. This is non-negotiable:
- After sending a command AND reading the response, wait 100ms before the next send
- This applies to ALL commands — GETs, SETs, retries, everything
- The transport layer must enforce this globally, not per-caller
- Use a mutex + timestamp to prevent any concurrent access from violating the timing

```
fn send_and_receive(cmd) -> response:
    lock(transport_mutex)
    elapsed = now() - last_command_time
    if elapsed < 100ms:
        sleep(100ms - elapsed)
    send_feature(cmd)
    response = read_feature()
    last_command_time = now()
    unlock(transport_mutex)
    return response
```

**Warning signs:**
- Keyboard stops responding after rapid command sequences
- Second command in a batch always fails
- Device needs USB reconnect to recover
- Works on first try but fails when scripted/automated

**Phase to address:**
Phase 1 (HID transport layer). This constraint must be baked into the lowest-level transport abstraction. Every higher-level operation inherits the protection automatically.

---

### Pitfall 1: Linux hidraw Feature Report Buffering Returns Stale Data

**What goes wrong:**
After sending a SET_FEATURE (HIDIOCSFEATURE) command on Linux hidraw, the immediately following GET_FEATURE (HIDIOCGFEATURE) returns the **previous** response, not the response to the command just sent. The driver appears to work (no errors), but every response is one command behind. Key remapping, LED settings, and profile changes silently apply wrong data.

**Why it happens:**
The Linux hidraw kernel driver buffers feature reports differently than Windows. The SET_FEATURE ioctl returns before the keyboard firmware has processed the command and written its response. The GET_FEATURE ioctl reads whatever the kernel last cached, which is the previous response. This is documented in the reference project's PROTOCOL.md (Section 3.4) and confirmed through extensive debugging.

**How to avoid:**
Implement a retry-and-match loop: after sending a command, read the feature report 2-3 times with ~50-100ms delays between reads. Match the command echo byte at position 0 of the response against the command byte that was sent. Only accept the response when bytes match. For commands without echo (GET_MULTI_MAGNETISM 0xE5, GET_DONGLE_STATUS 0xF7), track the last-sent command and accept on first non-stale read.

```
fn query(fd, cmd) -> response:
    for attempt in 0..3:
        send_feature(fd, cmd)
        sleep(100ms)
        response = read_feature(fd)
        if response[0] == cmd:  // echo match
            return response
    return error("no matching response")
```

**Warning signs:**
- Commands seem to work but return data that doesn't match what was just set
- GET_LEDPARAM returns the LED mode before the change, not the current one
- Responses look valid but are "shifted by one" compared to the command sequence

**Phase to address:**
Phase 1 (HID transport layer). This must be solved in the lowest-level HID communication abstraction before any protocol commands are built on top.

---

### Pitfall 2: Firmware Has No Bounds Checking -- Host Driver Must Enforce All Limits

**What goes wrong:**
The yc3121/RY firmware performs **zero bounds checking** on chunked SET commands (SET_KEYMATRIX 0x0A, SET_MACRO 0x0B, SET_FN 0x10, SET_USERPIC 0x0C). Sending a `chunk_index >= 10` overflows the firmware's 588-byte staging buffer into adjacent RAM. Sending a `macro_id >= 16` overflows the stack frame and enables arbitrary code execution on the keyboard MCU. Sending a `slot_id >= 6` for USERPIC overwrites the return address on the stack. A buggy driver can corrupt the keyboard's RAM, flash calibration data, or brick the device.

**Why it happens:**
The firmware was developed for a controlled environment where only the official Windows/macOS configurator sends commands. The official app always sends valid parameters. The firmware trusts the host completely. When building a third-party driver, the temptation is to pass through the web app's commands without validation, since "the web app wouldn't send bad data." But bugs in the bridge, malformed gRPC payloads, or protocol implementation errors can produce out-of-range values.

**How to avoid:**
Enforce hard limits at the transport/protocol layer -- not in business logic, not optional, not configurable:

```
SET_KEYMATRIX / SET_FN:  layer_id (byte 1)    must be 0-5
SET_MACRO:               macro_id (byte 1)    must be 0-15 (flash path limit, 0-31 for RAM)
SET_USERPIC:             slot_id (byte 1)     must be 0-4
All chunked commands:    chunk_index (byte 2)  must be <= 9
All chunked commands:    total accumulated bytes must be <= 514
```

Implement these as assertions in the send path that reject the command before it reaches the HID layer. Log rejected commands with full hex dump for debugging.

**Warning signs:**
- RGB animation glitches after setting macros (g_rgb_anim_state corruption)
- Keyboard stops responding after a SET command (RAM corruption)
- Calibration data lost after configuration changes (flash region overwrite)
- Keyboard enters bootloader unexpectedly (stack overflow → corrupted return address)

**Phase to address:**
Phase 1 (protocol layer). These validation checks must be baked into the protocol command builder from day one. They are not optional "hardening" to add later -- a single invalid command can brick the keyboard.

---

### Pitfall 3: Bootloader Erases Application Region Before USB Init -- Firmware Flash Is Point-of-No-Return

**What goes wrong:**
When the ENTER_BOOTLOADER command (0x7F + 0x55AA55AA magic) is sent, the firmware immediately: (1) erases the config header at 0x08028000 (all LED settings, profiles, keymaps, macros gone), (2) writes 0x55AA55AA to the mailbox at 0x08004800, (3) reboots. The bootloader then erases 70 pages x 2KB = 140KB of the application region (0x08005000-0x08027FFF) **before initializing USB**. If the firmware transfer fails, is interrupted, or uses the wrong checksum, the device is stuck in bootloader mode with no application firmware. It is not bricked (ROM DFU recovery exists via BOOT0 pin), but it requires physical hardware access to recover.

**Why it happens:**
Developers test firmware flashing with correct firmware files and stable connections. They don't test: power loss mid-transfer, USB cable disconnection, wrong firmware file for the device, checksum calculation errors. The checksum has a specific bug: the bootloader checksums ALL bytes of ALL 64-byte chunks **including 0xFF padding in the last chunk**. If the host omits padding from its checksum calculation, the checksums mismatch and the device stays in bootloader mode.

**How to avoid:**
1. **Pre-validation**: Before sending ENTER_BOOTLOADER, verify the firmware file matches the target device (check chip ID header bytes, file size within expected range for the SoC). For yc3121: firmware should be in the range of ~100-200KB.
2. **Checksum calculation**: Include 0xFF padding bytes for the final partial chunk in the checksum. The bootloader compares only the lower 24 bits (`checksum & 0xFFFFFF`).
3. **Explicit user confirmation**: Require user to type the device name or a confirmation string. This is not a "click OK" dialog -- it must be an intentional action.
4. **Transfer verification**: After FW_TRANSFER_COMPLETE (0xBA 0xC2), wait for the device to reboot. If it re-enumerates with the bootloader PID instead of the application PID, the transfer failed.
5. **Config backup**: Before entering bootloader, read and save all configuration (profiles, keymaps, LED settings, macros, Fn layers). Firmware update erases config. Restore after successful flash.

**Warning signs:**
- Device re-enumerates with bootloader PID (0x504A for yc3121) instead of application PID after flash
- Checksum mismatch errors in transfer log
- Device stops responding after ENTER_BOOTLOADER but before transfer starts (USB disconnect at worst time)

**Phase to address:**
Late phase (firmware update feature). This should be one of the last features implemented, after all protocol commands are verified working. Implement extensive dry-run/simulation mode first.

---

### Pitfall 4: VID/PID Differences Between Reference Project and Target (0x3151 vs 0x3141)

**What goes wrong:**
The reference project targets VID 0x3151 (Akko/MonsGeek AT32F405/RY5088 keyboards). The target device (MonsGeek M5W) uses VID 0x3141 (yc3121 SoC). Directly copying device enumeration, HID interface matching, udev rules, or bootloader PIDs from the reference project results in the driver never finding the keyboard. The protocol commands are the same, but every single VID/PID-based lookup fails silently.

**Why it happens:**
The FEA command protocol is shared between the AT32F405 (RY5088) and yc3121 platforms. The reference project documentation says "same FEA command protocol structure" which encourages copying code. But the VID is different (0x3141 vs 0x3151), the PID is different (0x4005 vs 0x5030), the device ID is different (1308 vs 2949), and the bootloader PIDs differ (0x504A/0x404A vs 0x502A). Cargo-culting the reference project's constants without updating them for the target device means nothing connects.

**How to avoid:**
Create a device registry abstraction from the start. Every device constant (VID, PID, device ID, key count, key layout name, bootloader PID, SoC type) comes from a registry entry, not hardcoded constants. The M5W entry:

```
Device {
    name: "MonsGeek M5W",
    vid: 0x3141,
    pid: 0x4005,
    device_id: 1308,
    soc: "yc3121_m5w_soc",
    key_layout: "Common108_MG108B",
    bootloader_pids: [0x504A, 0x404A],
    interface: 2,
    usage_page: 0xFFFF,
    usage: 0x02,
}
```

Never hardcode VID/PID anywhere except the registry. All enumeration, matching, udev rules, and protocol code reference the registry.

**Warning signs:**
- `lsusb` shows the keyboard but the driver reports "no device found"
- hidapi enumeration returns empty results
- udev rules don't trigger on keyboard plug/unplug
- Bootloader detection fails after sending ENTER_BOOTLOADER

**Phase to address:**
Phase 1 (device discovery and HID transport). The device registry is the first thing to implement. All subsequent code depends on correct VID/PID matching.

---

### Pitfall 5: gRPC-Web Proto Schema Must Match Official App Exactly -- Including Typos

**What goes wrong:**
The MonsGeek web configurator (app.monsgeek.com) uses hardcoded protobuf message names, field numbers, and RPC method names. If the bridge server's .proto file differs in any way -- even "fixing" the typo `VenderMsg` to `VendorMsg`, or changing field numbers, or using a different package name -- the web app silently fails to communicate. No error messages appear in the web app. It just shows "Waiting for device" indefinitely.

**Why it happens:**
gRPC-Web encodes service/method names and field numbers in the wire protocol. The web app was compiled against a specific .proto, and it sends requests to exact paths like `/driver.DriverGrpc/watchDevList`. If the bridge uses a different package name, the path doesn't match. If a field number differs, deserialization produces zero/empty values. The temptation to "clean up" the proto schema (fix typos, rename fields, reorder) breaks compatibility silently.

**How to avoid:**
Copy the proto file from the reference project verbatim, including:
- Package name: `driver`
- Service name: `DriverGrpc`
- All RPC method names exactly as-is (camelCase, not snake_case)
- All message/enum names with their exact casing
- All field numbers unchanged
- The `VenderMsg` typo preserved (it matches the web app's compiled proto)
- The `DangleDevType` typo preserved (should be "Dongle" but the app uses "Dangle")

Verify by actually connecting the web app to the bridge and confirming the device list appears. Automated tests should compare the .proto file hash against the reference.

**Warning signs:**
- Web app shows "Waiting for device" when bridge is running
- Browser DevTools Network tab shows gRPC requests returning errors
- `watchDevList` stream connects but never emits device entries
- Commands sent from web app produce no keyboard response

**Phase to address:**
Phase 2 (gRPC bridge). The proto file must be copied verbatim before any server implementation begins. Integration testing with the real web app is the only reliable validation.

---

### Pitfall 6: CORS Configuration for gRPC-Web From HTTPS Origin to HTTP Localhost

**What goes wrong:**
The MonsGeek web app is served from `https://app.monsgeek.com` and makes gRPC-Web requests to `http://127.0.0.1:3814`. Browsers enforce CORS strictly: the bridge must respond to preflight OPTIONS requests with the correct `Access-Control-Allow-Origin`, `Access-Control-Allow-Headers` (including `x-grpc-web`, `content-type`, `x-user-agent`), and `Access-Control-Allow-Methods` headers. Missing or incorrect CORS headers cause the browser to silently block all gRPC communication. Additionally, some browsers block mixed-content (HTTPS page requesting HTTP localhost) unless specifically exempted.

**Why it happens:**
tonic-web handles gRPC-Web protocol translation but its built-in CORS only covers grpc-web-specific headers. The actual required headers depend on what the web app sends, which must be discovered by inspecting the real browser traffic. Developers typically test with curl or a Rust gRPC client, which don't enforce CORS. Everything works in testing, fails completely in the browser.

**How to avoid:**
1. Use `tower_http::cors::CorsLayer` composed with `tonic_web::GrpcWebLayer`
2. Allow origin: `https://app.monsgeek.com` (and `https://web.monsgeek.com` if applicable)
3. Allow headers: `content-type`, `x-grpc-web`, `x-user-agent`, `grpc-timeout`, `accept`
4. Allow methods: POST, OPTIONS
5. Expose headers: `grpc-status`, `grpc-message`, `grpc-status-details-bin`
6. Test in an actual browser with DevTools open, not curl
7. For mixed-content blocking: bind to `127.0.0.1` (not `0.0.0.0`), as browsers typically exempt localhost from mixed-content restrictions

**Warning signs:**
- curl to localhost:3814 works, browser doesn't
- Browser console shows "CORS policy" errors
- Network tab shows OPTIONS requests failing with no response
- Web app loads but never detects the keyboard

**Phase to address:**
Phase 2 (gRPC bridge). CORS must be configured correctly before any browser testing. Validate with the real web app in a real browser early.

---

### Pitfall 7: GET_MACRO Read Stride Bug -- Firmware Returns Wrong Data for Odd Macro Indices

**What goes wrong:**
The firmware's GET_MACRO (0x8B) handler uses a 512-byte stride to calculate the flash read address (`macroIndex * 512`), but SET_MACRO (0x0B) saves with a 256-byte stride (`macro_id * 256`). Reading back macro index 1 actually returns data from macro index 2. Reading macro index 2 returns empty data. The driver appears to read macros correctly for index 0, but all subsequent indices are wrong.

**Why it happens:**
This is a confirmed firmware bug (documented in `docs/bugs/get_macro_stride_bug.txt`). The official web app never calls GET_MACRO -- it caches macros in browser localStorage, so the bug was never caught. A third-party driver that reads macros from the device (e.g., for backup/restore, or to display current configuration) will hit this bug immediately.

**How to avoid:**
Two approaches:
1. **Work around it**: When reading macros, use the corrected stride (multiply index by 256, not 512) and read raw flash offsets via chunked reads. This requires deeper protocol knowledge.
2. **Cache locally**: Like the official app, maintain a local cache of macro definitions. After SET_MACRO, store the macro data locally. For GET_MACRO, serve from cache. Only read from the device for initial sync (accepting that odd indices will be wrong).
3. **Document the bug**: Clearly document that macro indices >= 1 cannot be reliably read from the device due to a firmware bug.

**Warning signs:**
- Macro backup/restore corrupts macros at odd indices
- Macro editor shows wrong macro content for indices 1, 3, 5...
- Macro at index 0 works perfectly, index 1 contains index 2's data

**Phase to address:**
Phase where macro support is implemented. This is a firmware-level bug with no fix available from the host side (unless firmware patching is in scope).

---

### Pitfall 8: Assuming Protocol Compatibility Without Verification for yc3121 vs AT32F405

**What goes wrong:**
PROJECT.md states "yc3121 keyboards use the same FEA command protocol structure as the AT32F405 keyboards." Developers take this as gospel and implement the full protocol based on the AT32F405 reference, only to discover that specific commands behave differently, have different data layouts, support different feature sets, or don't exist on the yc3121. The M5W (yc3121) is a non-HE (Hall Effect) keyboard -- it has no magnetism/analog features, no TMR sensors, no per-key actuation points. Sending magnetism commands to it produces undefined behavior.

**Why it happens:**
The FEA command protocol defines the message envelope (header, checksum, chunked write format). The commands within that envelope depend on the SoC and firmware version. The yc3121 is a different SoC than the AT32F405 (RY5088). While basic commands like GET_USB_VERSION, GET/SET_LEDPARAM, GET/SET_KEYMATRIX will likely work identically, HE-specific commands (SET/GET_MULTI_MAGNETISM, SET_MAGNETISM_CAL, SET_MAGNETISM_REPORT, key depth monitoring) are specific to the AT32F405 HE firmware and may not exist on the yc3121. The M5W firmware JS bundle must be analyzed to determine which commands are actually supported.

**How to avoid:**
1. **Feature detection first**: Before implementing any command, verify it works on the actual M5W hardware. Send the command, check for a valid response (not just zeros or echoed input).
2. **Parse the JS bundle**: The 41MB `dist/index.eb7071d5.js` from the MonsGeek Electron app contains the device definition for device ID 1308. Extract the feature flags, supported command list, and key matrix layout from it.
3. **Use GET_FEATURE_LIST (0xE6)**: This command returns a bitmap of supported features. Query it first and only expose commands that the device reports supporting.
4. **Incremental verification**: Implement and test one command at a time against the real hardware. Don't batch-implement 20 commands based on the reference protocol and test later.

**Warning signs:**
- Commands return all-zeros responses (command not implemented in firmware)
- Commands return the sent data echoed back unchanged (firmware ignoring the command)
- Keyboard stops responding after certain commands (undefined behavior)
- Web app shows features that don't work when configured

**Phase to address:**
Phase 1 (protocol implementation). Every command must be individually verified against the M5W hardware before being considered "implemented."

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Hardcoded VID/PID constants throughout the code | Faster initial development | Cannot add second keyboard without touching every file | Never -- use a device registry from the start |
| Passing raw byte arrays through the gRPC bridge without validation | Simpler bridge code, web app "just works" | Firmware OOB hazards exposed, bricking risk | Never -- validate all bytes at the bridge layer |
| Skipping the retry-and-match loop for HID reads | Faster, simpler read path | Stale data causes silent corruption of settings | Never -- Linux hidraw requires this |
| Caching device config in memory instead of reading from device | Faster operations, avoids read timing issues | Cache goes stale when user changes settings via Fn keys | Early MVP only -- must implement event-driven cache invalidation |
| Using `MODE="0666"` in udev rules | Works immediately for all users | Overly permissive, any process can talk to keyboard | MVP only -- switch to group-based access (MODE="0660", GROUP="plugdev") for release |
| Implementing firmware flash without dry-run mode | Faster development, fewer code paths | First real flash attempt may have bugs, no way to test safely | Never -- always implement dry-run simulation first |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| hidapi on Linux | Using `hid_write`/`hid_read` (interrupt transfers) instead of `hid_send_feature_report`/`hid_get_feature_report` | The vendor config interface (IF2) uses Feature Reports, not interrupt IN/OUT. Check the report descriptor: `b1 02` = Feature, `81 02` = Input, `91 02` = Output |
| hidapi device enumeration | Matching only on VID/PID | Must also match usage_page (0xFFFF), usage (0x02), and interface_number (2). The keyboard exposes 3 HID interfaces; the wrong one gives no vendor responses |
| tonic gRPC-Web | Using `tonic::transport::Server` directly | Must wrap services with `tonic_web::enable()` or `GrpcWebLayer`. Without this, the server speaks gRPC (HTTP/2 framing) but the web app sends gRPC-Web (HTTP/1.1 with base64 or binary) |
| protobuf field numbers | Renumbering fields for cleanliness | Field numbers are part of the wire format. Changing field 4 to field 3 silently breaks deserialization. Use the reference project's proto verbatim |
| udev + hidraw | Creating rules for SUBSYSTEM=="usb" only | Need BOTH: `SUBSYSTEM=="hidraw"` (for /dev/hidraw* access) and `SUBSYSTEM=="usb"` (for USB-level permissions). Missing either causes intermittent permission errors |
| Linux Report ID handling | Sending 64 bytes (the report size) | Linux hidraw prepends Report ID as byte 0, making the total 65 bytes. If Report ID is 0 (as it is here), byte 0 must be 0x00, followed by 64 bytes of payload |
| MonsGeek web app URL | Targeting web.monsgeek.com | The MonsGeek-branded app uses `app.monsgeek.com` (confirmed in PROJECT.md). The Akko-branded app uses `web.akkogear.com`. Both expect the driver at localhost:3814 |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Polling HID for events instead of using async reads | High CPU usage, delayed event processing, missed key depth reports | Use a dedicated thread for `hid_read` on the Input interface (IF1), with async channel to forward events | Key depth monitoring at 8KHz generates ~8000 events/sec; polling at 10ms misses 80% |
| Synchronous gRPC handlers blocking on HID I/O | Web app freezes during keyboard communication, timeouts | Use tokio async runtime; HID I/O in a blocking thread pool via `tokio::task::spawn_blocking` | Multiple concurrent requests from web app (e.g., reading all settings during device connect) |
| Reading all macro/keymap data at device connect time | 10-30 second device connection delay | Read lazily on first access, or read in background after initial device info query | Device with 4 profiles x 108 keys x 4 layers = many chunked reads |
| Unbounded event broadcast channel | Memory growth, slow event delivery | Use bounded broadcast channel (e.g., 256 entries) with overflow policy (drop oldest) | Key depth monitoring generates high-frequency events that overwhelm unbounded channels |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| No validation of gRPC payloads before HID send | Arbitrary HID commands can corrupt firmware RAM, overwrite flash regions, or brick device | Validate all command parameters at the bridge layer: command byte, parameter ranges, chunk indices, slot IDs |
| Binding gRPC server to 0.0.0.0 instead of 127.0.0.1 | Any machine on the network can send commands to the keyboard, including firmware erase | Bind exclusively to `127.0.0.1:3814`. Never expose to network interfaces |
| Firmware flash without integrity verification | Wrong firmware file can brick the device (bootloader erases app before transfer) | Verify chip ID header in firmware binary matches target device before entering bootloader |
| Running driver as root | Compromised driver has full system access | Use udev rules for HID permissions. Driver runs as regular user. Only BPF loading (if needed) requires root |
| Passing firmware update commands from web app without gate | Web app could trigger firmware flash accidentally or maliciously | Require explicit CLI flag or separate confirmation mechanism for firmware operations. Don't expose ENTER_BOOTLOADER through the gRPC bridge by default |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Silent failure when keyboard not found | User starts bridge, opens web app, sees nothing, has no idea why | Print clear status: "Scanning for keyboards... Found MonsGeek M5W at /dev/hidraw6" or "No MonsGeek keyboard found. Check USB connection and udev rules." |
| No feedback during firmware flash | User has no idea if flash is progressing, stuck, or failed | Stream progress percentage to stdout. Show chunk N/M. After completion, verify device re-enumerates with correct PID |
| udev rules not installed on first run | Driver fails with "Permission denied" and user gives up | Check for permission issues at startup. If EACCES on HID device, print exact udev rule to install and command to reload |
| Bridge running but web app on wrong URL | User tries web.monsgeek.com instead of app.monsgeek.com (or vice versa) | Document the exact URL. Consider printing it at bridge startup: "Open https://app.monsgeek.com in your browser" |
| Settings not persisting after keyboard power cycle | User changes LED mode via bridge, unplugs keyboard, settings reset | Some SET commands trigger flash save (indicated by SettingsAck event 0x0F), others don't. Document which settings persist and which are volatile |

## "Looks Done But Isn't" Checklist

- [ ] **HID communication**: Often missing retry-and-match logic for feature report reads -- verify that reading a setting back after setting it returns the new value, not the old value
- [ ] **Checksum calculation**: Often gets Bit7 vs Bit8 mode wrong for LED commands (SET_LEDPARAM uses Bit8, most other commands use Bit7) -- verify by capturing traffic from the official Windows app
- [ ] **Device enumeration**: Often matches VID/PID but not usage_page/usage/interface -- verify that enumeration finds exactly one device path (IF2), not the keyboard interface (IF0) or consumer interface (IF1)
- [ ] **gRPC device path format**: Often uses OS-level HID path instead of the synthetic "VID-PID-USAGE_PAGE-USAGE-INTERFACE" format the web app expects -- verify by connecting the web app and checking that device appears in the device picker
- [ ] **Chunked write commands**: Often implements single-chunk sends but breaks on multi-chunk (macros with >56 bytes, large keymaps) -- verify with a macro longer than 56 bytes
- [ ] **Firmware update checksum**: Often omits 0xFF padding bytes in final chunk from checksum calculation -- verify by computing checksum for a firmware binary whose size is NOT a multiple of 64
- [ ] **CORS headers**: Often works with curl but fails in browser -- verify with actual browser + DevTools Network tab showing successful preflight + request
- [ ] **Event forwarding**: Often implements command path but forgets to forward vendor events (0x05 reports) to the web app via watchVender stream -- verify by changing LED mode with Fn+Home and checking if web app UI updates

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Stale HID data corrupts settings | LOW | Read current settings from device, compare with expected. Re-send corrected SET commands. Worst case: factory reset (0x01) restores defaults |
| OOB write corrupts firmware RAM | MEDIUM | Power-cycle the keyboard (unplug/replug). RAM corruption is volatile. If behavior remains wrong, factory reset (0x01). If calibration is corrupted, re-run calibration procedure |
| Failed firmware flash (stuck in bootloader) | HIGH | Must use ROM DFU recovery via BOOT0 pin (physical hardware access). Connect BOOT0 to 3.3V, plug USB, flash stock firmware with dfu-util. If BOOT0 is not accessible, the device requires PCB-level intervention |
| Wrong VID/PID in code | LOW | Update constants in device registry and rebuild. No data loss or hardware impact |
| gRPC proto mismatch with web app | LOW | Copy proto from reference project. Rebuild. Compare wire format with working reference |
| CORS blocking browser requests | LOW | Add correct CORS headers. Restart bridge. Browser retries automatically |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Linux hidraw buffering (stale reads) | Phase 1: HID transport | Unit test: SET then GET returns new value, not old |
| Firmware OOB (no bounds checking) | Phase 1: Protocol layer | All SET commands reject out-of-range parameters with error |
| Bootloader point-of-no-return | Late phase: Firmware update | Dry-run mode passes; real flash tested only with known-good firmware |
| VID/PID differences (0x3141 vs 0x3151) | Phase 1: Device registry | M5W enumerated and queried via GET_USB_VERSION successfully |
| Proto schema mismatch | Phase 2: gRPC bridge | Web app detects keyboard and displays device info |
| CORS blocking | Phase 2: gRPC bridge | Browser DevTools shows successful gRPC-Web requests from HTTPS origin |
| GET_MACRO stride bug | Macro implementation phase | Document as known firmware limitation; implement local cache workaround |
| Protocol assumption (yc3121 vs AT32F405) | Phase 1: Protocol verification | Each command tested individually against M5W hardware, response validated |

## Sources

- Reference project PROTOCOL.md (Section 3.4: Linux hidraw buffering)
- Reference project `docs/bugs/oob_hazards.txt` (firmware bounds checking analysis)
- Reference project `docs/bugs/get_macro_stride_bug.txt` (macro read stride mismatch)
- Reference project `docs/HARDWARE.md` (bootloader behavior, flash memory map)
- Reference project `docs/FIRMWARE_PATCH.md` (recovery procedures)
- Reference project `iot_driver_linux/proto/driver.proto` (gRPC schema)
- Reference project `CLAUDE.md` (BPF loading notes, operational pitfalls)
- PROJECT.md (device specifics: VID 0x3141, PID 0x4005, device ID 1308)
- [HIDAPI Linux feature report issues (libusb/hidapi#174)](https://github.com/libusb/hidapi/issues/174)
- [HIDAPI consecutive read hang (signal11/hidapi#110)](https://github.com/signal11/hidapi/issues/110)
- [tonic gRPC-Web CORS (hyperium/tonic#270)](https://github.com/hyperium/tonic/issues/270)
- [tonic_web NamedService issue (hyperium/tonic#1312)](https://github.com/hyperium/tonic/issues/1312)
- [OpenRazer Reverse Engineering USB Protocol guide](https://github.com/openrazer/openrazer/wiki/Reverse-Engineering-USB-Protocol)
- [Linux HIDRAW kernel documentation](https://docs.kernel.org/hid/hidraw.html)

---
*Pitfalls research for: Linux HID keyboard driver / configurator bridge (MonsGeek yc3121)*
*Researched: 2026-03-19*
