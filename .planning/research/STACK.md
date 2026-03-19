# Stack Research

**Domain:** Linux HID keyboard driver with gRPC-Web bridge
**Researched:** 2026-03-19
**Confidence:** HIGH

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Rust | 1.94.0 stable, edition 2024 | Implementation language | Same language as the proven reference project. Zero-cost abstractions matter for HID timing. Edition 2024 brings async closures (useful for tokio/tonic patterns) and is the current stable default. |
| tonic | 0.14 | gRPC server framework | The standard Rust gRPC implementation. Version 0.14 is current stable, uses hyper 1.0 ecosystem. The reference project uses 0.12 but 0.14 is a clean break point for a new project. |
| tonic-web | 0.14 | gRPC-Web protocol translation | Converts gRPC-Web (HTTP/1.1 from browsers) to native gRPC without an external proxy like Envoy. Version must match tonic major version. The MonsGeek web configurator at app.monsgeek.com uses gRPC-Web to talk to the bridge. |
| tonic-build | 0.14 | Proto code generation (build dep) | Generates Rust types and server traits from .proto files at compile time. Must match tonic version. |
| prost | 0.14 | Protocol Buffers serialization | Tonic's default protobuf codec. Version 0.14 aligns with tonic 0.14 (they moved prost integration to tonic-prost in 0.14). |
| tokio | 1 | Async runtime | The async runtime that tonic, tokio-udev, and the entire async ecosystem depends on. Use `features = ["full"]` for the main binary; `features = ["sync"]` for library crates that only need channels/mutexes. |
| hidapi | 2.6 | HID device communication | Rust bindings for libhidapi. Provides Feature Report send/receive on Linux via hidraw. The reference project uses this successfully. Version 2.6.5 is latest. |
| tokio-udev | 0.10 | Async udev hotplug monitoring | Detects keyboard connect/disconnect events asynchronously. Version 0.10.0 (Oct 2025) is current. The reference project uses 0.9; 0.10 is the latest. |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tower-http | 0.6 | CORS middleware | `features = ["cors"]`. Required for browser-to-localhost gRPC-Web. The MonsGeek web configurator needs permissive CORS (Allow-Origin: *, Allow-Headers: *, etc). |
| http | 1.0 | HTTP types | Required by tower-http and tonic for header/method types. |
| tokio-stream | 0.1 | Stream utilities for tokio | `features = ["sync"]`. Wraps broadcast receivers as Streams for tonic server-streaming RPCs (watchDevList, watchVender). |
| clap | 4.6 | CLI argument parsing | `features = ["derive"]`. For the `monsgeek-driver` binary CLI (serve, info, set-led, etc). Current stable. |
| tracing | 0.1 | Structured logging | The standard Rust logging/tracing framework. Pairs with tracing-subscriber. |
| tracing-subscriber | 0.3 | Log output formatting | `features = ["env-filter"]`. Enables `RUST_LOG=debug` style filtering. |
| thiserror | 2.0 | Error type derivation | Derive `Error` trait on custom error enums. Version 2.0 is current (released Nov 2024). |
| anyhow | 1.0 | Application-level errors | For the binary crate's top-level error handling. Library crates should use thiserror for typed errors. |
| serde | 1.0 | Serialization framework | `features = ["derive"]`. For device registry JSON, config files. |
| serde_json | 1.0 | JSON serialization | For loading device definitions from JSON files (device registry). |
| zerocopy | 0.8 | Zero-copy byte parsing | `features = ["derive"]`. For parsing 64-byte HID reports into structured command/response types without copying. Critical for the FEA protocol layer where reports have well-defined byte layouts. |
| parking_lot | 0.12 | Faster synchronization primitives | Smaller and faster Mutex/RwLock than std. Used in the transport layer where HID device handles are shared across threads. |
| futures | 0.3 | Async stream combinators | For `StreamExt`, `Stream` trait, and async stream utilities needed by tonic streaming RPCs. |
| async-stream | 0.3 | Async stream macros | Ergonomic `stream! {}` macro for creating async streams in gRPC server streaming methods. |
| libc | 0.2 | C FFI types | Required for low-level udev/ioctl interactions on Linux. |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| protobuf-compiler (protoc) | Compiles .proto files | System package: `sudo dnf install protobuf-compiler` on Fedora. Required by tonic-build at compile time. |
| cargo-watch | Auto-rebuild on file change | `cargo install cargo-watch`. Run `cargo watch -x check` during development. |
| hidapi-devel | C library headers | System package: `sudo dnf install hidapi-devel` on Fedora. Required by the hidapi crate's default `linux-static-hidraw` backend. |
| systemd-devel | udev headers | System package: `sudo dnf install systemd-devel` on Fedora. Required by tokio-udev for libudev bindings. |
| usbutils | USB debugging | `lsusb` for verifying keyboard VID:PID enumeration. Already installed on most distros. |
| wireshark/usbmon | Protocol debugging | For capturing USB HID traffic to verify command/response correctness against the reference project. |

