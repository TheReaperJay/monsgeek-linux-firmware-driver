use std::future::Future;
use std::pin::Pin;

use anyhow::Result;
use clap::Parser;
use monsgeek_cli::{
    Cli, Commands, LedCommands, ProfileCommands, RawCommands, commands, device_select,
};
use monsgeek_driver::pb::driver::{
    Device, DeviceList, DeviceListChangeType, DeviceType, DjDev, dj_dev,
};
use monsgeek_protocol::{DeviceDefinition, DeviceRegistry, cmd};

fn load_registry() -> DeviceRegistry {
    device_select::load_registry().expect("registry should load for smoke tests")
}

fn fixture_device(path: &str, definition: &DeviceDefinition) -> DjDev {
    DjDev {
        oneof_dev: Some(dj_dev::OneofDev::Dev(Device {
            dev_type: DeviceType::YzwKeyboard as i32,
            is24: false,
            path: path.to_string(),
            id: definition.id,
            battery: 100,
            is_online: true,
            vid: definition.vid as u32,
            pid: definition.pid as u32,
            usb_location: path.to_string(),
            canonical_pid: definition.pid as u32,
            connection_mode: "usb".to_string(),
        })),
    }
}

fn fixture_init(definitions: &[(&str, &DeviceDefinition)]) -> DeviceList {
    DeviceList {
        dev_list: definitions
            .iter()
            .map(|(path, definition)| fixture_device(path, definition))
            .collect(),
        r#type: DeviceListChangeType::Init as i32,
    }
}

fn first_two_definitions(registry: &DeviceRegistry) -> (DeviceDefinition, DeviceDefinition) {
    let mut all: Vec<DeviceDefinition> = registry.all_devices().cloned().collect();
    all.sort_by_key(|definition| definition.id);
    assert!(
        all.len() >= 2,
        "registry must contain at least two definitions"
    );
    (all[0].clone(), all[1].clone())
}

#[test]
fn parser_recognizes_all_required_subcommands_and_selector_flags() {
    let selector_parse = Cli::try_parse_from([
        "monsgeek-cli",
        "--endpoint",
        "http://127.0.0.1:3814",
        "--path",
        "usb-b003-p1.2",
        "--usb-location",
        "usb-b003-p1.2",
        "--device-id",
        "1308",
        "--model",
        "monsgeek-m5w",
        "--json",
        "info",
    ]);
    assert!(
        selector_parse.is_ok(),
        "global selector flags should parse with info command"
    );

    let cases: [Vec<&str>; 14] = [
        vec!["monsgeek-cli", "devices", "list"],
        vec!["monsgeek-cli", "info"],
        vec!["monsgeek-cli", "led", "get"],
        vec![
            "monsgeek-cli",
            "led",
            "set",
            "--mode",
            "1",
            "--speed",
            "2",
            "--brightness",
            "3",
            "--dazzle",
            "--r",
            "4",
            "--g",
            "5",
            "--b",
            "6",
        ],
        vec!["monsgeek-cli", "debounce", "get"],
        vec!["monsgeek-cli", "debounce", "set", "--value", "5"],
        vec!["monsgeek-cli", "poll", "get"],
        vec!["monsgeek-cli", "poll", "set", "--value", "1"],
        vec!["monsgeek-cli", "profile", "get"],
        vec!["monsgeek-cli", "profile", "set", "--value", "2"],
        vec![
            "monsgeek-cli",
            "keymap",
            "get",
            "--profile",
            "0",
            "--key-index",
            "9",
        ],
        vec![
            "monsgeek-cli",
            "keymap",
            "set",
            "--profile",
            "0",
            "--key-index",
            "9",
            "--layer",
            "1",
            "--config-type",
            "2",
            "--b1",
            "3",
            "--b2",
            "4",
            "--b3",
            "5",
        ],
        vec![
            "monsgeek-cli",
            "macro",
            "get",
            "--macro-index",
            "1",
            "--page",
            "0",
        ],
        vec!["monsgeek-cli", "raw", "read"],
    ];

    for args in &cases {
        assert!(
            Cli::try_parse_from(args).is_ok(),
            "parser should accept args: {:?}",
            args
        );
    }

    assert!(
        Cli::try_parse_from([
            "monsgeek-cli",
            "macro",
            "set",
            "--macro-index",
            "1",
            "--page",
            "0",
            "--is-last",
            "1",
            "0xAA",
            "0xBB",
        ])
        .is_ok()
    );

    assert!(
        Cli::try_parse_from(["monsgeek-cli", "raw", "send", "0x8F"]).is_ok(),
        "raw send parser should accept bytes"
    );
}

#[test]
fn raw_write_requires_unsafe_flag() {
    let registry = load_registry();
    let definition = registry
        .find_by_id(1308)
        .expect("m5w definition exists")
        .clone();

    let err = commands::build_command_request(
        &Commands::Raw {
            command: RawCommands::Send {
                bytes: vec![0x07, 0x01],
            },
        },
        &definition,
        false,
    )
    .expect_err("write opcode (<0x80) must be blocked without --unsafe");

    assert!(err.to_string().contains("--unsafe"));
}

