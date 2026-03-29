//! Daemon main loop: device discovery, IF0 polling, InputProcessor report
//! processing, uinput event emission, signal handling, and disconnect/reconnect
//! lifecycle with udev monitoring.
//!
//! This module does NOT use the transport thread / `TransportHandle` /
//! `CommandController` machinery. It directly uses `UsbSession` for IF0 reads
//! and `InputProcessor` for report processing, avoiding the command channel,
//! 100ms throttling, and IF2 vendor logic that are irrelevant to input
//! processing.

use std::fmt;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use monsgeek_protocol::DeviceRegistry;
use monsgeek_transport::discovery::{self, DeviceInfo};
use monsgeek_transport::error::TransportError;
use monsgeek_transport::input::InputProcessor;
use monsgeek_transport::usb::{SessionMode, UsbSession};

use monsgeek_inputd::uinput_device::{create_uinput_device, emit_actions};

/// Configuration for the daemon's runtime behavior.
pub struct DaemonConfig {
    /// Software debounce window in milliseconds.
    pub debounce_ms: u64,
    /// Optional bus:address filter for targeting a specific USB device.
    pub device_filter: Option<(u8, u8)>,
    /// Optional explicit name override for the uinput virtual device.
    pub device_name: Option<String>,
}

/// Errors specific to daemon lifecycle operations.
enum DaemonError {
    Disconnected,
    Transport(TransportError),
    Io(std::io::Error),
}

impl fmt::Display for DaemonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DaemonError::Disconnected => write!(f, "keyboard disconnected"),
            DaemonError::Transport(e) => write!(f, "transport error: {}", e),
            DaemonError::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl From<TransportError> for DaemonError {
    fn from(e: TransportError) -> Self {
        DaemonError::Transport(e)
    }
}

impl From<std::io::Error> for DaemonError {
    fn from(e: std::io::Error) -> Self {
        DaemonError::Io(e)
    }
}

/// Interval between sd_notify watchdog pings in the poll loop.
const WATCHDOG_INTERVAL: Duration = Duration::from_secs(5);

/// Number of consecutive read errors before treating as disconnect.
/// On unplug, some kernels return Pipe or Other instead of NoDevice/Io.
/// These fall outside the explicit Disconnected mapping in read_report_with_timeout.
/// Consecutive errors with no successful reads indicate the device is gone.
const DISCONNECT_ERROR_THRESHOLD: u32 = 10;

/// Path to the device registry JSON files relative to the binary's compile-time
/// location. At runtime, the daemon resolves this from the cargo workspace root.
const DEVICE_REGISTRY_PATH: &str = "crates/monsgeek-protocol/devices";