## Installation

```bash
# System dependencies (Fedora)
sudo dnf install gcc make pkgconf-pkg-config systemd-devel hidapi-devel protobuf-compiler

# Create project
cargo init monsgeek-firmware-driver
cd monsgeek-firmware-driver

# Core dependencies
cargo add tonic@0.14 tonic-web@0.14 prost@0.14
cargo add tokio --features full
cargo add tokio-stream --features sync
cargo add tower-http --features cors
cargo add http@1.0
cargo add hidapi@2.6
cargo add tokio-udev@0.10
cargo add clap --features derive
cargo add tracing tracing-subscriber --features tracing-subscriber/env-filter
cargo add thiserror@2.0 anyhow@1.0
cargo add serde --features derive
cargo add serde_json
cargo add zerocopy --features derive
cargo add parking_lot@0.12
cargo add futures@0.3 async-stream@0.3
cargo add libc@0.2

# Build dependencies
cargo add --build tonic-build@0.14
```

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| tonic 0.14 | tonic 0.12 (matching reference project) | Only if proto compatibility with reference project's generated code is needed. Since this is a standalone project, using the latest version avoids starting on a version that's two majors behind. |
| hidapi (linux-static-hidraw) | hidapi (linux-native) | The `linux-native` feature uses a pure Rust hidraw implementation (no C library). Appealing but less battle-tested than the C library backend. Use if you want to eliminate the `hidapi-devel` system dependency. |
| hidapi | nusb | Never for this project. nusb is for raw USB (non-HID) devices. The M5W keyboard exposes a standard HID interface; hidapi is the correct tool. |
| tokio-udev 0.10 | inotify on /dev/hidraw* | Only if tokio-udev causes issues. inotify is lower-level and doesn't give you structured device metadata (VID/PID). Stick with tokio-udev. |
| tonic-web (built-in) | Envoy proxy for gRPC-Web | Only if you need to support bidirectional streaming from the browser (tonic-web only supports unary and server-streaming). The MonsGeek configurator only uses unary and server-streaming RPCs, so tonic-web is sufficient. |
| thiserror 2.0 | thiserror 1.0 | Never. The reference project already uses 2.0. No reason to start on the old version. |
| zerocopy | bytemuck | When you only need simple transmutations. zerocopy provides richer derive macros for structured protocol parsing. Reference project uses zerocopy. |
| edition 2024 | edition 2021 | Only if a dependency requires it, but all major crates (tonic, tokio, etc.) support edition 2024. Starting a greenfield project in 2026 on the 2021 edition is leaving features on the table. |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| sled (embedded DB) | Abandoned for years, never left beta. The reference project uses it for "database operations" but the MonsGeek web app barely uses the DB RPCs. | redb 3.1 if you need key-value storage, or just skip it entirely. The DB RPCs (getItemFromDb, insertDb, etc.) can return stub responses initially; the configurator functions without them. |
| nusb | Designed for non-HID USB devices. The keyboard is a standard HID device. Using nusb would mean reimplementing HID report handling that hidapi already provides. | hidapi 2.6 |
| reqwest (for firmware download) | Out of scope for v1. Firmware files are local. Adding an HTTP client adds 50+ transitive dependencies. | Defer entirely. Firmware files are provided locally. Add reqwest behind a feature gate only if cloud firmware download becomes a requirement. |
| ratatui / crossterm (TUI) | Out of scope for v1. The project goal is a gRPC-Web bridge, not a TUI. The reference project has a TUI but it's a feature, not a requirement. | Defer entirely. CLI output via tracing is sufficient. TUI is a future milestone. |
| cpal / spectrum-analyzer (audio) | Audio-reactive LED modes are explicitly out of scope per PROJECT.md. | Omit. |
| ashpd / pipewire (screen capture) | Screen sync LED mode is explicitly out of scope per PROJECT.md. | Omit. |
| aya (eBPF) | eBPF HID driver is deferred per PROJECT.md. Only needed if configurator-based debounce tuning doesn't fix the ghosting issue. | Defer. Add behind a feature gate in a later milestone if needed. |

## Stack Patterns by Variant

**For the gRPC-Web bridge (primary target):**
- Use tonic + tonic-web + tower-http CORS
- `Server::builder().accept_http1(true)` is mandatory for gRPC-Web
- Wrap each service with `tonic_web::enable()` before adding to server
- Browser sends HTTP/1.1 POST with gRPC-Web content type; tonic-web translates to native gRPC

**For HID communication:**
- Use hidapi with default `linux-static-hidraw` backend
- Open device by VID/PID/usage_page/interface number (not path string)
- Feature reports (64 bytes) via `send_feature_report()` / `get_feature_report()`
- The vendor config interface is IF2 with usage_page 0xFF00

