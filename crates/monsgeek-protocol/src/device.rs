use serde::{Deserialize, Serialize};

use crate::protocol::{CommandTable, ProtocolFamily};

/// Numeric range configuration for travel/actuation settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeConfig {
    pub min: f32,
    pub max: f32,
    #[serde(default)]
    pub step: Option<f32>,
    #[serde(default)]
    pub default: Option<f32>,
}

/// Travel settings for magnetic (Hall effect) switch keyboards.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TravelSetting {
    #[serde(default)]
    pub travel: Option<RangeConfig>,
    #[serde(default)]
    pub fire_press: Option<RangeConfig>,
    #[serde(default)]
    pub fire_lift: Option<RangeConfig>,
    #[serde(default)]
    pub deadzone: Option<RangeConfig>,
}

/// Fn/Sys layer counts per OS mode.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FnSysLayer {
    #[serde(default)]
    pub win: u8,
    #[serde(default)]
    pub mac: u8,
}

/// Per-device command-byte overrides layered on top of the baseline protocol
/// family table.
///
/// This is necessary because some keyboards do not cleanly follow the generic
/// family mapping for every command byte. The M5W is the first verified case:
/// it is a YC3121 device, but its debounce/report commands follow the exact
/// values observed in the M5W reference project and on live hardware.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandOverrides {
    #[serde(default)]
    pub set_reset: Option<u8>,
    #[serde(default)]
    pub set_profile: Option<u8>,
    #[serde(default)]
    pub set_debounce: Option<u8>,
    #[serde(default)]
    pub set_keymatrix: Option<u8>,
    #[serde(default)]
    pub set_macro: Option<u8>,
    #[serde(default)]
    pub get_profile: Option<u8>,
    #[serde(default)]
    pub get_debounce: Option<u8>,
    #[serde(default)]
    pub get_keymatrix: Option<u8>,
    #[serde(default)]
    pub set_keymatrix_simple: Option<u8>,
    #[serde(default)]
    pub get_keymatrix_simple: Option<u8>,
    #[serde(default)]
    pub set_fn_simple: Option<u8>,
    #[serde(default)]
    pub get_fn_simple: Option<u8>,
    #[serde(default)]
    pub set_report: Option<u8>,
    #[serde(default)]
    pub set_kboption: Option<u8>,
    #[serde(default)]
    pub set_sleeptime: Option<u8>,
    #[serde(default)]
    pub get_report: Option<u8>,
    #[serde(default)]
    pub get_kboption: Option<u8>,
    #[serde(default)]
    pub get_sleeptime: Option<u8>,
}

fn default_device_type() -> String {
    "keyboard".to_string()
}

/// Complete device definition loaded from a per-device JSON file.
///
/// Fields use `#[serde(rename_all = "camelCase")]` to map from camelCase JSON
/// keys to snake_case Rust fields. The schema matches all yc3121-based keyboards,
/// not just the M5W.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDefinition {
    /// Device ID (signed -- some device IDs are negative for special devices).
    pub id: i32,
    /// USB Vendor ID.
    pub vid: u16,
    /// USB Product ID.
    pub pid: u16,
    /// Internal device name (e.g., "yc3121_m5w_soc").
    pub name: String,
    /// Human-readable display name (e.g., "M5W").
    pub display_name: String,
    #[serde(default)]
    pub company: Option<String>,
    #[serde(rename = "type", default = "default_device_type")]
    pub device_type: String,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub key_count: Option<u8>,
    #[serde(default)]
    pub key_layout_name: Option<String>,
    #[serde(default)]
    pub layer: Option<u8>,
    #[serde(default)]
    pub fn_sys_layer: Option<FnSysLayer>,
    /// True if device has magnetic (Hall effect) switches.
    #[serde(default)]
    pub magnetism: Option<bool>,
    /// True if device explicitly does NOT have magnetic switches.
    #[serde(default)]
    pub no_magnetic_switch: Option<bool>,
    #[serde(default)]
    pub has_light_layout: Option<bool>,
    #[serde(default)]
    pub has_side_light: Option<bool>,
    #[serde(default)]
    pub hot_swap: Option<bool>,
    #[serde(default)]
    pub travel_setting: Option<TravelSetting>,
    /// LED matrix mapping position index to HID keycode.
    #[serde(default)]
    pub led_matrix: Option<Vec<u8>>,
    /// Chip family (e.g., "YC3121", "RY5088").
    #[serde(default)]
    pub chip_family: Option<String>,
    /// Optional per-device overrides for command bytes that diverge from the
    /// baseline family table.
    #[serde(default)]
    pub command_overrides: Option<CommandOverrides>,
}