/// Run the daemon main loop.
///
/// Discovers the target keyboard, claims IF0/IF1 via `SessionMode::InputOnly`,
/// polls boot protocol HID reports, processes them through `InputProcessor`,
/// and emits clean key events via uinput. Handles SIGTERM/SIGINT for clean
/// shutdown and reconnects automatically via udev on keyboard disconnect.
pub fn run_daemon(config: DaemonConfig) -> Result<(), Box<dyn std::error::Error>> {
    let shutdown = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&shutdown))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&shutdown))?;

    log::info!("Discovery mode: descriptor-only (IF2 probe disabled in inputd)");

    let registry = load_registry()?;

    // Prefer reconnecting the last known-good location first, but keep
    // fallback candidates so the daemon can recover ownership automatically.
    let mut preferred_location: Option<(u8, u8)> = None;
    let mut reconnect_vid: Option<u16> = None;

    while !shutdown.load(Ordering::Relaxed) {
        let candidates =
            match find_target_devices(&registry, config.device_filter, preferred_location) {
                Ok(candidates) => candidates,
                Err(err) => {
                    log::warn!("No target keyboard available yet: {}", err);
                    sd_notify::notify(&[sd_notify::NotifyState::Status(
                        "Waiting for MonsGeek keyboard",
                    )])
                    .ok();

                    if let Some(vid) = reconnect_vid {
                        if let Err(wait_err) =
                            wait_for_device_udev(&shutdown, vid, Duration::from_secs(3))
                        {
                            log::error!("udev wait failed: {}", wait_err);
                        }
                    } else {
                        std::thread::sleep(Duration::from_secs(1));
                    }
                    continue;
                }
            };

        let mut handled_disconnect = false;

        for device in candidates {
            reconnect_vid = Some(device.vid);

            log::info!(
                "Target candidate: {} (ID {}) at {:03}:{:03}",
                device.display_name,
                device.device_id,
                device.bus,
                device.address
            );

            let uinput_name = config
                .device_name
                .clone()
                .unwrap_or_else(|| format!("{} (monsgeek-inputd)", device.display_name));

            match try_connect_and_run(&config, &shutdown, &device, &uinput_name) {
                Ok(()) => return Ok(()), // Clean shutdown via signal
                Err(e) => {
                    log::warn!(
                        "Candidate {:03}:{:03} failed: {}",
                        device.bus,
                        device.address,
                        e
                    );

                    if matches!(e, DaemonError::Disconnected) {
                        preferred_location = Some((device.bus, device.address));
                        handled_disconnect = true;
                        // Device was unplugged — wait for udev add event
                        sd_notify::notify(&[sd_notify::NotifyState::Status(
                            "Waiting for keyboard reconnect",
                        )])
                        .ok();
                        log::info!("Waiting for keyboard reconnect via udev...");
                        if let Err(e) = wait_for_device_udev(
                            &shutdown,
                            reconnect_vid.unwrap_or(0),
                            Duration::from_secs(5),
                        ) {
                            log::error!("udev wait failed: {}", e);
                            return Err(e.to_string().into());
                        }

                        // Settle time: the firmware needs time after re-enumeration
                        // before it accepts control requests (SET_PROTOCOL, etc).
                        if !shutdown.load(Ordering::Relaxed) {
                            log::info!("Waiting 2s for device to settle...");
                            std::thread::sleep(Duration::from_secs(2));
                        }
                        break;
                    }
                }
            }
        }

        if !handled_disconnect && !shutdown.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(300));
        }
    }

    log::info!("Daemon shut down cleanly");
    Ok(())
}

/// Resolve target device candidates from the registry and connected USB devices.
fn find_target_devices(
    registry: &DeviceRegistry,
    device_filter: Option<(u8, u8)>,
    preferred_location: Option<(u8, u8)>,
) -> Result<Vec<DeviceInfo>, String> {
    // Input daemon must stay on descriptor-only discovery and avoid IF2
    // probing (`GET_USB_VERSION`) to prevent control-path contention.
    let devices = discovery::find_devices_no_probe(registry).map_err(|e| e.to_string())?;
    select_target_devices(registry, devices, device_filter, preferred_location)
}

fn select_target_devices(
    registry: &DeviceRegistry,
    mut devices: Vec<DeviceInfo>,
    device_filter: Option<(u8, u8)>,
    preferred_location: Option<(u8, u8)>,
) -> Result<Vec<DeviceInfo>, String> {
    if devices.is_empty() {
        return Err("No MonsGeek keyboards found. Is the keyboard connected?".to_string());
    }

    // Keep auto-selection deterministic across runs.
    devices.sort_by_key(|d| (d.bus, d.address, d.vid, d.pid));

    if let Some((bus, addr)) = device_filter {
        let mut filtered: Vec<DeviceInfo> = devices
            .into_iter()
            .filter(|d| d.bus == bus && d.address == addr)
            .collect();
        if filtered.is_empty() {
            return Err(format!(
                "No MonsGeek keyboard found at bus {:03}:{:03}.",
                bus, addr
            ));
        }
        filtered.sort_by_key(|d| (d.bus, d.address, d.vid, d.pid));
        return Ok(filtered);
    }

    // Prefer canonical registry PIDs over runtime alias PIDs for first choice.
    // Fallback candidates remain in the list for automatic recovery.
    devices.sort_by_key(|d| !registry.find_by_vid_pid(d.vid, d.pid).is_empty());
    devices.reverse();

    if let Some((bus, address)) = preferred_location {
        devices.sort_by_key(|d| d.bus != bus || d.address != address);
    }

    if devices.len() > 1 {
        let preferred = &devices[0];
        log::warn!(
            "Multiple MonsGeek keyboards found. Trying {} at {:03}:{:03} first \
             and auto-falling back if needed. Use --device BUS:ADDR to pin one.",
            preferred.display_name,
            preferred.bus,
            preferred.address
        );
    }

    Ok(devices)
}

