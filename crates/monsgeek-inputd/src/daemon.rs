//! Multi-device input daemon.
//!
//! The daemon runs a supervisor loop that discovers all supported keyboards via
//! descriptor-only enumeration and spawns one input worker per device instance.
//! Each worker owns a dedicated IF0/IF1 session and uinput virtual keyboard.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use monsgeek_inputd::uinput_device::{create_uinput_device, emit_actions, VirtualKeyboardIdentity};
use monsgeek_protocol::DeviceRegistry;
use monsgeek_transport::active_path;
use monsgeek_transport::discovery::{self, DeviceInfo};
use monsgeek_transport::error::TransportError;
use monsgeek_transport::input::InputProcessor;
use monsgeek_transport::usb::{SessionMode, UsbSession};

use crate::DeviceSelector;

/// Configuration for the daemon's runtime behavior.
#[derive(Clone)]
pub struct DaemonConfig {
    /// Software debounce window in milliseconds.
    pub debounce_ms: u64,
    /// Optional exact selectors for restricting monitored devices.
    pub device_selectors: Vec<DeviceSelector>,
    /// Optional explicit name override for the uinput virtual device.
    pub device_name: Option<String>,
}

#[derive(Debug)]
enum WorkerExit {
    Disconnected,
    Shutdown,
    Error(String),
}

impl fmt::Display for WorkerExit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disconnected => write!(f, "device disconnected"),
            Self::Shutdown => write!(f, "shutdown"),
            Self::Error(err) => write!(f, "{err}"),
        }
    }
}

struct WorkerHandle {
    shutdown: Arc<AtomicBool>,
    join: JoinHandle<WorkerExit>,
}

#[derive(Debug, Default)]
struct InputLatencyWindow {
    read_block_us: Vec<u64>,
    process_us: Vec<u64>,
    emit_us: Vec<u64>,
    read_to_emit_us: Vec<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct LatencySummary {
    count: usize,
    p50_us: u64,
    p95_us: u64,
    max_us: u64,
}

impl InputLatencyWindow {
    fn record_report(&mut self, read_block: Duration, process: Duration) {
        self.read_block_us.push(duration_micros(read_block));
        self.process_us.push(duration_micros(process));
    }

    fn record_emit(&mut self, emit: Duration, read_to_emit: Duration) {
        self.emit_us.push(duration_micros(emit));
        self.read_to_emit_us.push(duration_micros(read_to_emit));
    }

    fn is_empty(&self) -> bool {
        self.read_block_us.is_empty()
            && self.process_us.is_empty()
            && self.emit_us.is_empty()
            && self.read_to_emit_us.is_empty()
    }

    fn take_summaries(&mut self) -> InputLatencySummaries {
        InputLatencySummaries {
            reports_timed: self.read_block_us.len(),
            reports_with_actions: self.emit_us.len(),
            read_block: summarize_latency(&mut self.read_block_us),
            process: summarize_latency(&mut self.process_us),
            emit: summarize_latency(&mut self.emit_us),
            read_to_emit: summarize_latency(&mut self.read_to_emit_us),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct InputLatencySummaries {
    reports_timed: usize,
    reports_with_actions: usize,
    read_block: LatencySummary,
    process: LatencySummary,
    emit: LatencySummary,
    read_to_emit: LatencySummary,
}

fn duration_micros(duration: Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}

fn summarize_latency(samples: &mut Vec<u64>) -> LatencySummary {
    if samples.is_empty() {
        return LatencySummary::default();
    }

    samples.sort_unstable();
    let summary = LatencySummary {
        count: samples.len(),
        p50_us: percentile_us(samples, 50),
        p95_us: percentile_us(samples, 95),
        max_us: *samples.last().expect("non-empty samples have a max"),
    };
    samples.clear();
    summary
}

fn percentile_us(sorted_samples: &[u64], percentile: usize) -> u64 {
    debug_assert!(!sorted_samples.is_empty());
    let max_index = sorted_samples.len() - 1;
    let index = (max_index * percentile).div_ceil(100);
    sorted_samples[index]
}

const WATCHDOG_INTERVAL: Duration = Duration::from_secs(5);
const SUPERVISOR_TICK: Duration = Duration::from_secs(1);
const ACTIVE_PATH_PUBLISH_INTERVAL: Duration = Duration::from_secs(15);
const INPUT_METRICS_LOG_INTERVAL: Duration = Duration::from_secs(5);
const DISCONNECT_ERROR_THRESHOLD: u32 = 10;
const DEVICE_REGISTRY_PATH: &str = "crates/monsgeek-protocol/devices";

pub fn run_daemon(config: DaemonConfig) -> Result<(), Box<dyn std::error::Error>> {
    let shutdown = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&shutdown))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&shutdown))?;

