# Phase 3: gRPC-Web Bridge - Research

**Researched:** 2026-03-23
**Domain:** Rust gRPC-Web localhost bridge for MonsGeek web configurator compatibility
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
### RPC compatibility surface
- **D-01:** Register the full `DriverGrpc` service surface from the vendor-compatible proto, not a partial service.
- **D-02:** Implement the Phase 3 MVP RPCs for real and keep out-of-scope RPCs present but returning deliberate compatibility-safe "not supported yet" responses rather than omitting endpoints.

### Device identity exposed to the web app
- **D-03:** Expose a bridge-owned synthetic compatibility path, not raw bus/address or raw hidraw identity.
- **D-04:** Keep firmware device ID as canonical model identity and treat the bridge path as a session/runtime locator only.
- **D-05:** Allow reconnects to receive a new synthetic path when runtime topology changes; do not promise stable path reuse across reconnects.
- **D-06:** Include a uniqueness suffix in the compatibility path when needed so identical devices can coexist without collisions.

### Device-list stream semantics
- **D-07:** `watchDevList` sends one `Init` snapshot immediately on subscription.
- **D-08:** After `Init`, Phase 3 emits only `Add` and `Remove` events for device-list changes.
- **D-09:** Do not implement `Change` semantics in Phase 3 unless planning/research proves the web app actually requires them.

### Local storage compatibility
- **D-10:** Phase 3 DB RPCs use an in-memory compatibility store for MVP behavior.
- **D-11:** Persistence across bridge restarts is explicitly out of scope for this phase unless later research proves the web app requires it.

### Raw vs higher-level message handling
- **D-12:** Implement both `sendRawFeature` / `readRawFeature` and `sendMsg` / `readMsg` as first-class RPCs.
- **D-13:** Route both RPC families through one shared transport core so behavior, throttling, and device ownership remain consistent.
- **D-14:** It is acceptable to extend lower layers in this repo if the current transport API is too narrow for bridge compatibility; Phase 3 must not distort the bridge contract just to avoid transport-layer edits.

### Ownership and runtime behavior
- **D-15:** Phase 3 uses control-only transport by default so normal kernel typing is preserved while the bridge is running.
- **D-16:** Userspace-input mode is not the default bridge behavior and should not be pulled into Phase 3 unless required for compatibility.

### Claude's Discretion
- Exact internal server/module layout inside the bridge crate
- Exact error text for unsupported RPCs, as long as the RPCs remain present and compatibility-safe
- Exact in-memory DB container type and concurrency primitives

### Deferred Ideas (OUT OF SCOPE)
- File-backed DB persistence across bridge restarts
- `DeviceListChangeType::Change` support for richer online/battery/status updates
- OTA, weather, microphone, LED streaming, and other non-MVP RPC families beyond compatibility-safe stubs
- Stable synthetic device paths across reconnects for identical devices
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| GRPC-01 | localhost:3814 + gRPC-Web from browser | Server runtime pattern proven in reference main (`accept_http1(true)`, `tonic_web::enable`, CORS) |
| GRPC-02 | `sendRawFeature` | Raw send implementation pattern proven in reference `grpc.rs` |
| GRPC-03 | `readRawFeature` | Split read implementation pattern proven in reference `grpc.rs` |
| GRPC-04 | `watchDevList` stream | `Init` + `Add/Remove` semantics proven in reference `grpc.rs` |
| GRPC-05 | `getVersion` | Response schema and field names proven in proto + reference impl |
| GRPC-06 | `insertDb` / `getItemFromDb` | In-memory store pattern proven in reference `grpc.rs` |
| GRPC-07 | CORS headers | `tower_http::cors::CorsLayer` usage proven in reference main |
| GRPC-08 | Exact proto contract quirks | Full 25-RPC service + field-name quirks proven in proto file |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

No `./CLAUDE.md` exists in this repo (verified 2026-03-23), so there are no additional project-specific directives to enforce.

## Summary

