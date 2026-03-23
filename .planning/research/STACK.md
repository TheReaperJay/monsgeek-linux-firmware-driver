# Stack Research

**Domain:** Linux FEA keyboard framework and configurator bridge  
**Researched:** 2026-03-19  
**Corrected:** 2026-03-23

## Recommended Core Stack

| Dependency | Role | Notes |
|------------|------|-------|
| `rusb` | USB control transfers and interface ownership | Correct backend for the wired M5W path |
| `crossbeam-channel` | transport-thread request queue | Good fit for serialized HID I/O |
| `udev` | hot-plug monitoring | Preferred over `libusb` arrival callbacks in this environment |
| `log` | diagnostics | Lightweight and sufficient for transport |
| `thiserror` | error types | Already fits the Rust crate layout |
| `serde`, `serde_json` | registry/profile data | Required for data-driven device support |

## Recommended Bridge Stack

| Dependency | Role |
|------------|------|
| `tokio` | async runtime for server layer |
| `tonic` | gRPC server |
| `tonic-web` | gRPC-Web compatibility |
| `tower-http` | CORS handling |
| `prost` / `tonic-build` | protobuf code generation |

## Project Crate Boundaries

| Crate | Role |
|-------|------|
| `monsgeek-protocol` | protocol constants, checksums, command tables, registry/profile data |
| `monsgeek-transport` | USB session, discovery, flow control, transport thread, optional input path |
| bridge / driver crate | gRPC bridge, CLI, service entrypoints |

## Stack Decisions To Avoid

| Do Not Treat As Canonical | Why |
|---------------------------|-----|
| `hidapi` / hidraw as the primary MonsGeek transport backend | Not the reliable low-level truth for the validated M5W path |
| `tokio-udev` as planning truth | The project currently uses `udev` in the transport layer directly |
| `libusb` hot-plug callbacks as the main add/remove source | Arrival reliability was not good enough here |

## System Considerations

- Linux only
- real host-side USB access required for hardware validation
- userspace must be deliberate about `IF0` ownership

---
*Stack research corrected after hardware validation on 2026-03-23*
