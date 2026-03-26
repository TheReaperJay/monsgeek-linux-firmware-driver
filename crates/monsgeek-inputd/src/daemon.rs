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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use monsgeek_protocol::DeviceRegistry;
use monsgeek_transport::discovery::{self, DeviceInfo};
use monsgeek_transport::error::TransportError;
use monsgeek_transport::input::InputProcessor;
use monsgeek_transport::usb::{SessionMode, UsbSession};

use crate::uinput_device::{create_uinput_device, emit_actions};

/// Configuration for the daemon's runtime behavior.
pub struct DaemonConfig {
    /// Software debounce window in milliseconds.
    pub debounce_ms: u64,
    /// Optional bus:address filter for targeting a specific USB device.
    pub device_filter: Option<(u8, u8)>,
    /// Name for the uinput virtual device.
    pub device_name: String,
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

    let registry = load_registry()?;

    // Discover the target device once to get its VID/PID for session opening
    // and reconnect monitoring.
    let device = find_target_device(&registry, config.device_filter)?;
    let vid = device.vid;
    let pid = device.pid;

    log::info!(
        "Target device: {} (ID {}) VID 0x{:04X} PID 0x{:04X}",
        device.display_name,
        device.device_id,
        vid,
        pid
    );

    while !shutdown.load(Ordering::Relaxed) {
        match try_connect_and_run(&config, &shutdown, vid, pid) {
            Ok(()) => break, // Clean shutdown via signal
            Err(DaemonError::Disconnected) => {
                sd_notify::notify(&[sd_notify::NotifyState::Status(
                    "Waiting for keyboard reconnect",
                )])
                .ok();
                log::info!("Waiting for keyboard reconnect via udev...");
                if let Err(e) = wait_for_device_udev(&shutdown, vid) {
                    log::error!("udev wait failed: {}", e);
                    return Err(e.to_string().into());
                }
                // Loop back to try_connect_and_run with a fresh session
            }
            Err(e) => return Err(e.to_string().into()),
        }
    }

    log::info!("Daemon shut down cleanly");
    Ok(())
}

/// Attempt to connect to the keyboard and run the input polling loop.
///
/// Returns `Ok(())` on clean signal-initiated shutdown.
/// Returns `Err(DaemonError::Disconnected)` when the keyboard is unplugged.
fn try_connect_and_run(
    config: &DaemonConfig,
    shutdown: &Arc<AtomicBool>,
    vid: u16,
    pid: u16,
) -> Result<(), DaemonError> {
    let session = UsbSession::open_with_mode(vid, pid, SessionMode::InputOnly)?;

    // Fresh InputProcessor per connect to clear stale debounce state.
    let mut processor = InputProcessor::new(config.debounce_ms);

    let mut uinput_dev = create_uinput_device(&config.device_name)?;

    sd_notify::notify(&[sd_notify::NotifyState::Ready]).ok();
    sd_notify::notify(&[sd_notify::NotifyState::Status(&format!(
        "Connected: VID 0x{:04X} PID 0x{:04X}",
        vid, pid
    ))])
    .ok();
    log::info!("Connected and polling IF0 (debounce {}ms)", config.debounce_ms);

    let mut report = [0u8; 8];
    let mut last_watchdog = Instant::now();

    while !shutdown.load(Ordering::Relaxed) {
        // Periodic watchdog ping
        if last_watchdog.elapsed() >= WATCHDOG_INTERVAL {
            sd_notify::notify(&[sd_notify::NotifyState::Watchdog]).ok();
            last_watchdog = Instant::now();
        }

        match session.read_report_with_timeout(&mut report, Duration::from_millis(10)) {
            Ok(n) if n >= 8 => {
                let actions = processor.process_report(&report);
                if !actions.is_empty() {
                    if let Err(e) = emit_actions(&mut uinput_dev, &actions) {
                        log::error!("uinput emit failed: {}", e);
                        // uinput failure is fatal for input path
                        break;
                    }
                }
            }
            Ok(_) => {} // Timeout or short read, continue polling
            Err(TransportError::Disconnected) => {
                log::warn!("Keyboard disconnected");
                let release_actions = processor.release_all_keys();
                let _ = emit_actions(&mut uinput_dev, &release_actions);
                drop(uinput_dev);
                drop(session);
                return Err(DaemonError::Disconnected);
            }
            Err(e) => {
                log::debug!("IF0 poll error: {}", e);
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

    log::debug!("udev monitor started for VID 0x{}", vid_str);

    loop {
        if shutdown.load(Ordering::Relaxed) {
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
    // Try paths relative to the current working directory first, then try
    // relative to the executable location.
    let candidates = [
        Path::new(DEVICE_REGISTRY_PATH).to_path_buf(),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("../../").join(DEVICE_REGISTRY_PATH)))
            .unwrap_or_default(),
    ];

    for path in &candidates {
        if path.is_dir() {
            return DeviceRegistry::load_from_directory(path)
                .map_err(|e| format!("Failed to load device registry from {}: {}", path.display(), e).into());
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

/// Find the target device from the registry and connected USB devices.
fn find_target_device(
    registry: &DeviceRegistry,
    device_filter: Option<(u8, u8)>,
) -> Result<DeviceInfo, Box<dyn std::error::Error>> {
    let devices = discovery::enumerate_devices(registry)?;

    if devices.is_empty() {
        return Err("No MonsGeek keyboards found. Is the keyboard connected?".into());
    }

    if let Some((bus, addr)) = device_filter {
        for device in &devices {
            if device.bus == bus && device.address == addr {
                return Ok(device.clone());
            }
        }
        return Err(format!(
            "No MonsGeek keyboard found at bus {:03}:{:03}. Found: {}",
            bus,
            addr,
            devices
                .iter()
                .map(|d| format!("{:03}:{:03} ({})", d.bus, d.address, d.display_name))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .into());
    }

    if devices.len() > 1 {
        log::warn!(
            "Multiple MonsGeek keyboards found. Using first: {} at {:03}:{:03}. \
             Use --device BUS:ADDR to select a specific one.",
            devices[0].display_name,
            devices[0].bus,
            devices[0].address
        );
    }

    Ok(devices[0].clone())
}