    let registry = DeviceRegistry::load_from_directory(&registry_dir())?;
    let mut workers: HashMap<String, WorkerHandle> = HashMap::new();
    let mut last_watchdog = Instant::now() - WATCHDOG_INTERVAL;

    log::info!("Discovery mode: descriptor-only, multi-device supervisor");
    sd_notify::notify(&[sd_notify::NotifyState::Ready]).ok();

    while !shutdown.load(Ordering::Relaxed) {
        if last_watchdog.elapsed() >= WATCHDOG_INTERVAL {
            sd_notify::notify(&[sd_notify::NotifyState::Watchdog]).ok();
            last_watchdog = Instant::now();
        }

        let devices = select_devices(
            discovery::find_devices_no_probe(&registry)?,
            &config.device_selectors,
        )?;
        let live_paths: HashSet<String> = devices
            .iter()
            .map(|device| device.instance_path.clone())
            .collect();

        let stale_paths: Vec<String> = workers
            .keys()
            .filter(|path| !live_paths.contains(path.as_str()))
            .cloned()
            .collect();
        for path in stale_paths {
            if let Some(worker) = workers.remove(&path) {
                worker.shutdown.store(true, Ordering::Relaxed);
                let _ = worker.join.join();
            }
        }

        let finished_paths: Vec<String> = workers
            .iter()
            .filter(|(_, worker)| worker.join.is_finished())
            .map(|(path, _)| path.clone())
            .collect();
        for path in finished_paths {
            if let Some(worker) = workers.remove(&path) {
                match worker.join.join() {
                    Ok(exit) => log::info!("input worker {} exited: {}", path, exit),
                    Err(_) => log::warn!("input worker {} panicked", path),
                }
            }
        }

        for device in devices {
            if workers.contains_key(&device.instance_path) {
                continue;
            }
            let worker = spawn_worker(device, config.clone(), Arc::clone(&shutdown));
            workers.insert(worker.0, worker.1);
        }

        let online_count = workers.len();
        sd_notify::notify(&[sd_notify::NotifyState::Status(&format!(
            "monitoring {} keyboard(s)",
            online_count
        ))])
        .ok();

        std::thread::sleep(SUPERVISOR_TICK);
    }

    for (_, worker) in workers {
        worker.shutdown.store(true, Ordering::Relaxed);
        let _ = worker.join.join();
    }
    let _ = active_path::clear_active_path();
    Ok(())
}

fn spawn_worker(
    device: DeviceInfo,
    config: DaemonConfig,
    global_shutdown: Arc<AtomicBool>,
) -> (String, WorkerHandle) {
    let path = device.instance_path.clone();
    let shutdown = Arc::new(AtomicBool::new(false));
    let worker_shutdown = Arc::clone(&shutdown);
    let join = std::thread::Builder::new()
        .name(format!("monsgeek-input-{}", sanitize_thread_name(&path)))
        .spawn(move || run_worker(device, config, global_shutdown, worker_shutdown))
        .expect("failed to spawn input worker");
    (path, WorkerHandle { shutdown, join })
}

