# Architecture Research

**Domain:** Linux HID keyboard driver and configurator bridge (MonsGeek yc3121)
**Researched:** 2026-03-19
**Confidence:** HIGH

## Standard Architecture

### System Overview

```
+---------------------------------------------------------------+
|  MonsGeek Web Configurator (app.monsgeek.com)                 |
|  - React SPA in browser                                       |
|  - Uses @protobuf-ts/grpcweb-transport                        |
+------------------------------+--------------------------------+
                               | gRPC-Web (HTTP/2)
                               | localhost:3814
                               v
+---------------------------------------------------------------+
|  Bridge Server (Rust binary)                                  |
|  +------------------+  +------------------+  +--------------+ |
|  | gRPC Service     |  | Device Manager   |  | DB Store     | |
|  | (tonic + web)    |  | (hotplug, enum)  |  | (sled/mem)   | |
|  +--------+---------+  +--------+---------+  +--------------+ |
|           |                     |                              |
|  +--------v---------------------v-----------+                  |
|  | Keyboard Interface                       |                  |
|  | (typed API: LED, keymap, macros, etc.)   |                  |
|  +--------+---------------------------------+                  |
|           |                                                    |
|  +--------v---------------------------------+                  |
|  | Flow Control Transport                   |                  |
|  | (retries, echo matching, cmd delay)      |                  |
|  +--------+---------------------------------+                  |
|           |                                                    |
|  +--------v---------------------------------+                  |
|  | Raw Transport (HID I/O)                  |                  |
|  | - HidWiredTransport (Feature Reports)    |                  |
|  +--------+---------------------------------+                  |
+-----------|-----------------------------------------------+----+
            | HID Feature Reports (SET_FEATURE / GET_FEATURE)
            | 65 bytes (Report ID 0 + 64 bytes payload)
            v
+---------------------------------------------------------------+
|  MonsGeek M5W Keyboard (yc3121 SoC)                           |
|  - IF0: Boot Keyboard (standard 6KRO)                         |
|  - IF1: Multi-function (Mouse, Consumer, NKRO, Vendor Input)  |
|  - IF2: Vendor Config (Usage Page 0xFFFF, Usage 0x02)         |
|  - 64-byte feature reports with Bit7 checksum                 |
+---------------------------------------------------------------+
```

### Component Responsibilities

| Component | Responsibility | Implementation |
|-----------|----------------|----------------|
| **gRPC Service** | Translate gRPC-Web calls from web configurator into HID operations. Implements the DriverGrpc proto service exactly as the Windows `iot_driver.exe` does. | tonic gRPC server with tonic-web middleware, tower-http CORS layer. Listens on `127.0.0.1:3814`. |
| **Device Manager** | Enumerate HID devices by VID/PID/usage, detect hot-plug via udev, open transports, track connected devices. | hidapi for enumeration, tokio-udev for hot-plug monitoring. Maintains a `HashMap<path, ConnectedTransport>`. |
| **Device Registry** | Map device IDs to definitions (key count, layout, features, LED matrix). Data-driven: new keyboards added via JSON, not code. | JSON device definitions loaded at startup. Includes key matrices, LED matrices, per-device capabilities. |
| **Keyboard Interface** | High-level typed API for keyboard features (LED control, key remapping, macro programming, trigger settings). Hides protocol byte manipulation. | Wraps FlowControlTransport. Methods like `set_led_params()`, `get_key_matrix()`, `set_macro()`. Protocol-family-aware. |
| **Flow Control Transport** | Adds reliability on top of raw HID I/O: command-response serialization, echo matching (verify response CMD byte matches request), inter-command delay, retries on timeout. | Wraps raw Transport. For wired: simple send-delay-read with mutex serialization. |
| **Raw Transport** | Send/receive individual HID feature reports. No retries, no validation. Blocking I/O via hidapi. | `HidWiredTransport`: `send_feature_report()` / `get_feature_report()` via hidapi. Separate event reader thread for IF1 input reports. |
| **Protocol Layer** | Command constants (FEA_CMD_*), packet framing (report ID + cmd + data + checksum), checksum calculation (Bit7/Bit8), response parsing. | Pure functions: `build_command()`, typed `HidCommand`/`HidResponse` traits with `to_data()`/`from_data()`. |
| **Firmware Flash Engine** | Bootloader entry, device re-enumeration, chunk transfer with CRC-24 verification, progress reporting. Destructive operation requiring explicit safety gates. | Separate from normal transport: opens bootloader device directly via hidapi after keyboard reboots into bootloader mode. |
| **DB Store** | Persist web app preferences (the configurator stores UI state via gRPC DB RPCs). | In-memory `HashMap` (reference uses sled). The web app calls `insertDb`/`getItemFromDb` for local settings. |
| **udev Rules** | Grant non-root access to hidraw devices. | Rules file matching VID `0x3141`, setting `MODE="0666"` on hidraw subsystem. |