impl DeviceDefinition {
    /// Check if this device has magnetic (Hall effect) switches.
    ///
    /// Returns true if `magnetism` is explicitly true, or if `no_magnetic_switch`
    /// is explicitly false. If neither field is set, defaults to false (no magnetism).
    pub fn has_magnetism(&self) -> bool {
        if let Some(magnetism) = self.magnetism {
            return magnetism;
        }
        if let Some(no_magnetic) = self.no_magnetic_switch {
            return !no_magnetic;
        }
        false
    }

    /// Resolve the baseline command table for this device from protocol family
    /// heuristics.
    pub fn protocol_family(&self) -> ProtocolFamily {
        ProtocolFamily::detect(Some(&self.name), self.pid)
    }

    /// Resolve command bytes for this device, applying any per-device
    /// overrides on top of the baseline family table.
    pub fn commands(&self) -> CommandTable {
        let mut table = *self.protocol_family().commands();

        if let Some(overrides) = &self.command_overrides {
            if let Some(value) = overrides.set_reset {
                table.set_reset = value;
            }
            if let Some(value) = overrides.set_profile {
                table.set_profile = value;
            }
            if let Some(value) = overrides.set_debounce {
                table.set_debounce = value;
            }
            if let Some(value) = overrides.set_keymatrix {
                table.set_keymatrix = value;
            }
            if let Some(value) = overrides.set_macro {
                table.set_macro = value;
            }
            if let Some(value) = overrides.get_profile {
                table.get_profile = value;
            }
            if let Some(value) = overrides.get_debounce {
                table.get_debounce = value;
            }
            if let Some(value) = overrides.get_keymatrix {
                table.get_keymatrix = value;
            }
            if let Some(value) = overrides.set_keymatrix_simple {
                table.set_keymatrix_simple = Some(value);
            }
            if let Some(value) = overrides.get_keymatrix_simple {
                table.get_keymatrix_simple = Some(value);
            }
            if let Some(value) = overrides.set_fn_simple {
                table.set_fn_simple = Some(value);
            }
            if let Some(value) = overrides.get_fn_simple {
                table.get_fn_simple = Some(value);
            }
            if let Some(value) = overrides.set_report {
                table.set_report = Some(value);
            }
            if let Some(value) = overrides.set_kboption {
                table.set_kboption = Some(value);
            }
            if let Some(value) = overrides.set_sleeptime {
                table.set_sleeptime = Some(value);
            }
            if let Some(value) = overrides.get_report {
                table.get_report = Some(value);
            }
            if let Some(value) = overrides.get_kboption {
                table.get_kboption = Some(value);
            }
            if let Some(value) = overrides.get_sleeptime {
                table.get_sleeptime = Some(value);
            }
        }

        table
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m5w_json() -> &'static str {
        include_str!("../devices/m5w.json")
    }

    #[test]
    fn test_m5w_device_definition() {
        let device: DeviceDefinition =
            serde_json::from_str(m5w_json()).expect("failed to deserialize M5W JSON");

        assert_eq!(device.id, 1308);
        assert_eq!(device.vid, 0x3151); // 12625
        assert_eq!(device.pid, 0x4015); // 16405
        assert_eq!(device.name, "yc3121_m5w_soc");
        assert_eq!(device.display_name, "M5W");
        assert_eq!(device.company, Some("MonsGeek".to_string()));
        assert_eq!(device.key_count, Some(108));
        assert_eq!(device.key_layout_name, Some("Common108_MG108B".to_string()));
        assert_eq!(device.layer, Some(4));

        let fn_sys = device.fn_sys_layer.as_ref().expect("fn_sys_layer missing");
        assert_eq!(fn_sys.win, 2);
        assert_eq!(fn_sys.mac, 2);

        assert_eq!(device.magnetism, None);
        assert_eq!(device.no_magnetic_switch, Some(true));
        assert_eq!(device.has_light_layout, Some(true));
        assert_eq!(device.has_side_light, Some(false));
        assert_eq!(device.hot_swap, Some(false));
        assert_eq!(device.chip_family, Some("YC3121".to_string()));
        let commands = device.commands();
        assert_eq!(commands.set_debounce, 0x11);
        assert_eq!(commands.get_debounce, 0x91);
        assert_eq!(commands.set_macro, 0x0B);
        assert_eq!(commands.set_kboption, Some(0x06));
        assert_eq!(commands.get_kboption, Some(0x86));
        assert_eq!(commands.set_sleeptime, Some(0x12));
        assert_eq!(commands.get_sleeptime, Some(0x92));
        assert_eq!(commands.set_report, None);
        assert_eq!(commands.get_report, None);
    }

