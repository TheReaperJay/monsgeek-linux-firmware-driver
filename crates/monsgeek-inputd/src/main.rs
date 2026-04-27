mod config;
mod daemon;

use clap::Parser;

#[derive(Debug, Clone, PartialEq, Eq)]
enum DeviceSelector {
    BusAddress(u8, u8),
    InstancePath(String),
    UsbLocation(String),
}

#[derive(Parser)]
#[command(name = "monsgeek-inputd", about = "MonsGeek keyboard input daemon")]
struct Cli {
    /// Software debounce window in milliseconds (overrides config file)
    #[arg(long, value_name = "MS")]
    debounce_ms: Option<u64>,

    /// Restrict monitoring to specific device selectors. Accepts BUS:ADDR, instance path, or usb location.
    #[arg(long, value_name = "TARGET")]
    device: Vec<String>,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    let config = config::load_config();
    let debounce_ms = config::resolve_debounce_ms(cli.debounce_ms, &config);

    log::info!("Effective debounce: {}ms", debounce_ms);

    let device_selectors: Vec<DeviceSelector> = cli
        .device
        .iter()
        .map(|value| {
            parse_device_selector(value).unwrap_or_else(|err| {
                log::error!("{err}");
                std::process::exit(1);
            })
        })
        .collect();

    let daemon_config = daemon::DaemonConfig {
        debounce_ms,
        device_selectors,
        device_name: None,
    };

    if let Err(e) = daemon::run_daemon(daemon_config) {
        log::error!("Daemon exited with error: {}", e);
        std::process::exit(1);
    }
}

fn parse_device_selector(input: &str) -> Result<DeviceSelector, String> {
    let parts: Vec<&str> = input.split(':').collect();
    if parts.len() == 2 {
        if let (Ok(bus), Ok(address)) = (parts[0].parse::<u8>(), parts[1].parse::<u8>()) {
            return Ok(DeviceSelector::BusAddress(bus, address));
        }
        return Err(format!(
            "Invalid --device selector '{}': expected BUS:ADDR, instance path, or usb location",
            input
        ));
    }

    if input.starts_with("usb-b") {
        if input.contains("-p") {
            return Ok(DeviceSelector::UsbLocation(input.to_string()));
        }
        if input.contains("-a") {
            return Ok(DeviceSelector::InstancePath(input.to_string()));
        }
    }

    Ok(DeviceSelector::InstancePath(input.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_device_selector_accepts_bus_address() {
        assert_eq!(
            parse_device_selector("003:015").expect("selector should parse"),
            DeviceSelector::BusAddress(3, 15)
        );
    }

    #[test]
    fn parse_device_selector_accepts_usb_location() {
        assert_eq!(
            parse_device_selector("usb-b003-p1.2").expect("selector should parse"),
            DeviceSelector::UsbLocation("usb-b003-p1.2".to_string())
        );
    }

    #[test]
    fn parse_device_selector_accepts_instance_path() {
        assert_eq!(
            parse_device_selector("usb-b003-a015").expect("selector should parse"),
            DeviceSelector::InstancePath("usb-b003-a015".to_string())
        );
    }
}
