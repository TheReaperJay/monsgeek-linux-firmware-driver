# Project Research Summary

**Project:** Linux FEA Keyboard Framework & Configurator Bridge  
**Domain:** Linux userspace transport and bridge for FEA-based keyboards  
**Researched:** 2026-03-19  
**Corrected:** 2026-03-23  
**Confidence:** HIGH after direct M5W host-side validation

## Executive Summary

This project is building a Linux userspace framework and compatibility bridge for FEA-based keyboards. The immediate MVP is not a custom GUI and not a generic “driver” in the abstract. The MVP is a transport plus gRPC-Web bridge that lets the existing MonsGeek configurator talk to Linux on `localhost:3814`.

The first fully validated target is the wired MonsGeek M5W. The framework should remain general enough to support additional MonsGeek and Akko devices that share the FEA protocol family, but support claims must follow real transport and profile validation per device.

The original planning assumptions were partially wrong. The corrected transport model is:

- use `rusb` for MonsGeek wired transport on Linux
- identify devices by firmware device ID from `GET_USB_VERSION`, not USB PID alone
- treat bus/address as runtime transport coordinates, never as stable identity
- use `udev` for hot-plug in this environment
- keep kernel typing intact unless userspace input mode is explicitly chosen

## Verified Findings

- Wired M5W USB identity: VID `0x3151`, PID `0x4015`
- M5W 2.4GHz dongle PID: `0x4011`
- `GET_USB_VERSION` returns firmware device ID `1308`
- that device ID is encoded as a 32-bit little-endian field
- the firmware requires a 100ms minimum command cadence
- reset-then-reopen is the practical recovery path from transient `PIPE` states
- `IF0` must be returned to the kernel after short-lived sessions

## Recommended Stack

### Core transport stack

- `rusb` for USB control transfers and interface ownership
- `crossbeam-channel` for the transport thread request queue
- `udev` for host-side hot-plug events
- `log` + `thiserror` for diagnostics and error handling

### Bridge stack

- `tokio`
- `tonic`
- `tonic-web`
- `tower-http` for CORS handling

### Data / registry stack

- `serde` / `serde_json`
- JSON device/profile registry loaded from the repo

## MVP Definition

The MVP is complete when:

- the wired M5W can be discovered dynamically on Linux
- the configurator bridge runs on `127.0.0.1:3814`
- the MonsGeek configurator can see the device and exchange raw HID commands
- transport ownership no longer accidentally breaks typing

That means the MVP depends on:

- stable Phase 2 transport
- Phase 3 gRPC-Web bridge

It does not require:

- custom GUI work
- firmware flashing
- broad support promises for every FEA-family device

## Key Risks

### Firmware timing

Commands sent too quickly can wedge the firmware. The transport thread must enforce the timing rule globally.

### Identity mistakes

Bad USB IDs and PID-only thinking already caused planning errors once. Identity must stay registry/profile-driven and firmware-ID-aware.

### Ownership mistakes

A transport session that detaches `IF0` and fails to restore it leaves the keyboard unable to type. Session cleanup is a product-quality issue, not a test-only nicety.

### Bridge contract fidelity

The web configurator expects the Windows proto contract exactly. Proto “cleanup” or field renaming risks silent incompatibility.

## Phase Guidance

- **Phase 1:** registry and protocol foundation
- **Phase 2:** safe wired transport, discovery, and real hardware validation
- **Phase 3:** gRPC-Web bridge and configurator compatibility
- **Phase 4+:** feature-family verification and tooling

The current project is at late Phase 2. Basic transport is proven; transport ownership still needs final cleanup before Phase 3 begins.

## Primary Sources

- `references/monsgeek-hid-driver/`
- `references/monsgeek-akko-linux/`
- live host-side M5W validation performed during Phase 2
- extracted Windows/Electron firmware/application bundle in `firmware/`

---
*Research summary corrected after hardware validation on 2026-03-23*