## Recommended Project Structure

```
monsgeek-firmware-driver/
+-- Cargo.toml                      # Workspace root
+-- proto/
|   +-- driver.proto                # gRPC service definition (matches Windows iot_driver)
+-- monsgeek-transport/             # Crate: raw HID transport + protocol
|   +-- Cargo.toml
|   +-- src/
|       +-- lib.rs                  # Transport trait, re-exports
|       +-- protocol.rs             # FEA_CMD constants, build_command(), checksums
|       +-- command.rs              # HidCommand/HidResponse traits, typed builders
|       +-- hid_wired.rs            # HidWiredTransport (Feature Report I/O)
|       +-- flow_control.rs         # FlowControlTransport (retries, echo matching)
|       +-- discovery.rs            # Device enumeration via hidapi + udev hotplug
|       +-- event_parser.rs         # Parse vendor input reports (IF1)
|       +-- types.rs                # ChecksumType, TransportDeviceInfo, VendorEvent
|       +-- error.rs                # TransportError
+-- monsgeek-keyboard/              # Crate: high-level keyboard API
|   +-- Cargo.toml
|   +-- src/
|       +-- lib.rs                  # KeyboardInterface
|       +-- led.rs                  # LED mode/params types
|       +-- settings.rs             # FirmwareVersion, PollingRate, etc.
|       +-- error.rs                # KeyboardError
+-- src/                            # Main binary crate
|   +-- main.rs                     # Entry point, CLI dispatch, gRPC server startup
|   +-- grpc.rs                     # DriverService (gRPC handler implementations)
|   +-- cli.rs                      # CLI argument definitions (clap)
|   +-- commands/                   # CLI command handlers
|   |   +-- mod.rs
|   |   +-- query.rs                # GET commands (info, led, debounce, etc.)
|   |   +-- set.rs                  # SET commands (set-led, set-profile, etc.)
|   |   +-- firmware.rs             # Firmware flash commands
|   |   +-- keymap.rs               # Key remapping commands
|   |   +-- macros.rs               # Macro programming commands
|   +-- device_loader.rs            # JSON device definition loader
|   +-- devices.rs                  # Device support checking (VID/PID matching)
|   +-- firmware.rs                 # Firmware file parsing/validation
|   +-- flash.rs                    # Firmware flash engine (bootloader protocol)
|   +-- hal/                        # Hardware abstraction (constants, interface types)
|   |   +-- mod.rs
|   |   +-- constants.rs            # VID, PID, usage page/usage values
|   |   +-- interface.rs            # HidInterface type (VID/PID/usage matching)
|   |   +-- registry.rs             # Known device interfaces
|   +-- protocol.rs                 # App-level protocol extensions beyond transport
+-- data/                           # JSON device databases (extracted from configurator)
|   +-- devices.json                # Device definitions (ID, name, key count, etc.)
|   +-- device_matrices.json        # Key position matrices per layout
|   +-- led_matrices.json           # LED position-to-key mappings
|   +-- key_codes.json              # HID keycode names/values
|   +-- key_layouts.json            # Layout name to key position mapping
+-- udev/
|   +-- 99-monsgeek.rules           # udev rules for non-root hidraw access
+-- references/                     # Reference materials (not compiled)
+-- firmware/                       # Firmware binaries and extracted app
```

### Structure Rationale

- **monsgeek-transport/:** Isolated crate for raw HID communication. No knowledge of keyboard features, gRPC, or UI. Depends only on hidapi, tokio (for broadcast channels), and parking_lot. This boundary is critical: the transport must be testable in isolation and swappable (wired today, dongle later).

- **monsgeek-keyboard/:** High-level typed API on top of transport. Depends on monsgeek-transport. Knows about LED modes, trigger settings, key matrices, but not about gRPC or CLI. This is the "domain model" of the keyboard.

- **src/ (main binary):** Ties everything together: gRPC server, CLI, device loading, firmware flashing. Depends on both crates. This is where the bridge logic lives.

- **data/:** JSON files extracted from the MonsGeek web/Electron app. The device registry is data-driven: adding a new keyboard means adding its definition to these files, not writing Rust code. This is a core project constraint.

- **proto/:** Protobuf service definition. Must match the Windows `iot_driver.exe` contract exactly, including typos (e.g., `watchVender` not `watchVendor`, `DangleDevType` not `DongleDevType`).

