pub mod client;
pub mod commands;
pub mod device_select;
pub mod format;

use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use client::{DEFAULT_ENDPOINT, DriverClient};
use commands::execute_command;
use device_select::{
    SelectorOptions, load_registry, resolve_target_device, supported_online_devices,
};
use format::{print_command_result, print_devices};

#[derive(Debug, Parser, Clone)]
#[command(
    name = "monsgeek-cli",
    about = "Bridge-first MonsGeek command-line client"
)]
pub struct Cli {
    #[arg(long, default_value = DEFAULT_ENDPOINT, value_name = "URL")]
    pub endpoint: String,

    #[arg(long, value_name = "BRIDGE_PATH")]
    pub path: Option<String>,

    #[arg(long, value_name = "USB_LOCATION")]
    pub usb_location: Option<String>,

    #[arg(long, value_name = "FIRMWARE_ID")]
    pub device_id: Option<i32>,

    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,

    #[arg(long)]
    pub json: bool,

    #[arg(long = "unsafe")]
    pub unsafe_mode: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand, Clone)]
pub enum Commands {
    Devices {
        #[command(subcommand)]
        command: DevicesCommands,
    },
    Info,
    Led {
        #[command(subcommand)]
        command: LedCommands,
    },
    Debounce {
        #[command(subcommand)]
        command: DebounceCommands,
    },
    Poll {
        #[command(subcommand)]
        command: PollCommands,
    },
    Profile {
        #[command(subcommand)]
        command: ProfileCommands,
    },
    Keymap {
        #[command(subcommand)]
        command: KeymapCommands,
    },
    Macro {
        #[command(subcommand)]
        command: MacroCommands,
    },
    Raw {
        #[command(subcommand)]
        command: RawCommands,
    },
    Firmware {
        #[command(subcommand)]
        command: FirmwareCommands,
    },
}

#[derive(Debug, Subcommand, Clone)]
pub enum DevicesCommands {
    List,
}

#[derive(Debug, Subcommand, Clone)]
pub enum LedCommands {
    Get,
    Set {
        #[arg(long)]
        mode: u8,
        #[arg(long)]
        speed: u8,
        #[arg(long)]
        brightness: u8,
        #[arg(long)]
        dazzle: bool,
        #[arg(long)]
        r: u8,
        #[arg(long)]
        g: u8,
        #[arg(long)]
        b: u8,
    },
}

#[derive(Debug, Subcommand, Clone)]
pub enum DebounceCommands {
    Get,
    Set {
        #[arg(long)]
        value: u8,
    },
}

#[derive(Debug, Subcommand, Clone)]
pub enum PollCommands {
    Get,
    Set {
        #[arg(long)]
        value: u8,
    },
}

#[derive(Debug, Subcommand, Clone)]
pub enum ProfileCommands {
    Get,
    Set {
        #[arg(long)]
        value: u8,
    },
}

#[derive(Debug, Subcommand, Clone)]
pub enum KeymapCommands {
    Get {
        #[arg(long)]
        profile: u8,
        #[arg(long)]
        key_index: u8,
    },
    Set {
        #[arg(long)]
        profile: u8,
        #[arg(long)]
        key_index: u8,
        #[arg(long)]
        layer: u8,
        #[arg(long)]
        config_type: u8,
        #[arg(long)]
        b1: u8,
        #[arg(long)]
        b2: u8,
        #[arg(long)]
        b3: u8,
    },
}

#[derive(Debug, Subcommand, Clone)]
pub enum MacroCommands {
    Get {
        #[arg(long)]
        macro_index: u8,
        #[arg(long)]
        page: u8,
    },
    Set {
        #[arg(long)]
        macro_index: u8,
        #[arg(long)]
        page: u8,
        #[arg(long)]
        is_last: u8,
        #[arg(value_name = "BYTE", num_args = 0.., value_parser = parse_byte)]
        data: Vec<u8>,
    },
}

#[derive(Debug, Subcommand, Clone)]
pub enum RawCommands {
    Send {
        #[arg(value_name = "BYTE", num_args = 1.., value_parser = parse_byte)]
        bytes: Vec<u8>,
    },
    Read,
}

#[derive(Debug, Subcommand, Clone)]
pub enum FirmwareCommands {
    Version,
    Validate {
        #[arg(long, value_name = "PATH")]
        file: Option<PathBuf>,
        #[arg(long, value_name = "DIR")]
        download_dir: Option<PathBuf>,
        #[arg(long)]
        allow_unofficial: bool,
    },
    Flash {
        #[arg(long, value_name = "PATH")]
        file: Option<PathBuf>,
        #[arg(long, value_name = "DIR")]
        download_dir: Option<PathBuf>,
        #[arg(long)]
        allow_unofficial: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long = "i-understand-firmware-risk")]
        i_understand_firmware_risk: bool,
        #[arg(long)]
        typed_phrase: Option<String>,
        #[arg(long)]
        allow_backup_failure: bool,
    },
}

pub fn parse_byte(input: &str) -> std::result::Result<u8, String> {
    if let Some(hex) = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
    {
        u8::from_str_radix(hex, 16).map_err(|err| err.to_string())
    } else {
        input.parse::<u8>().map_err(|err| err.to_string())
    }
}

pub async fn run(cli: Cli) -> Result<()> {
    let firmware_status = std::io::stderr().is_terminal()
        && matches!(
            &cli.command,
            Commands::Firmware {
                command: FirmwareCommands::Validate { .. } | FirmwareCommands::Flash { .. }
            }
        );

    if firmware_status {
        eprintln!("[firmware] connecting to driver at {}", cli.endpoint);
    }
    let mut client = DriverClient::connect(&cli.endpoint).await?;

    if firmware_status {
        eprintln!("[firmware] requesting initial device list");
    }
    let init = client.watch_dev_list_init().await?;
    let registry = load_registry()?;
    let online_supported = supported_online_devices(&init, &registry);

    match &cli.command {
        Commands::Devices {
            command: DevicesCommands::List,
        } => {
            print_devices(cli.json, &online_supported)?;
            Ok(())
        }
        _ => {
            if firmware_status {
                eprintln!("[firmware] resolving target device");
            }
            let target = resolve_target_device(
                SelectorOptions {
                    path: cli.path.as_deref(),
                    usb_location: cli.usb_location.as_deref(),
                    device_id: cli.device_id,
                    model: cli.model.as_deref(),
                },
                &online_supported,
                &registry,
            )?;

            if firmware_status {
                eprintln!("[firmware] executing {}", cli.command.operation_name());
            }
            let result =
                execute_command(&mut client, &target, &cli.command, cli.unsafe_mode).await?;
            print_command_result(cli.json, &target, &result)?;
            Ok(())
        }
    }
}

impl Commands {
    fn operation_name(&self) -> &'static str {
        match self {
            Commands::Devices { .. } => "devices",
            Commands::Info => "info",
            Commands::Led { .. } => "led",
            Commands::Debounce { .. } => "debounce",
            Commands::Poll { .. } => "poll",
            Commands::Profile { .. } => "profile",
            Commands::Keymap { .. } => "keymap",
            Commands::Macro { .. } => "macro",
            Commands::Raw { .. } => "raw",
            Commands::Firmware { command } => match command {
                FirmwareCommands::Version => "firmware version",
                FirmwareCommands::Validate { .. } => "firmware validate",
                FirmwareCommands::Flash { .. } => "firmware flash",
            },
        }
    }
}