fn run_worker(
    device: DeviceInfo,
    config: DaemonConfig,
    global_shutdown: Arc<AtomicBool>,
    worker_shutdown: Arc<AtomicBool>,
) -> WorkerExit {
    let session =
        match UsbSession::open_at_with_mode(device.bus, device.address, SessionMode::InputOnly) {
            Ok(session) => session,
            Err(err) => return WorkerExit::Error(format!("open failed: {err}")),
        };

    let uinput_name = config.device_name.clone().unwrap_or_else(|| {
        format!(
            "MonsGeek {} [{}]",
            device.display_name, device.instance_path
        )
    });
    let mut uinput_dev = match create_uinput_device(
        &uinput_name,
        VirtualKeyboardIdentity::new(device.vid, device.pid),
    ) {
        Ok(device) => device,
        Err(err) => return WorkerExit::Error(format!("uinput open failed: {err}")),
    };
    let mut processor = InputProcessor::new(config.debounce_ms);
    let mut report = [0u8; 8];
    let mut last_report = [0u8; 8];
    let mut last_active_publish = Instant::now();
    let mut last_metrics_log = Instant::now();
    let mut consecutive_errors = 0u32;
    let mut reports_ok = 0u64;
    let mut report_timeouts_or_short = 0u64;
    let mut report_errors = 0u64;
    let mut emit_batches = 0u64;
    let mut emitted_actions_count = 0u64;
    let mut latency_window = InputLatencyWindow::default();

    log::info!(
        "worker start path={} location={} debounce={}ms",
        device.instance_path,
        device.usb_location,
        config.debounce_ms
    );
    let _ = active_path::publish_active_path(
        &device.instance_path,
        &device.usb_location,
        device.vid,
        device.pid,
        device.bus,
        device.address,
    );

    loop {
        if last_active_publish.elapsed() >= ACTIVE_PATH_PUBLISH_INTERVAL {
            let _ = active_path::publish_active_path(
                &device.instance_path,
                &device.usb_location,
                device.vid,
                device.pid,
                device.bus,
                device.address,
            );
            last_active_publish = Instant::now();
        }
        if last_metrics_log.elapsed() >= INPUT_METRICS_LOG_INTERVAL {
            let metrics_window_ms = last_metrics_log.elapsed().as_millis();
            let processor_metrics = processor.take_metrics();
            log::info!(
                "input_metrics path={} window_ms={} reports_ok={} timeouts_or_short={} report_errors={} emit_batches={} emit_actions={} debounce_suppressed={} duplicate_prev_skipped={} duplicate_current_skipped={} parser_reports_total={} parser_reports_short={} parser_actions_emitted={}",
                device.instance_path,
                metrics_window_ms,
                reports_ok,
                report_timeouts_or_short,
                report_errors,
                emit_batches,
                emitted_actions_count,
                processor_metrics.debounce_suppressed,
                processor_metrics.duplicate_prev_skipped,
                processor_metrics.duplicate_current_skipped,
                processor_metrics.reports_total,
                processor_metrics.reports_short,
                processor_metrics.key_actions_emitted
            );
            if !latency_window.is_empty() {
                let latency = latency_window.take_summaries();
                log::info!(
                    "input_latency path={} window_ms={} reports_timed={} reports_with_actions={} read_block_count={} read_block_p50_us={} read_block_p95_us={} read_block_max_us={} process_count={} process_p50_us={} process_p95_us={} process_max_us={} emit_count={} emit_p50_us={} emit_p95_us={} emit_max_us={} read_to_emit_count={} read_to_emit_p50_us={} read_to_emit_p95_us={} read_to_emit_max_us={}",
                    device.instance_path,
                    metrics_window_ms,
                    latency.reports_timed,
                    latency.reports_with_actions,
                    latency.read_block.count,
                    latency.read_block.p50_us,
                    latency.read_block.p95_us,
                    latency.read_block.max_us,
                    latency.process.count,
                    latency.process.p50_us,
                    latency.process.p95_us,
                    latency.process.max_us,
                    latency.emit.count,
                    latency.emit.p50_us,
                    latency.emit.p95_us,
                    latency.emit.max_us,
                    latency.read_to_emit.count,
                    latency.read_to_emit.p50_us,
                    latency.read_to_emit.p95_us,
                    latency.read_to_emit.max_us
                );
            }
            reports_ok = 0;
            report_timeouts_or_short = 0;
            report_errors = 0;
            emit_batches = 0;
            emitted_actions_count = 0;
            last_metrics_log = Instant::now();
        }

        if global_shutdown.load(Ordering::Relaxed) || worker_shutdown.load(Ordering::Relaxed) {
            let _ = emit_actions(&mut uinput_dev, &processor.release_all_keys());
            let _ = active_path::remove_active_path(&device.instance_path);
            return WorkerExit::Shutdown;
        }

        let read_started_at = Instant::now();
        match session.read_report_with_timeout(&mut report, Duration::from_millis(10)) {
            Ok(n) if n >= 8 => {
                let read_completed_at = Instant::now();
                consecutive_errors = 0;
                reports_ok += 1;

                let process_started_at = Instant::now();
                let actions = processor.process_report(&report);
                let process_completed_at = Instant::now();
                latency_window.record_report(
                    read_completed_at.duration_since(read_started_at),
                    process_completed_at.duration_since(process_started_at),
                );

                if report != last_report
                    && (report.iter().any(|&b| b != 0) || last_report.iter().any(|&b| b != 0))
                {
                    log::debug!(
                        "raw_if0_report path={} raw={} actions={}",
                        device.instance_path,
                        format_report(&report),
                        format_actions(&actions)
                    );
                    last_report = report;
                }
                if !actions.is_empty() {
                    let emit_started_at = Instant::now();
                    if let Err(err) = emit_actions(&mut uinput_dev, &actions) {
                        let _ = active_path::remove_active_path(&device.instance_path);
                        return WorkerExit::Error(format!("uinput emit failed: {err}"));
                    }
                    let emit_completed_at = Instant::now();
                    latency_window.record_emit(
                        emit_completed_at.duration_since(emit_started_at),
                        emit_completed_at.duration_since(read_completed_at),
                    );
                    emit_batches += 1;
                    emitted_actions_count += actions.len() as u64;
                }
            }
            Ok(_) => {
                consecutive_errors = 0;
                report_timeouts_or_short += 1;
            }
            Err(TransportError::Disconnected) => {
                let _ = emit_actions(&mut uinput_dev, &processor.release_all_keys());
                let _ = active_path::remove_active_path(&device.instance_path);
                return WorkerExit::Disconnected;
            }
            Err(err) => {
                consecutive_errors += 1;
                report_errors += 1;
                if consecutive_errors >= DISCONNECT_ERROR_THRESHOLD {
                    let _ = emit_actions(&mut uinput_dev, &processor.release_all_keys());
                    let _ = active_path::remove_active_path(&device.instance_path);
                    return WorkerExit::Error(format!(
                        "disconnect after {} consecutive errors: {}",
                        consecutive_errors, err
                    ));
                }
            }
        }
    }
}

