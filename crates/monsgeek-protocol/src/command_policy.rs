//! Registry-driven outbound command policy.
//!
//! This module centralizes command safety semantics and capability gating using
//! [`DeviceDefinition`] data. Callers evaluate an outbound command once, then
//! execute the returned decision (forward to transport, synthesize read token,
//! and/or surface an error).

use crate::{cmd, DeviceDefinition};

const MAX_MACRO_INDEX: u8 = 49;
const MAX_CHUNK_PAGE: u8 = 9;
const MAGNETIC_WRITE_CMDS: [u8; 5] = [
    cmd::SET_MAGNETISM_REPORT,
    cmd::SET_MAGNETISM_CAL,
    cmd::SET_KEY_MAGNETISM_MODE,
    cmd::SET_MAGNETISM_MAX_CAL,
    cmd::SET_MULTI_MAGNETISM,
];

/// High-level command classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandClass {
    Query,
    Write,
}

/// Whether command bytes should be forwarded to transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandDispatchPolicy {
    ForwardToTransport,
    SkipTransport,
}

/// How the read side should complete after this command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandReadPolicy {
    DeviceResponse,
    SyntheticEmptyRead,
}

/// Transport-agnostic error code for policy violations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandPolicyErrorCode {
    InvalidArgument,
    FailedPrecondition,
}

/// Policy violation details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPolicyError {
    pub code: CommandPolicyErrorCode,
    pub message: String,
}

/// Decision returned by [`evaluate_outbound_command`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundCommandDecision {
    pub class: CommandClass,
    pub dispatch: CommandDispatchPolicy,
    pub read_policy: CommandReadPolicy,
    pub error: Option<CommandPolicyError>,
}

impl OutboundCommandDecision {
    fn forward(class: CommandClass) -> Self {
        let read_policy = match class {
            CommandClass::Write => CommandReadPolicy::SyntheticEmptyRead,
            CommandClass::Query => CommandReadPolicy::DeviceResponse,
        };
        Self {
            class,
            dispatch: CommandDispatchPolicy::ForwardToTransport,
            read_policy,
            error: None,
        }
    }

    fn blocked_ok(class: CommandClass) -> Self {
        Self {
            class,
            dispatch: CommandDispatchPolicy::SkipTransport,
            read_policy: CommandReadPolicy::SyntheticEmptyRead,
            error: None,
        }
    }

    fn rejected(class: CommandClass, code: CommandPolicyErrorCode, message: String) -> Self {
        Self {
            class,
            dispatch: CommandDispatchPolicy::SkipTransport,
            read_policy: CommandReadPolicy::SyntheticEmptyRead,
            error: Some(CommandPolicyError { code, message }),
        }
    }
}

fn max_profile_for_device(definition: &DeviceDefinition) -> Option<u8> {
    definition
        .layer
        .map(|layer_count| layer_count.saturating_sub(1))
}

fn max_fn_sys_for_device(definition: &DeviceDefinition) -> Option<u8> {
    definition
        .fn_sys_layer
        .as_ref()
        .map(|layer| layer.win.max(layer.mac).saturating_sub(1))
}

fn is_magnetic_write_cmd(cmd_byte: u8) -> bool {
    MAGNETIC_WRITE_CMDS.contains(&cmd_byte)
}

fn validate_key_and_layer(
    definition: &DeviceDefinition,
    key_index: u16,
    layer: u8,
) -> Result<(), String> {
    let max_keys = definition.key_count.ok_or_else(|| {
        format!(
            "bounds violation: key_index {} exceeds max 0, layer {} exceeds max 0",
            key_index, layer
        )
    })? as u16;
    let max_layers = definition.layer.ok_or_else(|| {
        format!(
            "bounds violation: key_index {} exceeds max {}, layer {} exceeds max 0",
            key_index, max_keys, layer
        )
    })?;

    if key_index >= max_keys || layer >= max_layers {
        return Err(format!(
            "bounds violation: key_index {} exceeds max {}, layer {} exceeds max {}",
            key_index, max_keys, layer, max_layers
        ));
    }

    Ok(())
}

