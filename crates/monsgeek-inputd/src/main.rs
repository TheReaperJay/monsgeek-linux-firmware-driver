mod config;
mod daemon;

use clap::Parser;

#[derive(Parser)]
#[command(name = "monsgeek-inputd", about = "MonsGeek keyboard input daemon")]
struct Cli {
    /// Software debounce window in milliseconds (overrides config file)
    #[arg(long, value_name = "MS")]
    debounce_ms: Option<u64>,

    /// Target device by USB bus:address (e.g., "001:005"). Auto-detects if omitted.
    #[arg(long, value_name = "BUS:ADDR")]
    device: Option<String>,
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();
    let config = config::load_config();
    let debounce_ms = config::resolve_debounce_ms(cli.debounce_ms, &config);

    log::info!("Effective debounce: {}ms", debounce_ms);

    let device_filter = cli.device.as_ref().and_then(|d| {
        let parts: Vec<&str> = d.split(':').collect();
        if parts.len() == 2 {
            match (parts[0].parse::<u8>(), parts[1].parse::<u8>()) {
                (Ok(bus), Ok(addr)) => Some((bus, addr)),
                _ => {
                    log::error!(
                        "Invalid --device format '{}', expected BUS:ADDR (e.g., 001:005)",
                        d
                    );
                    std::process::exit(1);
                }
            }
        } else {
            log::error!(
                "Invalid --device format '{}', expected BUS:ADDR (e.g., 001:005)",
                d
            );
            std::process::exit(1);
        }
    });

    let daemon_config = daemon::DaemonConfig {
        debounce_ms,
        device_filter,
        device_name: None,
    };

    if let Err(e) = daemon::run_daemon(daemon_config) {
        log::error!("Daemon exited with error: {}", e);
        std::process::exit(1);
    }
}