Phase 3 must implement a browser-facing gRPC-Web bridge that is contract-compatible with `DriverGrpc` and runs on `127.0.0.1:3814`. The compatibility target is clear: full proto registration (25 RPCs), real behavior for the MVP surface, and explicit compatibility-safe stubs for the rest.

The main technical risk is not gRPC itself; it is bridging the webapp’s split `send*` / `read*` semantics onto the current `monsgeek-transport` API, which currently exposes query/send command APIs but not per-device split read queues by runtime device path. Planning should explicitly include transport-layer extension work (allowed by D-14), not bridge-level hacks.

The second risk is device identity/event semantics. The web app expects path-like runtime locators and `watchDevList` semantics (`Init` then `Add/Remove`). We already have enough primitives (firmware ID probing + hotplug events), but we need a bridge-owned path registry and event mapping strategy that does not leak raw bus/address identity.

**Primary recommendation:** Build Phase 3 around a `BridgeDeviceSession` manager that owns synthetic path mapping + split send/read queues, while registering the full generated `DriverGrpc` server and stubbing non-MVP methods safely.

## Research Questions (Planning-Critical)

1. **Exact MVP RPC set and full-service stub policy for GRPC-01..08**
   - **PROVEN:** `DriverGrpc` has 25 RPC methods and GRPC-08 requires full contract match, including quirks like `watchVender`, `devList`, `err_str`, `baseVersion`, `timeStamp`, and `checkSumType` naming (`driver.proto`:6-55, 169-181, 192, 212).  
   - **PROVEN:** Reference server registers full service via `DriverGrpcServer::new(service)` and implements all methods (`src/main.rs`:424, `src/grpc.rs`:904+).  
   - **PROVEN:** Required MVP real implementations for this phase: `watchDevList`, `sendRawFeature`, `readRawFeature`, `getVersion`, `insertDb`, `getItemFromDb` (`REQUIREMENTS.md`:22-29; `grpc.rs`:909, 949, 972, 1029, 1047, 1122).  
   - **PROVEN:** Context locks `sendMsg`/`readMsg` as first-class in Phase 3 too (D-12), so MVP execution set should include them as real, not stubs (`03-CONTEXT.md`:37-38; `grpc.rs`:991, 1013).  
   - **Policy:** **PROVEN + LOCKED** => register all 25 RPCs; implement MVP-real methods above; return compatibility-safe "not supported yet" for out-of-scope methods (D-02).

2. **Precise gap between current `monsgeek-transport` API and split send/read bridge semantics**
   - **PROVEN:** Current API is command-centric (`send_query`, `send_fire_and_forget`) and device-definition-centric `connect(...)`, not runtime `devicePath` keyed session management (`lib.rs`:109, 144, 194).  
   - **PROVEN:** No bridge-facing `read_response(device_path)` primitive exists in current crate; response is tied to query call path.  
   - **PROVEN:** Current hotplug events expose `DeviceArrived{vid,pid,bus,address}` / `DeviceLeft{bus,address}` only (`thread.rs`:43-57), not synthetic bridge path IDs.  
   - **PROVEN:** Reference bridge requires split semantics: send now, read later (`grpc.rs`:834-838, 877-878) and caches open transports by path (`grpc.rs`:736-783).  
   - **INFERRED:** We likely need one of:
     - transport extension to expose per-session split queue (`send_report_raw` + `read_report_raw`) and/or
     - bridge-local session abstraction over `UsbSession` while preserving transport safety features.  
     Reason: current API shape cannot directly represent webapp split contract without extra buffering/session state.

3. **Server runtime requirements for browser gRPC-Web on localhost:3814**
   - **PROVEN:** Bind `127.0.0.1:3814` (`main.rs`:390).  
   - **PROVEN:** Must enable gRPC-Web wrapping (`tonic_web::enable(...)`) (`main.rs`:424).  
   - **PROVEN:** Must accept HTTP/1.1 (`accept_http1(true)`) because browser gRPC-Web uses HTTP/1.1 framing (`main.rs`:429).  
   - **PROVEN:** Must apply permissive CORS for web app access (`CorsLayer::new().allow_origin(Any).allow_headers(Any).allow_methods(Any)`) (`main.rs`:417-420).  
   - **INFERRED:** We should mirror permissive CORS/exposed headers for MVP first, then tighten later, because compatibility is primary and origin behavior in vendor app can vary.