fn classify_command(definition: &DeviceDefinition, cmd_byte: u8) -> CommandClass {
    let table = definition.commands();
    let is_known_write = cmd_byte == table.set_reset
        || cmd_byte == table.set_profile
        || cmd_byte == table.set_debounce
        || cmd_byte == table.set_keymatrix
        || cmd_byte == table.set_macro
        || table.set_keymatrix_simple.is_some_and(|c| c == cmd_byte)
        || table.set_fn_simple.is_some_and(|c| c == cmd_byte)
        || table.set_report.is_some_and(|c| c == cmd_byte)
        || table.set_kboption.is_some_and(|c| c == cmd_byte)
        || table.set_sleeptime.is_some_and(|c| c == cmd_byte)
        || [
            cmd::SET_LEDPARAM,
            cmd::SET_SLEDPARAM,
            cmd::SET_USERPIC,
            cmd::SET_AUDIO_VIZ,
            cmd::SET_SCREEN_COLOR,
            cmd::SET_REPORT,
            cmd::SET_FN,
            cmd::SET_USERGIF,
            cmd::SET_AUTOOS_EN,
            cmd::SET_MAGNETISM_REPORT,
            cmd::SET_MAGNETISM_CAL,
            cmd::SET_KEY_MAGNETISM_MODE,
            cmd::SET_MAGNETISM_MAX_CAL,
            cmd::SET_MULTI_MAGNETISM,
            cmd::SET_CTRL_BYTE,
            cmd::ENTER_PAIRING,
            cmd::PAIRING_CMD,
            cmd::LED_STREAM,
        ]
        .contains(&cmd_byte);

    if is_known_write {
        CommandClass::Write
    } else {
        CommandClass::Query
    }
}