## Architectural Patterns

### Pattern 1: Transport Trait Abstraction

**What:** A `Transport` trait defines the raw I/O contract (send_report, read_report, read_event). Concrete implementations handle wired HID, with future dongle/BLE implementations using the same trait. Flow control wraps any transport to add reliability.

**When to use:** Always. Every higher layer interacts through this trait, never with hidapi directly.

**Trade-offs:** Adds an indirection layer, but the benefit is enormous: testability (mock transport for unit tests), future extensibility (dongle support), and separation of HID quirks from protocol logic.

**Example:**
```rust
pub trait Transport: Send + Sync {
    fn send_report(&self, cmd: u8, data: &[u8], checksum: ChecksumType) -> Result<(), TransportError>;
    fn read_report(&self) -> Result<Vec<u8>, TransportError>;
    fn read_event(&self, timeout_ms: u32) -> Result<Option<VendorEvent>, TransportError>;
    fn device_info(&self) -> &TransportDeviceInfo;
    fn is_connected(&self) -> bool;
    fn close(&self) -> Result<(), TransportError>;
}

// Layering: Raw I/O -> Flow Control -> Keyboard Interface -> gRPC/CLI
// HidWiredTransport implements Transport (raw I/O)
// FlowControlTransport wraps Transport (adds retries, echo matching)
// KeyboardInterface wraps FlowControlTransport (typed keyboard API)
```

### Pattern 2: Typed Command/Response Pairs

**What:** Each HID command is a struct implementing `HidCommand` (with `CMD`, `CHECKSUM`, `to_data()`). Each response is a struct implementing `HidResponse` (with `CMD_ECHO`, `MIN_LEN`, `from_data()`). Protocol byte manipulation is centralized in these types, not scattered across callers.

**When to use:** For every FEA command. The alternative (raw byte arrays everywhere) is error-prone and unmaintainable.

**Trade-offs:** More boilerplate up front, but prevents entire classes of bugs: wrong checksum type, wrong byte order, missing validation. Bounds checking in command builders prevents firmware corruption (the yc3121 firmware has NO bounds checking on indices -- out-of-range values corrupt RAM/flash).

**Example:**
```rust
pub trait HidCommand: Sized {
    const CMD: u8;
    const CHECKSUM: ChecksumType;
    fn to_data(&self) -> Vec<u8>;
    fn build(&self) -> Vec<u8> {
        protocol::build_command(Self::CMD, &self.to_data(), Self::CHECKSUM)
    }
}

pub struct SetLedParams { pub mode: u8, pub brightness: u8, /* ... */ }
impl HidCommand for SetLedParams {
    const CMD: u8 = 0x07;
    const CHECKSUM: ChecksumType = ChecksumType::Bit7;
    fn to_data(&self) -> Vec<u8> { /* serialize fields */ }
}
```

### Pattern 3: gRPC-Web Bridge as Protocol Translator

**What:** The gRPC service is a thin translation layer. The web configurator sends `sendMsg(devicePath, bytes, checksumType)` -- the bridge routes to the correct transport, frames the HID report, sends it, reads the response, returns bytes. The bridge does NOT interpret most commands; it's a pass-through for raw HID with device management on top.

**When to use:** This is the primary operating mode. The web configurator already knows the full protocol -- it just needs a bridge to reach the hardware.

**Trade-offs:** The bridge must match the Windows `iot_driver.exe` gRPC contract exactly (same proto, same field names, same behavior). This means preserving upstream typos and quirks. The benefit is zero modification to the web configurator.

### Pattern 4: Data-Driven Device Registry

**What:** Device definitions (key count, layout name, LED matrix, capabilities) live in JSON files extracted from the MonsGeek configurator app. The binary loads these at startup and uses them to configure the KeyboardInterface.

**When to use:** Always. A core project constraint is that adding new yc3121 keyboards requires only adding JSON definitions, not code changes.

**Trade-offs:** Requires JSON extraction tooling and schema validation. But eliminates code changes for new devices, which is the right trade-off for a keyboard ecosystem that ships new models regularly.

## Data Flow

### Configuration Command Flow (Web App -> Keyboard)