#[test]
fn single_device_auto_selection_succeeds() {
    let registry = load_registry();
    let definition = registry
        .find_by_id(1308)
        .expect("m5w definition exists")
        .clone();
    let init = fixture_init(&[("path-m5w", &definition)]);
    let online = device_select::supported_online_devices(&init, &registry);

    let selected = device_select::resolve_target_device(
        device_select::SelectorOptions::default(),
        &online,
        &registry,
    )
    .expect("single online supported device should be auto-selected");

    assert_eq!(selected.path, "path-m5w");
    assert_eq!(selected.device_id, 1308);
}

#[test]
fn multiple_devices_requires_selector() {
    let registry = load_registry();
    let (a, b) = first_two_definitions(&registry);
    let init = fixture_init(&[("path-a", &a), ("path-b", &b)]);
    let online = device_select::supported_online_devices(&init, &registry);

    let err = device_select::resolve_target_device(
        device_select::SelectorOptions::default(),
        &online,
        &registry,
    )
    .expect_err("multiple devices must force explicit selector");

    let text = err.to_string();
    assert!(text.contains("--path"));
    assert!(text.contains("--usb-location"));
    assert!(text.contains("--device-id"));
    assert!(text.contains("--model"));
}

#[test]
fn model_selector_filters_by_registry_slug_name() {
    let registry = load_registry();
    let (a, b) = first_two_definitions(&registry);
    let target_slug = device_select::preferred_model_slug(&b);
    let init = fixture_init(&[("path-a", &a), ("path-b", &b)]);
    let online = device_select::supported_online_devices(&init, &registry);

    let selected = device_select::resolve_target_device(
        device_select::SelectorOptions {
            path: None,
            usb_location: None,
            device_id: None,
            model: Some(target_slug.as_str()),
        },
        &online,
        &registry,
    )
    .expect("model selector should filter to matching registry entry");

    assert_eq!(selected.device_id, b.id);
}

#[derive(Default)]
struct StubTransport {
    sends: Vec<(String, Vec<u8>, i32)>,
    read_payload: Vec<u8>,
}

impl commands::DriverTransport for StubTransport {
    fn send_msg(
        &mut self,
        device_path: String,
        msg_bytes: Vec<u8>,
        checksum_enum_i32: i32,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        self.sends.push((device_path, msg_bytes, checksum_enum_i32));
        Box::pin(async { Ok(()) })
    }

    fn read_msg(
        &mut self,
        _device_path: String,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + '_>> {
        let payload = self.read_payload.clone();
        Box::pin(async move { Ok(payload) })
    }
}

#[tokio::test]
async fn smoke_command_framing_with_stubbed_transport() {
    let registry = load_registry();
    let definition = registry
        .find_by_id(1308)
        .expect("m5w definition exists")
        .clone();
    let init = fixture_init(&[("path-m5w", &definition)]);
    let online = device_select::supported_online_devices(&init, &registry);
    let target = device_select::resolve_target_device(
        device_select::SelectorOptions::default(),
        &online,
        &registry,
    )
    .expect("single device should resolve");

    let mut stub = StubTransport {
        sends: Vec::new(),
        read_payload: vec![0xAA],
    };

    let _ = commands::execute_command(&mut stub, &target, &Commands::Info, false)
        .await
        .expect("info execution should succeed");
    assert_eq!(stub.sends[0].1[0], 0x8F, "info must send GET_USB_VERSION");

    let mut stub = StubTransport::default();
    let _ = commands::execute_command(
        &mut stub,
        &target,
        &Commands::Led {
            command: LedCommands::Set {
                mode: 1,
                speed: 2,
                brightness: 3,
                dazzle: true,
                r: 4,
                g: 5,
                b: 6,
            },
        },
        false,
    )
    .await
    .expect("led set execution should succeed");
    assert_eq!(stub.sends[0].1[0], 0x07, "led set must send SET_LEDPARAM");
    assert_eq!(
        stub.sends[0].2,
        monsgeek_driver::pb::driver::CheckSumType::Bit8 as i32,
        "led set must use Bit8 checksum enum"
    );

    let mut stub = StubTransport::default();
    let _ = commands::execute_command(
        &mut stub,
        &target,
        &Commands::Profile {
            command: ProfileCommands::Set { value: 1 },
        },
        false,
    )
    .await
    .expect("profile set execution should succeed");
    assert_eq!(
        stub.sends[0].1[0],
        target.definition.commands().set_profile,
        "profile set should use definition.commands().set_profile"
    );
}

#[test]
fn info_mapping_uses_get_usb_version_byte() {
    let registry = load_registry();
    let definition = registry
        .find_by_id(1308)
        .expect("m5w definition exists")
        .clone();

    let plan = commands::build_command_request(&Commands::Info, &definition, false).unwrap();
    assert_eq!(plan.request.expect("info request")[0], cmd::GET_USB_VERSION);
}