/// Evaluate outbound command policy for a device and raw command frame.
///
/// Compatibility behavior:
/// - Unsupported magnetic writes are blocked with synthetic read completion but
///   treated as send-success (`error: None`) to preserve browser workflows.
/// - Rejected unsafe writes (invalid payload/bounds) are blocked, return an
///   error, and still use synthetic read completion to avoid send/read hangs.
pub fn evaluate_outbound_command(
    definition: &DeviceDefinition,
    msg: &[u8],
) -> OutboundCommandDecision {
    if msg.is_empty() {
        return OutboundCommandDecision::forward(CommandClass::Query);
    }

    let cmd_byte = msg[0];
    let class = classify_command(definition, cmd_byte);
    let commands = definition.commands();

    if cmd_byte == commands.set_profile {
        if msg.len() < 2 {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                "SET_PROFILE payload too short: need at least 2 bytes".to_string(),
            );
        }

        let Some(max_profile) = max_profile_for_device(definition) else {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::FailedPrecondition,
                format!(
                    "SET_PROFILE rejected: device {} missing required registry field: layer",
                    definition.display_name
                ),
            );
        };

        let profile = msg[1];
        if profile > max_profile {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                format!(
                    "SET_PROFILE profile {} exceeds max {}",
                    profile, max_profile
                ),
            );
        }
    }

    if cmd_byte == commands.set_keymatrix {
        if msg.len() < 7 {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                "SET_KEYMATRIX payload too short: need at least 7 bytes".to_string(),
            );
        }

        let profile = msg[1];
        let key_index = msg[2] as u16;
        let layer = msg[6];
        let Some(max_profile) = max_profile_for_device(definition) else {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::FailedPrecondition,
                format!(
                    "SET_KEYMATRIX rejected: device {} missing required registry field: layer",
                    definition.display_name
                ),
            );
        };

        if profile > max_profile {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                format!(
                    "SET_KEYMATRIX profile {} exceeds max {}",
                    profile, max_profile
                ),
            );
        }

        if let Err(err) = validate_key_and_layer(definition, key_index, layer) {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                err,
            );
        }
    }

    if commands.set_keymatrix_simple.is_some_and(|c| c == cmd_byte) {
        if msg.len() < 3 {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                "SET_KEYMATRIX_SIMPLE payload too short: need at least 3 bytes".to_string(),
            );
        }

        let profile = msg[1];
        let key_index = msg[2] as u16;
        let Some(max_profile) = max_profile_for_device(definition) else {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::FailedPrecondition,
                format!(
                    "SET_KEYMATRIX_SIMPLE rejected: device {} missing required registry field: layer",
                    definition.display_name
                ),
            );
        };

        if profile > max_profile {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                format!(
                    "SET_KEYMATRIX_SIMPLE profile {} exceeds max {}",
                    profile, max_profile
                ),
            );
        }

        if let Err(err) = validate_key_and_layer(definition, key_index, 0) {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                err,
            );
        }
    }

    if commands.set_fn_simple.is_some_and(|c| c == cmd_byte) {
        if msg.len() < 3 {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                "SET_FN_SIMPLE payload too short: need at least 3 bytes".to_string(),
            );
        }

        let profile = msg[1];
        let key_index = msg[2] as u16;
        let Some(max_profile) = max_profile_for_device(definition) else {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::FailedPrecondition,
                format!(
                    "SET_FN_SIMPLE rejected: device {} missing required registry field: layer",
                    definition.display_name
                ),
            );
        };

        if profile > max_profile {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                format!(
                    "SET_FN_SIMPLE profile {} exceeds max {}",
                    profile, max_profile
                ),
            );
        }

        if let Err(err) = validate_key_and_layer(definition, key_index, 0) {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                err,
            );
        }
    }

    if cmd_byte == commands.set_macro {
        if msg.len() < 3 {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                "SET_MACRO payload too short: need at least 3 bytes".to_string(),
            );
        }

        let macro_index = msg[1];
        let chunk_page = msg[2];

        if macro_index > MAX_MACRO_INDEX {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                format!(
                    "SET_MACRO macro_index {} exceeds max {}",
                    macro_index, MAX_MACRO_INDEX
                ),
            );
        }

        if chunk_page > MAX_CHUNK_PAGE {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                format!(
                    "SET_MACRO chunk_page {} exceeds max {}",
                    chunk_page, MAX_CHUNK_PAGE
                ),
            );
        }
    }

    if cmd_byte == cmd::SET_FN {
        if msg.len() < 4 {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                "SET_FN payload too short: need at least 4 bytes".to_string(),
            );
        }

        let Some(max_fn_sys) = max_fn_sys_for_device(definition) else {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::FailedPrecondition,
                format!(
                    "SET_FN rejected: device {} missing required registry field: fnSysLayer",
                    definition.display_name
                ),
            );
        };
        let Some(max_profile) = max_profile_for_device(definition) else {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::FailedPrecondition,
                format!(
                    "SET_FN rejected: device {} missing required registry field: layer",
                    definition.display_name
                ),
            );
        };

        let fn_sys = msg[1];
        let profile = msg[2];
        let key_index = msg[3] as u16;

        if fn_sys > max_fn_sys {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                format!("SET_FN fn_sys {} exceeds max {}", fn_sys, max_fn_sys),
            );
        }

        if profile > max_profile {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                format!("SET_FN profile {} exceeds max {}", profile, max_profile),
            );
        }

        if let Err(err) = validate_key_and_layer(definition, key_index, 0) {
            return OutboundCommandDecision::rejected(
                class,
                CommandPolicyErrorCode::InvalidArgument,
                err,
            );
        }
    }

    if is_magnetic_write_cmd(cmd_byte) && !definition.has_magnetism() {
        return OutboundCommandDecision::blocked_ok(class);
    }

    OutboundCommandDecision::forward(class)
}

