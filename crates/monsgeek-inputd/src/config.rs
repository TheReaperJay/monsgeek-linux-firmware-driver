use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Default software debounce for the userspace input path.
///
/// Repo diagnostics already measured real release→repress bounce in the
/// 6-12ms range on MonsGeek boards, and the hardware-gated input daemon tests
/// use 15ms as the reference value. Shipping 0ms makes the live path ignore the
/// very bounce the daemon exists to filter.
const DEFAULT_DEBOUNCE_MS: u64 = 15;

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Config {
    pub debounce_ms: Option<u64>,
}

/// Load config from user dir (~/.config/monsgeek/inputd.toml) then system dir (/etc/monsgeek/inputd.toml).
/// Returns Config::default() if neither exists or both are unreadable.
pub fn load_config() -> Config {
    let user_config = std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".config/monsgeek/inputd.toml"));

    let system_config = Path::new("/etc/monsgeek/inputd.toml");

    if let Some(path) = &user_config {
        if let Ok(contents) = std::fs::read_to_string(path) {
            if let Ok(config) = toml::from_str::<Config>(&contents) {
                log::info!("Loaded config from {}", path.display());
                return config;
            } else {
                log::warn!("Failed to parse config at {}", path.display());
            }
        }
    }

    if let Ok(contents) = std::fs::read_to_string(system_config) {
        if let Ok(config) = toml::from_str::<Config>(&contents) {
            log::info!("Loaded config from {}", system_config.display());
            return config;
        } else {
            log::warn!("Failed to parse config at {}", system_config.display());
        }
    }

    Config::default()
}

/// Resolve the effective debounce_ms value from CLI flag, config file, and hardcoded default.
/// Priority: CLI flag > config file > measured fallback (15ms).
pub fn resolve_debounce_ms(cli_flag: Option<u64>, config: &Config) -> u64 {
    cli_flag
        .or(config.debounce_ms)
        .unwrap_or(DEFAULT_DEBOUNCE_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_debounce_is_none() {
        let config = Config::default();
        assert_eq!(config.debounce_ms, None);
    }

    #[test]
    fn test_resolve_debounce_cli_wins_over_config() {
        let config = Config {
            debounce_ms: Some(10),
        };
        assert_eq!(resolve_debounce_ms(Some(20), &config), 20);
    }

    #[test]
    fn test_resolve_debounce_config_wins_over_default() {
        let config = Config {
            debounce_ms: Some(10),
        };
        assert_eq!(resolve_debounce_ms(None, &config), 10);
    }

    #[test]
    fn test_resolve_debounce_hardcoded_fallback() {
        let config = Config::default();
        assert_eq!(resolve_debounce_ms(None, &config), 15);
    }

    #[test]
    fn test_config_deserialize_from_toml() {
        let toml_str = "debounce_ms = 25";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.debounce_ms, Some(25));
    }

    #[test]
    fn test_config_deserialize_empty_toml() {
        let toml_str = "";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.debounce_ms, None);
    }
}