4. **Device path and `watchDevList` semantics (Init/Add/Remove) and mapping to our transport events**
   - **PROVEN:** Reference sends one `Init` snapshot on subscription, then streams broadcasted updates (`grpc.rs`:909-937).  
   - **PROVEN:** Reference emits only `Add` and `Remove`; no `Change` is used (`grpc.rs`:488, 554, 925).  
   - **PROVEN:** Synthetic path format includes uniqueness suffix (`@<device_path>`) and comments explain parser compatibility (`grpc.rs`:562-565).  
   - **PROVEN:** Current transport can identify device by firmware ID (probe) and provides bus/address in events (`discovery.rs`:17-31, 109, 166; `thread.rs`:45-55).  
   - **INFERRED:** Recommended mapping in this repo:
     - boot: enumerate/probe -> emit `Init` with synthetic paths
     - on `DeviceArrived`: rescan/probe; allocate synthetic path; emit `Add`
     - on `DeviceLeft`: resolve prior synthetic path via bus/address cache; emit `Remove`
     - skip `Change` in Phase 3 unless hard evidence appears  
     Reason: matches D-07..D-09 and reference semantics exactly.

5. **Practical plan decomposition into 03-01 / 03-02 / 03-03**
   - **INFERRED (high-confidence):**
     - **03-01 (Wave 1 foundation):** Proto generation + full service skeleton + localhost gRPC-Web+CORS runtime + compatibility stubs.  
       Dependency: none beyond existing workspace.
     - **03-02 (Wave 2 core behavior):** Device session manager + synthetic path registry + split send/read for raw+msg + `watchDevList` Init/Add/Remove + getVersion + in-memory DB (`insertDb`/`getItemFromDb`).  
       Dependency: 03-01 complete; may require `monsgeek-transport` extension.
     - **03-03 (Wave 3 hardening):** Browser handshake verification, contract edge-case fixes (field quirks/status codes), unsupported-RPC behavior audits, and Nyquist test coverage for GRPC-01..08.

6. **Planning traps and anti-patterns to explicitly avoid**
   - **PROVEN trap:** Registering only MVP RPCs violates D-01 and GRPC-08 full-contract requirement (`03-CONTEXT.md`:18; `REQUIREMENTS.md`:29).  
   - **PROVEN trap:** Treating bus/address as stable app identity conflicts with D-03..D-06 and runtime reality (`03-CONTEXT.md`:22-25; `discovery.rs`:29-31).  
   - **PROVEN trap:** Implementing only raw RPCs and ignoring `sendMsg`/`readMsg` conflicts with D-12 (`03-CONTEXT.md`:37).  
   - **INFERRED trap:** Faking split semantics by doing immediate query inside `sendMsg`; this can break webapp flow control expectations (reference explicitly separates send and read, `grpc.rs`:834-878).

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tonic` | `0.12` | gRPC server traits and transport | Same stack as proven Linux compatibility reference |
| `tonic-web` | `0.12` | gRPC-Web translation for browsers | Required for browser clients |
| `prost` | `0.13` | Proto codegen runtime | Native pairing with tonic |
| `tower-http` (`cors`) | `0.6` | CORS middleware | Needed for browser-origin calls |
| `tokio` | `1.x` | Async runtime | Required by tonic server |
| `tokio-stream` (`sync`) | `0.1` | Broadcast stream adapters | Used for streaming watch RPCs |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tracing` + `tracing-subscriber` | `0.1` / `0.3` | Structured logs | Service startup + RPC diagnostics |
| `futures` | `0.3` | stream combinators | chaining init stream with broadcast stream |
| `pin-project-lite` | `0.2` | stream guard wrappers | guarded stream drop bookkeeping |
| `tokio-udev` | `0.9` | hotplug notifications | if bridge listens to hidraw add/remove directly |
| `tonic-build` | `0.12` | proto build step | compile `driver.proto` into service stubs |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `tonic`/`tonic-web` | custom HTTP bridge | Reinvents gRPC-Web framing and increases compatibility risk |
| direct bus/address identity | synthetic bridge path map | synthetic path is required by locked decisions and avoids runtime identity churn |