    #[test]
    fn test_m5w_identity() {
        let device: DeviceDefinition =
            serde_json::from_str(m5w_json()).expect("failed to deserialize M5W JSON");

        assert_eq!(device.vid, 0x3151, "VID should be 0x3151 (MonsGeek)");
        assert_eq!(device.pid, 0x4015, "PID should be 0x4015");
        assert_eq!(device.id, 1308, "device ID should be 1308");
    }

    #[test]
    fn test_has_magnetism_false_when_no_magnetic_switch_true() {
        let json =
            r#"{"id":1,"vid":1,"pid":1,"name":"t","displayName":"T","noMagneticSwitch":true}"#;
        let device: DeviceDefinition = serde_json::from_str(json).unwrap();
        assert!(!device.has_magnetism());
    }

    #[test]
    fn test_has_magnetism_true_when_magnetism_true() {
        let json = r#"{"id":1,"vid":1,"pid":1,"name":"t","displayName":"T","magnetism":true}"#;
        let device: DeviceDefinition = serde_json::from_str(json).unwrap();
        assert!(device.has_magnetism());
    }

    #[test]
    fn test_has_magnetism_true_when_no_magnetic_switch_false() {
        let json =
            r#"{"id":1,"vid":1,"pid":1,"name":"t","displayName":"T","noMagneticSwitch":false}"#;
        let device: DeviceDefinition = serde_json::from_str(json).unwrap();
        assert!(device.has_magnetism());
    }

    #[test]
    fn test_optional_fields_default_to_none() {
        let json = r#"{"id":1,"vid":1,"pid":1,"name":"minimal","displayName":"Minimal"}"#;
        let device: DeviceDefinition = serde_json::from_str(json).unwrap();

        assert_eq!(device.company, None);
        assert_eq!(device.device_type, "keyboard");
        assert!(device.sources.is_empty());
        assert_eq!(device.key_count, None);
        assert_eq!(device.key_layout_name, None);
        assert_eq!(device.layer, None);
        assert!(device.fn_sys_layer.is_none());
        assert_eq!(device.magnetism, None);
        assert_eq!(device.no_magnetic_switch, None);
        assert_eq!(device.has_light_layout, None);
        assert_eq!(device.has_side_light, None);
        assert_eq!(device.hot_swap, None);
        assert!(device.travel_setting.is_none());
        assert_eq!(device.led_matrix, None);
        assert_eq!(device.chip_family, None);
        assert!(device.command_overrides.is_none());
    }

    #[test]
    fn test_fn_sys_layer_deserialization() {
        let json = r#"{"win": 2, "mac": 2}"#;
        let fn_sys: FnSysLayer = serde_json::from_str(json).unwrap();
        assert_eq!(fn_sys.win, 2);
        assert_eq!(fn_sys.mac, 2);
    }

    #[test]
    fn test_commands_fall_back_to_family_when_no_overrides() {
        let json = r#"{"id":1,"vid":1,"pid":16389,"name":"yc500_test","displayName":"Test"}"#;
        let device: DeviceDefinition = serde_json::from_str(json).unwrap();
        let commands = device.commands();
        assert_eq!(device.protocol_family(), ProtocolFamily::YiChip);
        assert_eq!(commands.set_debounce, 0x11);
        assert_eq!(commands.get_debounce, 0x91);
        assert_eq!(commands.set_report, None);
        assert_eq!(commands.get_report, None);
    }

    #[test]
    fn test_commands_apply_device_overrides() {
        let json = r#"{
            "id": 1308,
            "vid": 12625,
            "pid": 16405,
            "name": "yc3121_m5w_soc",
            "displayName": "M5W",
            "commandOverrides": {
                "setDebounce": 17,
                "getDebounce": 145,
                "setMacro": 11,
                "setKboption": 6,
                "getKboption": 134,
                "setSleeptime": 18,
                "getSleeptime": 146
            }
        }"#;
        let device: DeviceDefinition = serde_json::from_str(json).unwrap();
        let commands = device.commands();
        assert_eq!(device.protocol_family(), ProtocolFamily::YiChip);
        assert_eq!(commands.set_debounce, 0x11);
        assert_eq!(commands.get_debounce, 0x91);
        assert_eq!(commands.set_macro, 0x0B);
        assert_eq!(commands.set_kboption, Some(0x06));
        assert_eq!(commands.get_kboption, Some(0x86));
        assert_eq!(commands.set_sleeptime, Some(0x12));
        assert_eq!(commands.get_sleeptime, Some(0x92));
        assert_eq!(commands.set_report, None);
        assert_eq!(commands.get_report, None);
    }
}