```
Web App (browser)
    | gRPC-Web: sendMsg(devicePath="12609-16389-65535-2-2", msg=[0x07,...], checksum=Bit7)
    v
gRPC Service (DriverService::send_msg)
    | 1. Look up device by path in connected devices HashMap
    | 2. Convert proto CheckSumType to transport ChecksumType
    | 3. Extract cmd byte and data from msg bytes
    v
Raw Transport (HidWiredTransport::send_report)
    | 1. protocol::build_command(cmd, data, checksum) -> 65-byte buffer
    |    - buf[0] = Report ID (0x00)
    |    - buf[1] = cmd byte
    |    - buf[2..] = data bytes
    |    - buf[7] = 255 - (sum(buf[1..7]) & 0xFF)  (Bit7 checksum)
    | 2. hidapi::send_feature_report(&buf)
    v
Keyboard (yc3121 SoC, IF2)
    | Processes command, writes response to Feature Report buffer
    v
Raw Transport (HidWiredTransport::read_report)
    | 1. hidapi::get_feature_report(&mut buf) -> 65 bytes
    | 2. Strip Report ID, return buf[1..] (64 bytes)
    v
gRPC Service
    | Return ResRead { err: "", msg: response_bytes }
    v
Web App (processes response)
```

### Device Discovery Flow

```
1. STARTUP:
   HidDiscovery::list_devices()
     -> hidapi::HidApi::new() -> enumerate all HID devices
     -> Filter by VID (0x3141), Usage Page (0xFFFF), Usage (0x02), Interface (2)
     -> Return Vec<DiscoveredDevice>

2. HOTPLUG (runtime):
   tokio-udev MonitorBuilder
     -> Watch for hidraw add/remove events
     -> Filter by vendor attribute
     -> Emit DiscoveryEvent::Added / DiscoveryEvent::Removed
     -> gRPC watchDevList stream pushes DeviceList update to web app

3. DEVICE OPEN:
   HidDiscovery::open_device(discovered)
     -> hidapi::HidApi::open_path(hidraw_path) for IF2 (feature)
     -> Optionally open IF1 (input events) on same USB bus
     -> Construct HidWiredTransport(feature_device, input_device, info)
     -> Wrap in FlowControlTransport
     -> Query GET_USB_VERSION (0x8F) to get device_id
     -> Store in DriverService.devices HashMap
```

### Firmware Update Flow

```
1. Web App: upgradeOTAGATT(devPath, file_buf)
     |
2. gRPC Service: validate firmware bytes (header, CRC-24)
     |
3. Flash Engine: send SET_RESET with bootloader magic bytes
     |  Keyboard reboots into RY bootloader (different PID!)
     |  WARNING: Bootloader erases app region before USB init
     |
4. Flash Engine: poll hidapi for bootloader device (new PID)
     |  Timeout after ~15 seconds if bootloader not found
     |
5. Flash Engine: transfer firmware in 56-byte chunks
     |  Each chunk: [page_index, chunk_data...]
     |  Final chunk: commit command
     |
6. Flash Engine: keyboard reboots into new firmware
     |  Report progress via streaming gRPC response
     v
7. Web App: shows progress bar, verifies new firmware version
```

### Vendor Event Flow (Keyboard -> Web App)

```
Keyboard IF1 (Input Reports)
    | Background reader thread on IF1 (HidDevice::read with timeout)
    v
EventSubsystem (parse_usb_event)
    | Parse report ID 0x05 into VendorEvent variants
    | (KeyDepth, ProfileChange, BatteryStatus, etc.)
    v
broadcast::Sender<TimestampedEvent>
    | Multiple subscribers can receive same event
    v
gRPC Service (watchVender stream)
    | Convert VendorEvent to raw bytes matching Windows format
    | Prepend USB report ID (0x05)
    v
Web App (processes real-time events)
```

## Build Order (Dependency Graph)

The workspace crate dependency chain dictates build order:

```
Phase 1: monsgeek-transport (no internal deps)
   - Transport trait
   - Protocol constants (FEA_CMD_*)
   - Packet framing + checksum
   - HidCommand/HidResponse traits
   - HidWiredTransport
   - FlowControlTransport
   - Device discovery
   - Event parsing

Phase 2: monsgeek-keyboard (depends on monsgeek-transport)
   - KeyboardInterface
   - Typed feature APIs (LED, triggers, keymap, macros)
   - Settings types

Phase 3: Main binary (depends on both crates)
   - Proto compilation (driver.proto -> Rust)
   - gRPC service (DriverService)
   - Device loader (JSON -> DeviceDefinition)
   - CLI
   - Firmware flash engine

Phase 4: System integration
   - udev rules
   - Testing with actual hardware
   - Web configurator compatibility validation
```

**Build order rationale:**
- Transport is the foundation: nothing works without HID I/O. It can be tested against real hardware immediately (send a GET_REV command, read firmware version).
- Keyboard interface comes second because it provides the typed API that both CLI and gRPC depend on.
- gRPC service is built last because it is the thinnest layer: mostly forwarding bytes between web app and transport, with device management on top.
- Firmware flashing is the highest-risk feature and should be built and tested last, after the core protocol is proven correct.