**Installation (expected Phase 3 additions):**
```bash
cargo add tonic@0.12 tonic-web@0.12 prost@0.13 tower-http@0.6 tokio@1 tokio-stream@0.1 futures@0.3 tracing@0.1 tracing-subscriber@0.3 pin-project-lite@0.2 tokio-udev@0.9
cargo add tonic-build@0.12 --build
```

**Version verification:** versions above are **PROVEN** from `references/monsgeek-akko-linux/iot_driver_linux/Cargo.toml` (local canonical compatibility implementation).

## Architecture Patterns

### Recommended Project Structure
```text
crates/monsgeek-driver/
├── build.rs                    # tonic-build compile_protos(driver.proto)
├── proto/driver.proto          # vendored-compatible contract
└── src/
    ├── main.rs                 # bind + tonic-web + CORS + server bootstrap
    ├── grpc/
    │   ├── mod.rs              # DriverService + impl DriverGrpc
    │   ├── device_registry.rs  # synthetic path/session map
    │   ├── db_store.rs         # in-memory DB compatibility store
    │   └── stubs.rs            # non-MVP compatibility-safe responses
    └── bridge_transport.rs     # adapter from bridge semantics to monsgeek-transport
```

### Pattern 1: Full-Service Registration + Controlled Stub Policy
**What:** Generate and register full `DriverGrpc`, but implement out-of-scope RPCs as explicit compatibility-safe stubs.
**When to use:** Always in Phase 3 due to D-01/D-02 and GRPC-08.
**Example:**
```rust
// Source: references/.../src/main.rs:424, references/.../proto/driver.proto:6-55
let grpc_service = tonic_web::enable(DriverGrpcServer::new(service));
Server::builder().accept_http1(true).add_service(grpc_service);
```

### Pattern 2: Split Send/Read Session Adapter
**What:** Keep per-device session state keyed by synthetic `devicePath`; `send*` writes immediately, `read*` reads later.
**When to use:** `sendRawFeature`/`readRawFeature` and `sendMsg`/`readMsg`.
**Example:**
```rust
// Source: references/.../src/grpc.rs:834-878
async fn send_command(path: &str, frame: &[u8], checksum: ChecksumType) -> Result<(), Status>;
async fn read_response(path: &str) -> Result<Vec<u8>, Status>;
```

### Pattern 3: Device Stream Contract (`Init` then delta events)
**What:** `watchDevList` emits one initial snapshot, then only `Add`/`Remove` deltas.
**When to use:** GRPC-04 compatibility.
**Example:**
```rust
// Source: references/.../src/grpc.rs:909-937, 488, 554
let initial = DeviceList { dev_list: scan_devices().await, r#type: Init as i32 };
let stream = once(initial).chain(broadcast_updates_only_add_remove);
```

### Anti-Patterns to Avoid
- **Partial service registration:** breaks GRPC-08 and browser compatibility expectations.
- **Identity tied to bus/address:** invalid after reconnect/topology changes.
- **Bypassing transport safety invariants:** must keep throttling/ownership behavior from Phase 2.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| gRPC-Web framing | custom HTTP parser | `tonic-web` | Browser compatibility and protocol correctness |
| Proto schema mapping | manual JSON-like structs | `prost`/`tonic-build` generated types | preserves field names/numbers/quirks |
| Hotplug semantics engine | ad-hoc polling loop only | existing `TransportEvent` + controlled rescan mapping | aligns with current transport model |
| DB persistence for MVP | file-backed DB now | in-memory store | D-10/D-11 explicitly scope persistence out |

**Key insight:** Most compatibility failures come from contract drift, not business logic bugs.

## Common Pitfalls

### Pitfall 1: Contract drift from proto quirks
**What goes wrong:** Renaming typo fields/methods (`watchVender`, `err_str`, camelCase fields) breaks web app decoding.
**Why it happens:** "cleanup" impulses during implementation.
**How to avoid:** Use generated proto types; do not rename wire-facing fields.
**Warning signs:** Browser connects but silently ignores payloads.