/// Attempt to connect to the keyboard and run the input polling loop.
///
/// Returns `Ok(())` on clean signal-initiated shutdown.
/// Returns `Err(DaemonError::Disconnected)` when the keyboard is unplugged.
fn try_connect_and_run(
    config: &DaemonConfig,
    shutdown: &Arc<AtomicBool>,
    device: &DeviceInfo,
    uinput_device_name: &str,
) -> Result<(), DaemonError> {
    let session =
        UsbSession::open_at_with_mode(device.bus, device.address, SessionMode::InputOnly)?;

    // Fresh InputProcessor per connect to clear stale debounce state.
    let mut processor = InputProcessor::new(config.debounce_ms);

    let mut uinput_dev = create_uinput_device(uinput_device_name)?;

    sd_notify::notify(&[sd_notify::NotifyState::Ready]).ok();
    sd_notify::notify(&[sd_notify::NotifyState::Status(&format!(
        "Connected: VID 0x{:04X} PID 0x{:04X}",
        device.vid, device.pid
    ))])
    .ok();
    log::info!(
        "Connected and polling IF0 (debounce {}ms)",
        config.debounce_ms
    );

    let mut report = [0u8; 8];
    let mut last_watchdog = Instant::now();
    let mut consecutive_errors: u32 = 0;

    while !shutdown.load(Ordering::Relaxed) {
        // Periodic watchdog ping
        if last_watchdog.elapsed() >= WATCHDOG_INTERVAL {
            sd_notify::notify(&[sd_notify::NotifyState::Watchdog]).ok();
            last_watchdog = Instant::now();
        }

        match session.read_report_with_timeout(&mut report, Duration::from_millis(10)) {
            Ok(n) if n >= 8 => {
                consecutive_errors = 0;
                let actions = processor.process_report(&report);
                if !actions.is_empty() {
                    if let Err(e) = emit_actions(&mut uinput_dev, &actions) {
                        log::error!("uinput emit failed: {}", e);
                        // uinput failure is fatal for input path
                        break;
                    }
                }
            }
            Ok(_) => {
                // Timeout or short read — endpoint is alive, reset error counter
                consecutive_errors = 0;
            }
            Err(TransportError::Disconnected) => {
                log::warn!("Keyboard disconnected");
                let release_actions = processor.release_all_keys();
                let _ = emit_actions(&mut uinput_dev, &release_actions);
                drop(uinput_dev);
                drop(session);
                return Err(DaemonError::Disconnected);
            }
            Err(e) => {
                consecutive_errors += 1;
                if consecutive_errors >= DISCONNECT_ERROR_THRESHOLD {
                    log::warn!(
                        "Keyboard disconnected ({} consecutive errors, last: {})",
                        consecutive_errors,
                        e
                    );
                    let release_actions = processor.release_all_keys();
                    let _ = emit_actions(&mut uinput_dev, &release_actions);
                    drop(uinput_dev);
                    drop(session);
                    return Err(DaemonError::Disconnected);
                }
                log::warn!("IF0 poll error ({}x): {}", consecutive_errors, e);
            }
        }
    }

    // Clean shutdown path: release all held keys before tearing down
    let release_actions = processor.release_all_keys();
    let _ = emit_actions(&mut uinput_dev, &release_actions);
    // uinput_dev and session are dropped here, releasing resources
    Ok(())
}