## Anti-Patterns

### Anti-Pattern 1: Interpreting All Commands in the Bridge

**What people do:** Parse every HID command/response in the gRPC bridge, building a full keyboard state model.

**Why it's wrong:** The web configurator already understands the full protocol. The bridge is a transport proxy, not a keyboard manager. Over-interpreting wastes effort and creates divergence when the web app is updated.

**Do this instead:** Pass raw bytes through for `sendMsg`/`readMsg`. Only interpret commands when the bridge needs to (device identification via GET_USB_VERSION, firmware flashing, vendor event routing). The reference project follows this pattern: `sendMsg` directly calls `transport.send_report()` with the raw bytes.

### Anti-Pattern 2: Async HID I/O

**What people do:** Try to make hidapi calls async with `spawn_blocking` everywhere, or use an async HID library.

**Why it's wrong:** hidapi is fundamentally synchronous and blocking. The underlying Linux syscalls (`ioctl` for feature reports, `read` for input reports) are blocking. Wrapping every call in `spawn_blocking` adds complexity without benefit for a single-device scenario.

**Do this instead:** Use synchronous HID I/O in the transport layer (as the reference project does). Use async only at the gRPC server level (tonic requires tokio). Bridge the sync/async boundary at the gRPC handler with `tokio::task::spawn_blocking` for individual RPC calls. The reference project's Transport trait methods are all synchronous.

### Anti-Pattern 3: Custom gRPC Proto Definition

**What people do:** Design a "better" proto definition with cleaner field names and proper spelling.

**Why it's wrong:** The web configurator is compiled against a specific proto contract. Changing field names, fixing typos (VenderMsg -> VendorMsg, DangleDevType -> DongleDevType), or restructuring messages breaks compatibility. The whole point is zero-modification browser compatibility.

**Do this instead:** Copy the proto definition from the reference project verbatim, typos and all. The proto is a compatibility contract, not a design document.

### Anti-Pattern 4: Stateful Device Sessions

**What people do:** Maintain complex session state per device (current profile, cached LED settings, dirty tracking).

**Why it's wrong:** The keyboard firmware is the source of truth. The web app queries the keyboard directly for current state. Caching in the bridge creates stale state bugs, especially when the user changes settings via physical keyboard buttons.

**Do this instead:** Be stateless: forward commands, return responses. The only state the bridge should track is: which devices are connected, and their transport handles. The reference project's DriverService stores only `HashMap<path, ConnectedTransport>` plus a broadcast channel.

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| MonsGeek Web Configurator (app.monsgeek.com) | gRPC-Web over HTTP/2 on localhost:3814 | Must match Windows `iot_driver.exe` proto contract exactly. CORS must allow the web app's origin. |
| Linux HID subsystem | hidapi (userspace) via `/dev/hidrawN` | Requires udev rules for non-root access. Feature reports on IF2, input reports on IF1. |
| Linux udev | tokio-udev for hot-plug detection | Watches for `hidraw` subsystem events with vendor ID filter. |

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| gRPC Service <-> Transport | Synchronous method calls via `Arc<dyn Transport>` | gRPC handlers use `spawn_blocking` to call synchronous transport methods from async context. |
| Transport <-> hidapi | Direct FFI calls (hidapi is C library with Rust bindings) | Feature reports: `send_feature_report()` / `get_feature_report()`. Input reports: `read()` with timeout on a background thread. |
| gRPC Service <-> Web App | Protobuf-encoded gRPC-Web | Bidirectional: RPCs for commands, server-streaming for device list changes and vendor events. |
| Device Manager <-> udev | tokio-udev async stream | Hot-plug events trigger device re-enumeration and DeviceList broadcast to connected web app clients. |
| Main binary <-> JSON data | serde_json deserialization at startup | Device definitions loaded once. Key layout and LED matrix lookups by device ID. |

## Sources

- Reference project source code: `references/monsgeek-akko-linux/` (complete Rust implementation for Akko/MonsGeek keyboards, same FEA protocol)
- Reference project protocol documentation: `references/monsgeek-akko-linux/docs/PROTOCOL.md`
- Reference project hardware documentation: `references/monsgeek-akko-linux/docs/HARDWARE.md`
- gRPC proto definition: `references/monsgeek-akko-linux/iot_driver_linux/proto/driver.proto`
- Confidence: HIGH -- architecture derived directly from a working implementation targeting the same protocol family

---
*Architecture research for: MonsGeek Linux HID keyboard driver and configurator bridge*
*Researched: 2026-03-19*