### Pitfall 2: Wrong RPC behavior on split send/read
**What goes wrong:** `sendMsg` blocks waiting for response or consumes response before `readMsg`.
**Why it happens:** Reusing query-style APIs directly.
**How to avoid:** Introduce split session state and separate send/read paths.
**Warning signs:** Intermittent empty reads or app retries looping.

### Pitfall 3: Incorrect device identity lifecycle
**What goes wrong:** Remove events cannot match previously announced device.
**Why it happens:** no stable session-level mapping from transport events to synthetic path.
**How to avoid:** Maintain bridge-owned path map and bus/address reverse index.
**Warning signs:** duplicate ghost devices in web UI.

### Pitfall 4: Browser transport misconfiguration
**What goes wrong:** browser preflight or gRPC-Web calls fail despite server running.
**Why it happens:** missing HTTP/1 acceptance or CORS misconfig.
**How to avoid:** enforce `accept_http1(true)`, `tonic_web::enable`, permissive CORS for MVP.
**Warning signs:** network console shows CORS/preflight failures.

## Code Examples

Verified patterns from local canonical reference:

### Browser-capable gRPC-Web server bootstrap
```rust
// Source: references/monsgeek-akko-linux/iot_driver_linux/src/main.rs:390,417-420,424,429
let addr = "127.0.0.1:3814".parse()?;
let cors = CorsLayer::new().allow_origin(Any).allow_headers(Any).allow_methods(Any);
let grpc_service = tonic_web::enable(DriverGrpcServer::new(service));
Server::builder().accept_http1(true).layer(cors).add_service(grpc_service).serve(addr).await?;
```

