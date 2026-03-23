# Architecture Research

**Domain:** Linux FEA keyboard framework and configurator bridge  
**Researched:** 2026-03-19  
**Corrected:** 2026-03-23  
**Confidence:** HIGH for the wired M5W transport path

## System Overview

```text
MonsGeek Configurator / CLI
        |
        v
gRPC-Web Bridge / Command Frontend
        |
        v
Device Manager
  - registry/profile lookup
  - runtime enumeration
  - udev hot-plug
        |
        v
Transport Layer
  - transport thread
  - flow control
  - firmware-ID-aware probing
        |
        v
UsbSession
  - rusb control transfers on IF2
  - reset/reopen recovery
  - explicit interface ownership
        |
        v
Keyboard hardware
```

## Component Responsibilities

| Component | Responsibility |
|-----------|----------------|
| Bridge / CLI frontend | accept user or browser requests and route them to a selected device |
| Device manager | discover runtime transports, resolve them to registry/profile entries, surface hot-plug events |
| Registry / profile layer | define device metadata, layouts, and capabilities |
| Transport thread | serialize HID I/O and enforce timing rules |
| Flow-control layer | handle echo matching, retries, and recovery |
| `UsbSession` | own the USB handle, claim/release interfaces, perform raw control transfers |
| Optional input path | translate `IF0` boot reports only when userspace-input mode is intentional |

## Architecture Decisions

### 1. `rusb` over `hidapi`

For MonsGeek wired transport on Linux, the reliable low-level model is raw USB HID control transfers via `rusb`.

### 2. Firmware device ID as canonical identity

USB PID is transport identity. Canonical model identity is the device ID returned by `GET_USB_VERSION`.

### 3. `udev` hot-plug

In this environment, `udev` is the practical event source for arrival/removal. Design the bridge/device manager around that reality.

### 4. Explicit transport ownership modes

The framework should make a clean distinction between:

- control-only mode: preserve kernel typing, own `IF2`
- userspace-input mode: intentionally own `IF0`

### 5. Data-driven profile registry

The registry/profile layer should remain the place where model-specific facts live. Transport code should not accumulate per-device magic constants.

## Recommended Repo Shape

```text
monsgeek-protocol/
  - command tables
  - checksum logic
  - device/profile registry

monsgeek-transport/
  - usb.rs
  - flow_control.rs
  - discovery.rs
  - thread.rs
  - input.rs

bridge / driver layer
  - gRPC service
  - device manager
  - optional CLI
```

## Data Flow

### Raw command path

1. caller selects device profile / transport
2. transport thread serializes request
3. `UsbSession` sends HID class `SET_REPORT` / `GET_REPORT` on `IF2`
4. flow-control layer validates echo and timing
5. caller receives raw or typed response

### Discovery path

1. enumerate runtime USB devices
2. filter to likely supported candidates
3. probe `GET_USB_VERSION` where needed
4. resolve runtime transport to registry/profile entry
5. expose runtime bus/address plus canonical firmware device ID

## Primary Risks

- incorrect interface ownership leaving the keyboard unable to type
- PID-only coupling preventing broader framework support
- bridge code bypassing transport/thread safety rules
- unsupported device claims without validated profiles

## Current Architectural Gap

The main remaining gap before Phase 3 is not raw transport viability. It is the long-lived ownership split between control-only mode and userspace-input mode.

---
*Architecture research corrected after hardware validation on 2026-03-23*
