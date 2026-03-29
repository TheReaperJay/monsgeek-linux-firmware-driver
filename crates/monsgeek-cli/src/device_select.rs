use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};
use monsgeek_driver::pb::driver::{DeviceList, dj_dev};
use monsgeek_protocol::{DeviceDefinition, DeviceRegistry};

#[derive(Debug, Clone)]
pub struct OnlineDevice {
    pub path: String,
    pub device_id: i32,
    pub vid: u16,
    pub pid: u16,
    pub definition: DeviceDefinition,
}

#[derive(Debug, Clone)]
pub struct ResolvedTargetDevice {
    pub path: String,
    pub device_id: i32,
    pub vid: u16,
    pub pid: u16,
    pub definition: DeviceDefinition,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SelectorOptions<'a> {
    pub path: Option<&'a str>,
    pub device_id: Option<i32>,
    pub model: Option<&'a str>,
}

pub fn load_registry() -> Result<DeviceRegistry> {
    DeviceRegistry::load_from_directory(&registry_dir())
        .map_err(|err| anyhow!("failed to load device registry: {err}"))
}

pub fn registry_dir() -> PathBuf {
    if let Ok(path) = std::env::var("MONSGEEK_DEVICE_REGISTRY_DIR") {
        return PathBuf::from(path);
    }

    let installed = PathBuf::from("/usr/share/monsgeek/protocol/devices");
    if installed.is_dir() {
        return installed;
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../monsgeek-protocol")
        .join("devices")
}

pub fn supported_online_devices(init: &DeviceList, registry: &DeviceRegistry) -> Vec<OnlineDevice> {
    let mut devices = Vec::new();

    for item in &init.dev_list {
        let Some(dj_dev::OneofDev::Dev(dev)) = item.oneof_dev.as_ref() else {
            continue;
        };

        if !dev.is_online {
            continue;
        }

        let vid = dev.vid as u16;
        let pid = dev.pid as u16;
        if !registry.supports_runtime_vid_pid(vid, pid) {
            continue;
        }

        let Some(definition) = resolve_definition(registry, dev.id, vid, pid) else {
            continue;
        };

        devices.push(OnlineDevice {
            path: dev.path.clone(),
            device_id: definition.id,
            vid,
            pid,
            definition,
        });
    }

    devices.sort_by(|a, b| a.path.cmp(&b.path));
    devices
}

pub fn resolve_target_device(
    selectors: SelectorOptions<'_>,
    online_supported: &[OnlineDevice],
    registry: &DeviceRegistry,
) -> Result<ResolvedTargetDevice> {
    // 1) --path
    if let Some(path) = selectors.path {
        let selected = online_supported
            .iter()
            .find(|device| device.path == path)
            .ok_or_else(|| anyhow!("no supported online device matched --path {path}"))?;
        return Ok(to_resolved(selected));
    }

    // 2) --device-id
    if let Some(device_id) = selectors.device_id {
        let matches: Vec<&OnlineDevice> = online_supported
            .iter()
            .filter(|device| device.device_id == device_id)
            .collect();
        return match matches.as_slice() {
            [single] => Ok(to_resolved(single)),
            [] => bail!("no supported online device matched --device-id {device_id}"),
            _ => bail!(
                "multiple devices matched --device-id {device_id}; refine with --path <bridge-path>"
            ),
        };
    }

    // 3) --model
    if let Some(model) = selectors.model {
        let requested = model.trim().to_ascii_lowercase();
        let matching_ids: HashSet<i32> = registry
            .all_devices()
            .filter(|definition| {
                model_aliases(definition)
                    .iter()
                    .any(|alias| alias == &requested)
            })
            .map(|definition| definition.id)
            .collect();

        if matching_ids.is_empty() {
            bail!("no registry model matched --model {model}");
        }

        let matches: Vec<&OnlineDevice> = online_supported
            .iter()
            .filter(|device| matching_ids.contains(&device.device_id))
            .collect();
        return match matches.as_slice() {
            [single] => Ok(to_resolved(single)),
            [] => bail!("model {model} is known but not currently online"),
            _ => bail!("model {model} matched multiple online devices; refine with --path"),
        };
    }

    // 4) auto-select single supported online device only.
    match online_supported {
        [single] => Ok(to_resolved(single)),
        [] => bail!("no supported online devices are currently available"),
        _ => bail!(
            "multiple supported devices are online; select one with --path <bridge-path>, --device-id <firmware-id>, or --model <slug>"
        ),
    }
}

pub fn model_aliases(definition: &DeviceDefinition) -> Vec<String> {
    let mut aliases = Vec::new();
    aliases.push(definition.name.to_ascii_lowercase());
    aliases.push(slugify(&definition.display_name));

    if let Some(company) = definition.company.as_deref() {
        aliases.push(slugify(&format!("{company}-{}", definition.display_name)));
    }

    aliases.sort();
    aliases.dedup();
    aliases
}

pub fn preferred_model_slug(definition: &DeviceDefinition) -> String {
    if let Some(company) = definition.company.as_deref() {
        return slugify(&format!("{company}-{}", definition.display_name));
    }
    slugify(&definition.display_name)
}

fn to_resolved(selected: &OnlineDevice) -> ResolvedTargetDevice {
    ResolvedTargetDevice {
        path: selected.path.clone(),
        device_id: selected.device_id,
        vid: selected.vid,
        pid: selected.pid,
        definition: selected.definition.clone(),
    }
}

fn resolve_definition(
    registry: &DeviceRegistry,
    runtime_id: i32,
    vid: u16,
    pid: u16,
) -> Option<DeviceDefinition> {
    if let Some(definition) = registry.find_by_id(runtime_id)
        && definition.supports_runtime_pid(pid)
        && definition.vid == vid
    {
        return Some(definition.clone());
    }

    let runtime_matches = registry.find_by_runtime_vid_pid(vid, pid);
    if runtime_matches.is_empty() {
        return None;
    }

    if let Some(definition) = runtime_matches.into_iter().find(|d| d.id == runtime_id) {
        return Some(definition.clone());
    }

    registry
        .find_by_runtime_vid_pid(vid, pid)
        .first()
        .map(|definition| (*definition).clone())
}

fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;

    for ch in value.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }

    out.trim_matches('-').to_string()
}