**For device hotplug:**
- Use tokio-udev to monitor udev events filtered by VID/PID
- Broadcast device list changes via tokio broadcast channels
- gRPC `watchDevList` RPC streams these broadcast events to the browser

## Version Compatibility

| Package A | Compatible With | Notes |
|-----------|-----------------|-------|
| tonic 0.14 | tonic-web 0.14, tonic-build 0.14, prost 0.14 | All tonic ecosystem crates must use matching major version (0.14.x). Mixing 0.12 and 0.14 will cause compile errors. |
| tonic 0.14 | tokio 1.x | tonic is built on tokio. Any 1.x version works. |
| tonic 0.14 | tower-http 0.6 | Both use the hyper 1.0 / http 1.0 ecosystem. |
| hidapi 2.6 | Fedora kernel 6.19+ | hidraw backend works with all modern kernels. No compatibility issues. |
| tokio-udev 0.10 | tokio 1.x, systemd/libudev | Requires libudev-dev/systemd-devel system package. |
| Rust edition 2024 | All listed crates | All crates listed here compile under edition 2024. |

## Workspace Structure

The project should use a Cargo workspace with three crates, mirroring the reference project's proven architecture:

```
monsgeek-firmware-driver/
  Cargo.toml              # workspace root
  monsgeek-driver/        # binary crate (CLI, gRPC server, main)
    Cargo.toml
    src/main.rs
    proto/driver.proto    # gRPC service definition (from reference)
    build.rs              # tonic-build proto compilation
  monsgeek-transport/     # library crate (HID communication, protocol)
    Cargo.toml
    src/lib.rs
  monsgeek-protocol/      # library crate (FEA command definitions, byte layouts)
    Cargo.toml
    src/lib.rs
```

This separation provides:
- **monsgeek-protocol**: Pure data types and protocol constants. No I/O. Testable in isolation.
- **monsgeek-transport**: HID device access, hotplug, send/receive. Depends on monsgeek-protocol.
- **monsgeek-driver**: CLI, gRPC server, glue. Depends on both.

## Sources

- [docs.rs/crate/tonic/latest](https://docs.rs/crate/tonic/latest) -- tonic 0.14.5, verified 2026-03-19
- [docs.rs/crate/tonic-web/latest](https://docs.rs/crate/tonic-web/latest) -- tonic-web 0.14.5, verified 2026-03-19
- [docs.rs/crate/tonic-build/latest](https://docs.rs/crate/tonic-build/latest) -- tonic-build 0.14.5, verified 2026-03-19
- [docs.rs/crate/prost/latest](https://docs.rs/crate/prost/latest) -- prost 0.14.3, verified 2026-03-19
- [docs.rs/crate/tokio/latest](https://docs.rs/crate/tokio/latest) -- tokio 1.50.0, verified 2026-03-19
- [docs.rs/crate/hidapi/latest](https://docs.rs/crate/hidapi/latest) -- hidapi 2.6.5, verified 2026-03-19
- [docs.rs/crate/tokio-udev/latest](https://docs.rs/crate/tokio-udev/latest) -- tokio-udev 0.10.0, verified 2026-03-19
- [docs.rs/crate/tower-http/latest](https://docs.rs/crate/tower-http/latest) -- tower-http 0.6.8, verified 2026-03-19
- [docs.rs/crate/clap/latest](https://docs.rs/crate/clap/latest) -- clap 4.6.0, verified 2026-03-19
- [docs.rs/crate/thiserror/latest](https://docs.rs/crate/thiserror/latest) -- thiserror 2.0.18, verified 2026-03-19
- [docs.rs/crate/anyhow/latest](https://docs.rs/crate/anyhow/latest) -- anyhow 1.0.102, verified 2026-03-19
- [docs.rs/crate/zerocopy/latest](https://docs.rs/crate/zerocopy/latest) -- zerocopy 0.8.30, verified 2026-03-19
- [docs.rs/crate/redb/latest](https://docs.rs/crate/redb/latest) -- redb 3.1.1, verified 2026-03-19
- [docs.rs/crate/tracing-subscriber/latest](https://docs.rs/crate/tracing-subscriber/latest) -- tracing-subscriber 0.3.23, verified 2026-03-19
- [releases.rs](https://releases.rs) -- Rust 1.94.0 stable, verified 2026-03-19
- [blog.rust-lang.org/2025/02/20/Rust-1.85.0](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/) -- Rust 2024 edition stabilized
- [github.com/hyperium/tonic/blob/master/CHANGELOG.md](https://github.com/hyperium/tonic/blob/master/CHANGELOG.md) -- tonic breaking changes history
- Reference project: `references/monsgeek-akko-linux/iot_driver_linux/Cargo.toml` -- proven dependency selection

---
*Stack research for: Linux HID keyboard driver with gRPC-Web bridge*
*Researched: 2026-03-19*
