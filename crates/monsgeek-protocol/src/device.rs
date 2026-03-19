use serde::{Deserialize, Serialize};

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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m5w_json() -> &'static str {
        include_str!("../devices/m5w.json")
    }

    #[test]
    fn test_m5w_device_definition() {
        let device: DeviceDefinition = serde_json::from_str(m5w_json())
            .expect("failed to deserialize M5W JSON");

        assert_eq!(device.id, 1308);
        assert_eq!(device.vid, 0x3141); // 12609
        assert_eq!(device.pid, 0x4005); // 16389
        assert_eq!(device.name, "yc3121_m5w_soc");
        assert_eq!(device.display_name, "M5W");
        assert_eq!(device.company, Some("MonsGeek".to_string()));
        assert_eq!(device.key_count, Some(108));
        assert_eq!(
            device.key_layout_name,
            Some("Common108_MG108B".to_string())
        );
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
    }

    #[test]
    fn test_m5w_identity() {
        let device: DeviceDefinition = serde_json::from_str(m5w_json())
            .expect("failed to deserialize M5W JSON");

        assert_eq!(device.vid, 0x3141, "VID should be 0x3141 (MonsGeek)");
        assert_eq!(device.pid, 0x4005, "PID should be 0x4005");
        assert_eq!(device.id, 1308, "device ID should be 1308");
    }

    #[test]
    fn test_has_magnetism_false_when_no_magnetic_switch_true() {
        let json = r#"{"id":1,"vid":1,"pid":1,"name":"t","displayName":"T","noMagneticSwitch":true}"#;
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
        let json = r#"{"id":1,"vid":1,"pid":1,"name":"t","displayName":"T","noMagneticSwitch":false}"#;
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
    }

    #[test]
    fn test_fn_sys_layer_deserialization() {
        let json = r#"{"win": 2, "mac": 2}"#;
        let fn_sys: FnSysLayer = serde_json::from_str(json).unwrap();
        assert_eq!(fn_sys.win, 2);
        assert_eq!(fn_sys.mac, 2);
    }
}