### `watchDevList` Init + stream updates
```rust
// Source: references/monsgeek-akko-linux/iot_driver_linux/src/grpc.rs:909-937
let initial_list = DeviceList {
    dev_list: self.scan_devices().await,
    r#type: DeviceListChangeType::Init as i32,
};
let initial_stream = futures::stream::iter(std::iter::once(Ok(initial_list)));
let broadcast_stream = BroadcastStream::new(self.device_tx.subscribe()).filter_map(...);
let combined = initial_stream.chain(broadcast_stream);
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| transport identity by static USB assumptions | runtime discovery + firmware device ID | Phase 2 closeout (2026-03-23) | bridge must treat path as runtime locator, not model identity |
| no explicit browser bridge contract in this repo | full proto-compatible gRPC-Web bridge target | Phase 3 context (2026-03-23) | planning must prioritize contract fidelity over redesign |

**Deprecated/outdated:**
- Partial or "just enough" gRPC method exposure for MVP: replaced by full-service registration with controlled stubs (D-01/D-02).

## Open Questions

1. **Exact `getVersion` content expected by current web app**
   - What we know: proto shape requires `baseVersion` + `timeStamp`; reference returns static values.
   - What's unclear: whether app validates exact format/value semantics.
   - Recommendation: start with deterministic static values and capture browser behavior in 03-03.

2. **Best location for split send/read adaptation**
   - What we know: current transport API lacks direct split contract.
   - What's unclear: cleanest boundary (extend `monsgeek-transport` vs bridge-local adapter over `UsbSession`).
   - Recommendation: decide in 03-01 design notes; prefer transport extension if it preserves thread safety and throttling centrally.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `cargo` | build/test | ✓ | 1.93.1 | — |
| `rustc` | compile | ✓ | 1.93.1 | — |
| `protoc` | proto codegen | ✓ | libprotoc 33.4 | — |
| `pkg-config` | native deps resolution | ✓ | 2.3.0 | — |
| `udevadm` | Linux device/hotplug diagnostics | ✓ | 258 | — |

**Missing dependencies with no fallback:**
- None.

**Missing dependencies with fallback:**
- None.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness (`cargo test`) |
| Config file | none |
| Quick run command | `cargo test -p monsgeek-driver -- --nocapture` |
| Full suite command | `cargo test --workspace --all-targets -- --nocapture` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| GRPC-01 | gRPC-Web server on localhost:3814 with HTTP/1 | integration | `cargo test -p monsgeek-driver grpc_server_starts_http1 -- --nocapture` | ❌ Wave 0 |
| GRPC-02 | `sendRawFeature` forwards raw frame | integration (mock transport) | `cargo test -p monsgeek-driver grpc_send_raw_feature_forwards -- --nocapture` | ❌ Wave 0 |
| GRPC-03 | `readRawFeature` returns pending device response | integration (mock transport) | `cargo test -p monsgeek-driver grpc_read_raw_feature_returns_data -- --nocapture` | ❌ Wave 0 |
| GRPC-04 | `watchDevList` emits Init then Add/Remove | integration stream | `cargo test -p monsgeek-driver grpc_watch_dev_list_init_add_remove -- --nocapture` | ❌ Wave 0 |
| GRPC-05 | `getVersion` returns proto-compatible payload | unit | `cargo test -p monsgeek-driver grpc_get_version_shape -- --nocapture` | ❌ Wave 0 |
| GRPC-06 | `insertDb` + `getItemFromDb` in-memory session storage | unit/integration | `cargo test -p monsgeek-driver grpc_db_insert_get_roundtrip -- --nocapture` | ❌ Wave 0 |
| GRPC-07 | CORS allows browser-origin gRPC-Web requests | integration HTTP | `cargo test -p monsgeek-driver grpc_cors_headers_present -- --nocapture` | ❌ Wave 0 |
| GRPC-08 | full proto service methods registered with compatibility stubs | compile + integration | `cargo test -p monsgeek-driver grpc_full_service_contract_present -- --nocapture` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p monsgeek-driver -- --nocapture`
- **Per wave merge:** `cargo test --workspace --all-targets -- --nocapture`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/monsgeek-driver/tests/grpc_contract_tests.rs` — covers GRPC-01..08 core compatibility.
- [ ] `crates/monsgeek-driver/tests/grpc_watch_stream_tests.rs` — stream behavior (`Init` + delta events).
- [ ] `crates/monsgeek-driver/tests/grpc_db_tests.rs` — DB roundtrip + key-space behavior.
- [ ] `crates/monsgeek-driver/tests/mock_transport.rs` — deterministic send/read behavior without hardware.
- [ ] `crates/monsgeek-driver/build.rs` + generated proto module wiring.

## Sources

### Primary (HIGH confidence)
- `.planning/phases/03-grpc-web-bridge/03-CONTEXT.md` — locked decisions D-01..D-16.
- `.planning/REQUIREMENTS.md` — GRPC-01..GRPC-08 requirements.
- `references/monsgeek-akko-linux/iot_driver_linux/proto/driver.proto` — authoritative service/method/field contract.
- `references/monsgeek-akko-linux/iot_driver_linux/src/main.rs` — proven browser-facing server runtime settings.
- `references/monsgeek-akko-linux/iot_driver_linux/src/grpc.rs` — proven compatibility behavior for device stream/send-read/DB/stubs.
- `references/monsgeek-akko-linux/iot_driver_linux/Cargo.toml` — practical stack versions.
- `crates/monsgeek-transport/src/lib.rs`, `thread.rs`, `usb.rs`, `discovery.rs` — current local API and event/identity capabilities.
- `crates/monsgeek-driver/src/main.rs`, `Cargo.toml` — current starting point and gap baseline.

### Secondary (MEDIUM confidence)
- `references/monsgeek-akko-linux/FEATURE_MAP.md` — implementation status claims; used only as supporting evidence.

### Tertiary (LOW confidence)
- None.

## Metadata

**Confidence breakdown:**
- Standard stack: **HIGH** - directly from working compatibility reference Cargo manifest.
- Architecture: **HIGH** - inferred from concrete reference implementation + locked decisions.
- Pitfalls: **HIGH** - directly tied to contract, runtime, and existing transport constraints.

**Research date:** 2026-03-23
**Valid until:** 2026-04-22

## RESEARCH COMPLETE