/// Block until a USB device with the given VID appears via udev, or shutdown
/// is signaled.
fn wait_for_device_udev(
    shutdown: &Arc<AtomicBool>,
    vid: u16,
    timeout: Duration,
) -> Result<(), DaemonError> {
    let builder = udev::MonitorBuilder::new()
        .map_err(|e| DaemonError::Io(std::io::Error::other(format!("udev monitor: {}", e))))?;
    let builder = builder
        .match_subsystem("usb")
        .map_err(|e| DaemonError::Io(std::io::Error::other(format!("udev filter: {}", e))))?;
    let socket = builder
        .listen()
        .map_err(|e| DaemonError::Io(std::io::Error::other(format!("udev listen: {}", e))))?;

    let fd = socket.as_raw_fd();
    let vid_str = format!("{:04x}", vid);
    let started = Instant::now();

    log::debug!("udev monitor started for VID 0x{}", vid_str);

    loop {
        if shutdown.load(Ordering::Relaxed) {
            return Ok(());
        }

        if started.elapsed() >= timeout {
            log::debug!(
                "udev wait timed out after {}s for VID 0x{}",
                timeout.as_secs(),
                vid_str
            );
            return Ok(());
        }

        let mut fds = [libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        }];

        let ret = unsafe { libc::poll(fds.as_mut_ptr(), 1, 1000) };
        if ret <= 0 {
            continue;
        }

        let Some(event) = socket.iter().next() else {
            continue;
        };

        let matches_vid = event
            .property_value("PRODUCT")
            .and_then(|v| v.to_str())
            .map(|v| v.starts_with(&vid_str))
            .unwrap_or(false);

        if !matches_vid {
            continue;
        }

        if matches!(event.event_type(), udev::EventType::Add) {
            log::info!("udev: keyboard reconnected (VID 0x{})", vid_str);
            // Brief settle time for the USB stack to finish enumeration
            std::thread::sleep(Duration::from_millis(500));
            return Ok(());
        }
    }
}

/// Load the device registry from the standard path.
fn load_registry() -> Result<DeviceRegistry, Box<dyn std::error::Error>> {
    // Resolve registry locations in priority order:
    // 1) explicit override (`MONSGEEK_DEVICE_REGISTRY_DIR`)
    // 2) packaged install path
    // 3) cwd-relative workspace path
    // 4) executable-relative fallback
    let mut candidates = Vec::new();

    if let Ok(path) = std::env::var("MONSGEEK_DEVICE_REGISTRY_DIR") {
        candidates.push(Path::new(&path).to_path_buf());
    }

    candidates.push(Path::new("/usr/share/monsgeek/protocol/devices").to_path_buf());
    candidates.push(Path::new(DEVICE_REGISTRY_PATH).to_path_buf());
    candidates.push(
        std::env::current_exe()
            .ok()
            .and_then(|p| {
                p.parent()
                    .map(|p| p.join("../../").join(DEVICE_REGISTRY_PATH))
            })
            .unwrap_or_default(),
    );

    for path in &candidates {
        if path.is_dir() {
            return DeviceRegistry::load_from_directory(path).map_err(|e| {
                format!(
                    "Failed to load device registry from {}: {}",
                    path.display(),
                    e
                )
                .into()
            });
        }
    }

    Err(format!(
        "Device registry not found at any of: {}",
        candidates
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> DeviceRegistry {
        let devices_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../monsgeek-protocol")
            .join("devices");
        DeviceRegistry::load_from_directory(&devices_dir).expect("registry load failed")
    }

    fn fake_device(device_id: i32, pid: u16, bus: u8, address: u8) -> DeviceInfo {
        DeviceInfo {
            vid: 0x3151,
            pid,
            device_id,
            display_name: format!("Device-{device_id}"),
            name: format!("dev-{device_id}"),
            bus,
            address,
        }
    }

    #[test]
    fn select_targets_prefers_canonical_pid_for_runtime_alias() {
        let registry = registry();
        let candidates = vec![
            fake_device(1308, 0x4011, 1, 2), // runtime alias PID
            fake_device(1308, 0x4015, 1, 3), // canonical PID
        ];

        let selected = select_target_devices(&registry, candidates, None, None)
            .expect("selection failed");
        assert_eq!(selected[0].pid, 0x4015);
        assert_eq!(selected[0].bus, 1);
        assert_eq!(selected[0].address, 3);
    }

    #[test]
    fn select_targets_applies_preferred_location_ordering() {
        let registry = registry();
        let candidates = vec![
            fake_device(1308, 0x4015, 1, 2),
            fake_device(2299, 0x5002, 1, 3),
        ];

        let selected = select_target_devices(&registry, candidates, None, Some((1, 3)))
            .expect("selection failed");
        assert_eq!(selected[0].bus, 1);
        assert_eq!(selected[0].address, 3);
    }

    #[test]
    fn select_targets_filters_by_bus_address() {
        let registry = registry();
        let candidates = vec![
            fake_device(1308, 0x4015, 1, 2),
            fake_device(2299, 0x5002, 1, 3),
        ];

        let selected = select_target_devices(&registry, candidates, Some((1, 3)), None)
            .expect("selection failed");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].bus, 1);
        assert_eq!(selected[0].address, 3);
    }
}