fn format_report(report: &[u8; 8]) -> String {
    let modifier_names = [
        (0x01, "LCTRL"),
        (0x02, "LSHIFT"),
        (0x04, "LALT"),
        (0x08, "LMETA"),
        (0x10, "RCTRL"),
        (0x20, "RSHIFT"),
        (0x40, "RALT"),
        (0x80, "RMETA"),
    ]
    .into_iter()
    .filter_map(|(bit, name)| (report[0] & bit != 0).then_some(name))
    .collect::<Vec<_>>();

    let keys = report[2..]
        .iter()
        .copied()
        .filter(|hid| *hid != 0)
        .map(|hid| format!("0x{hid:02X}"))
        .collect::<Vec<_>>();

    format!(
        "mod=0x{:02X}[{}] keys=[{}] bytes={:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
        report[0],
        if modifier_names.is_empty() {
            "none".to_string()
        } else {
            modifier_names.join(",")
        },
        if keys.is_empty() {
            "none".to_string()
        } else {
            keys.join(",")
        },
        report[0],
        report[1],
        report[2],
        report[3],
        report[4],
        report[5],
        report[6],
        report[7]
    )
}

fn format_actions(actions: &[monsgeek_transport::input::KeyAction]) -> String {
    if actions.is_empty() {
        return "none".to_string();
    }

    actions
        .iter()
        .map(|action| format!("{}:{}", action.keycode, action.value))
        .collect::<Vec<_>>()
        .join(",")
}

