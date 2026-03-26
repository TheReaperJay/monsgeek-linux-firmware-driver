mod config;
mod uinput_device;

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
    // Daemon loop will be implemented in Plan 02
    eprintln!("monsgeek-inputd: daemon loop not yet implemented");
}