/// Normalize outbound command payloads for known web-compat quirks.
///
/// For YiChip SIMPLE keymatrix/FN writes, the web app reset pattern encodes
/// config as `[0, 0, keycode, 0]` but firmware expects keycode at `config[1]`.
/// This transform rewrites to `[0, keycode, 0, 0]`.
pub fn normalize_outbound_command(definition: &DeviceDefinition, mut msg: Vec<u8>) -> Vec<u8> {
    if msg.len() < 12 {
        return msg;
    }

    let cmd_byte = msg[0];
    let commands = definition.commands();

    let is_simple = commands.set_keymatrix_simple.is_some_and(|c| c == cmd_byte)
        || commands.set_fn_simple.is_some_and(|c| c == cmd_byte);

    if !is_simple {
        return msg;
    }

    let config = &msg[8..12];
    if config[0] == 0 && config[1] == 0 && config[2] != 0 && config[3] == 0 {
        msg[9] = msg[10];
        msg[10] = 0;
    }

    msg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ControlTransport;

    fn make_test_definition() -> DeviceDefinition {
        DeviceDefinition {
            id: 9999,
            vid: 0x3151,
            pid: 0x4015,
            runtime_pids: vec![],
            name: "yc3121_test".to_string(),
            display_name: "Test".to_string(),
            company: None,
            device_type: "keyboard".to_string(),
            control_transport: ControlTransport::Direct,
            sources: vec![],
            key_count: Some(108),
            key_layout_name: None,
            layer: Some(4),
            fn_sys_layer: None,
            magnetism: None,
            no_magnetic_switch: Some(true),
            has_light_layout: None,
            has_side_light: None,
            hot_swap: None,
            travel_setting: None,
            led_matrix: None,
            chip_family: None,
            command_overrides: None,
        }
    }

    #[test]
    fn magnetic_write_blocked_with_synthetic_read_and_no_error() {
        let def = make_test_definition();
        let msg = [cmd::SET_MAGNETISM_CAL, 0, 0, 0];
        let decision = evaluate_outbound_command(&def, &msg);
        assert_eq!(decision.dispatch, CommandDispatchPolicy::SkipTransport);
        assert_eq!(decision.read_policy, CommandReadPolicy::SyntheticEmptyRead);
        assert!(decision.error.is_none());
    }

    #[test]
    fn invalid_set_profile_returns_error_and_synthetic_read() {
        let def = make_test_definition();
        let msg = [def.commands().set_profile, 9];
        let decision = evaluate_outbound_command(&def, &msg);
        assert_eq!(decision.dispatch, CommandDispatchPolicy::SkipTransport);
        assert_eq!(decision.read_policy, CommandReadPolicy::SyntheticEmptyRead);
        let err = decision.error.expect("expected policy error");
        assert_eq!(err.code, CommandPolicyErrorCode::InvalidArgument);
        assert!(err.message.contains("SET_PROFILE profile"));
    }

    #[test]
    fn valid_set_profile_forwards() {
        let def = make_test_definition();
        let msg = [def.commands().set_profile, 2];
        let decision = evaluate_outbound_command(&def, &msg);
        assert_eq!(decision.dispatch, CommandDispatchPolicy::ForwardToTransport);
        assert_eq!(decision.read_policy, CommandReadPolicy::SyntheticEmptyRead);
        assert!(decision.error.is_none());
    }

    #[test]
    fn query_keeps_device_read_policy() {
        let def = make_test_definition();
        let msg = [def.commands().get_profile];
        let decision = evaluate_outbound_command(&def, &msg);
        assert_eq!(decision.class, CommandClass::Query);
        assert_eq!(decision.dispatch, CommandDispatchPolicy::ForwardToTransport);
        assert_eq!(decision.read_policy, CommandReadPolicy::DeviceResponse);
        assert!(decision.error.is_none());
    }

    #[test]
    fn set_profile_missing_layer_is_failed_precondition() {
        let mut def = make_test_definition();
        def.layer = None;
        let msg = [def.commands().set_profile, 0];
        let decision = evaluate_outbound_command(&def, &msg);
        let err = decision.error.expect("expected policy error");
        assert_eq!(err.code, CommandPolicyErrorCode::FailedPrecondition);
        assert!(err
            .message
            .contains("missing required registry field: layer"));
    }

    #[test]
    fn set_fn_missing_fn_sys_layer_is_failed_precondition() {
        let def = make_test_definition();
        let msg = [cmd::SET_FN, 0, 0, 0];
        let decision = evaluate_outbound_command(&def, &msg);
        let err = decision.error.expect("expected policy error");
        assert_eq!(err.code, CommandPolicyErrorCode::FailedPrecondition);
        assert!(err
            .message
            .contains("missing required registry field: fnSysLayer"));
    }

    #[test]
    fn normalize_simple_reset_pattern_rewrites_config() {
        let def = make_test_definition();
        let cmd = def
            .commands()
            .set_keymatrix_simple
            .expect("YiChip simple cmd");
        let mut msg = vec![0u8; 64];
        msg[0] = cmd;
        msg[8] = 0;
        msg[9] = 0;
        msg[10] = 7;
        msg[11] = 0;
        let out = normalize_outbound_command(&def, msg);
        assert_eq!(out[8..12], [0, 7, 0, 0]);
    }
}