fn select_devices(
    mut devices: Vec<DeviceInfo>,
    selectors: &[DeviceSelector],
) -> Result<Vec<DeviceInfo>, String> {
    if devices.is_empty() {
        return Err("No MonsGeek keyboards found. Is the keyboard connected?".to_string());
    }

    devices.sort_by_key(|device| device.instance_path.clone());

    if !selectors.is_empty() {
        devices.retain(|device| {
            selectors.iter().any(|selector| match selector {
                DeviceSelector::BusAddress(bus, address) => {
                    device.bus == *bus && device.address == *address
                }
                DeviceSelector::InstancePath(path) => device.instance_path == *path,
                DeviceSelector::UsbLocation(location) => device.usb_location == *location,
            })
        });
        if devices.is_empty() {
            return Err(format!(
                "No MonsGeek keyboard matched selectors: {}",
                selectors
                    .iter()
                    .map(selector_label)
                    .collect::<Vec<&str>>()
                    .join(", ")
            ));
        }
    }

    Ok(devices)
}

fn selector_label(selector: &DeviceSelector) -> &str {
    match selector {
        DeviceSelector::BusAddress(_, _) => "bus:addr",
        DeviceSelector::InstancePath(_) => "instance-path",
        DeviceSelector::UsbLocation(_) => "usb-location",
    }
}

fn sanitize_thread_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn registry_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("monsgeek-protocol")
        .join("devices")
        .canonicalize()
        .unwrap_or_else(|_| Path::new(DEVICE_REGISTRY_PATH).to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use monsgeek_transport::discovery::ConnectionMode;

    fn fake_device(path: &str, bus: u8, address: u8) -> DeviceInfo {
        DeviceInfo {
            instance_path: path.to_string(),
            usb_location: path.to_string(),
            vid: 0x3151,
            pid: 0x4015,
            canonical_pid: 0x4015,
            device_id: 1308,
            display_name: "M5W".to_string(),
            name: "yc3121_m5w_soc".to_string(),
            connection_mode: ConnectionMode::Usb,
            bus,
            address,
        }
    }

    #[test]
    fn select_devices_filters_by_bus_address() {
        let devices = vec![
            fake_device("usb-b003-p1.2", 3, 15),
            fake_device("usb-b003-p1.3", 3, 16),
        ];
        let selected = select_devices(devices, &[DeviceSelector::BusAddress(3, 16)])
            .expect("device should match");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].address, 16);
    }

    #[test]
    fn select_devices_filters_by_usb_location() {
        let devices = vec![
            fake_device("usb-b003-p1.2", 3, 15),
            fake_device("usb-b003-p1.3", 3, 16),
        ];
        let selected = select_devices(
            devices,
            &[DeviceSelector::UsbLocation("usb-b003-p1.2".to_string())],
        )
        .expect("device should match");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].instance_path, "usb-b003-p1.2");
    }

    #[test]
    fn sanitize_thread_name_replaces_symbols() {
        assert_eq!(sanitize_thread_name("usb-b003-p1.2"), "usb_b003_p1_2");
    }
}
