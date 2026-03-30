use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Deserialize;

const CONFIG_PATH_ENV: &str = "MONSGEEK_TRANSPORT_CONFIG";
const DEFAULT_CONFIG_PATH: &str = "/etc/monsgeek/transport-config.json";

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct RuntimeConfig {
    pub discovery: DiscoveryConfig,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            discovery: DiscoveryConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct DiscoveryConfig {
    pub query_retries: usize,
    pub dongle_forward_send_retries: usize,
    pub dongle_forward_poll_retries_per_send: usize,
    pub dongle_status_poll_retries: usize,
    pub dongle_forward_budget_ms: u64,
    pub fallback_to_direct_query: bool,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            query_retries: 2,
            dongle_forward_send_retries: 12,
            dongle_forward_poll_retries_per_send: 2,
            dongle_status_poll_retries: 4,
            dongle_forward_budget_ms: 1300,
            fallback_to_direct_query: true,
        }
    }
}

pub(crate) fn runtime_config() -> &'static RuntimeConfig {
    static RUNTIME_CONFIG: OnceLock<RuntimeConfig> = OnceLock::new();
    RUNTIME_CONFIG.get_or_init(load_runtime_config)
}

fn config_path() -> PathBuf {
    if let Ok(path) = std::env::var(CONFIG_PATH_ENV)
        && !path.trim().is_empty()
    {
        return PathBuf::from(path);
    }
    PathBuf::from(DEFAULT_CONFIG_PATH)
}

fn load_runtime_config() -> RuntimeConfig {
    let path = config_path();
    let raw = match fs::read(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            log::info!(
                "transport runtime config not found at {}; using defaults",
                path.display()
            );
            return RuntimeConfig::default();
        }
        Err(err) => {
            log::warn!(
                "failed reading transport runtime config {}: {}; using defaults",
                path.display(),
                err
            );
            return RuntimeConfig::default();
        }
    };

    match serde_json::from_slice::<RuntimeConfig>(&raw) {
        Ok(config) => config,
        Err(err) => {
            log::warn!(
                "failed parsing transport runtime config {}: {}; using defaults",
                path.display(),
                err
            );
            RuntimeConfig::default()
        }
    }
}
