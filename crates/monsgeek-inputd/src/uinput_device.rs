use evdev::{AttributeSet, BusType, EventType, InputEvent, InputId, KeyCode};
use monsgeek_transport::input::KeyAction;
use monsgeek_transport::keymap::all_keycodes;

const DEFAULT_VIRTUAL_DEVICE_VERSION: u16 = 0x0111;

/// Identity metadata exposed by the uinput virtual keyboard.
///
/// Using the physical device VID/PID avoids the evdev crate's placeholder
/// 0x1234:0x5678 identifiers, which confuse downstream tooling and make the
/// synthetic keyboard look unrelated to the board it represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualKeyboardIdentity {
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
}

impl VirtualKeyboardIdentity {
    pub fn new(vendor: u16, product: u16) -> Self {
        Self {
            vendor,
            product,
            version: DEFAULT_VIRTUAL_DEVICE_VERSION,
        }
    }

    fn input_id(self) -> InputId {
        InputId::new(BusType::BUS_USB, self.vendor, self.product, self.version)
    }
}

/// Create a uinput virtual keyboard device with all keycodes currently mapped
/// by the transport keymap table.
/// The device has a distinct name so it is distinguishable from the physical keyboard.
pub fn create_uinput_device(
    device_name: &str,
    identity: VirtualKeyboardIdentity,
) -> std::io::Result<evdev::uinput::VirtualDevice> {
    let mut keys = AttributeSet::<KeyCode>::new();
    for keycode in all_keycodes() {
        keys.insert(KeyCode::new(keycode));
    }

    evdev::uinput::VirtualDevice::builder()?
        .input_id(identity.input_id())
        .name(device_name)
        .with_keys(&keys)?
        .build()
}

/// Convert a KeyAction (Linux keycode + press/release) to an evdev InputEvent.
pub fn key_action_to_input_event(action: &KeyAction) -> InputEvent {
    InputEvent::new(EventType::KEY.0, action.keycode, action.value)
}

/// Convert a batch of KeyActions to evdev InputEvents.
pub fn key_actions_to_input_events(actions: &[KeyAction]) -> Vec<InputEvent> {
    actions.iter().map(key_action_to_input_event).collect()
}

/// Emit a batch of key events through the virtual device.
/// evdev's emit() auto-appends SYN_REPORT.
pub fn emit_actions(
    device: &mut evdev::uinput::VirtualDevice,
    actions: &[KeyAction],
) -> std::io::Result<()> {
    if actions.is_empty() {
        return Ok(());
    }
    let events = key_actions_to_input_events(actions);
    device.emit(&events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_keyboard_identity_uses_real_vid_pid() {
        let identity = VirtualKeyboardIdentity::new(0x3151, 0x4015);
        let input_id = identity.input_id();
        assert_eq!(input_id.vendor(), 0x3151);
        assert_eq!(input_id.product(), 0x4015);
        assert_eq!(input_id.version(), DEFAULT_VIRTUAL_DEVICE_VERSION);
        assert_eq!(input_id.bus_type(), BusType::BUS_USB);
    }

    #[test]
    fn test_key_action_to_input_event_press() {
        let action = KeyAction {
            keycode: 30, // KEY_A
            value: 1,    // press
        };
        let event = key_action_to_input_event(&action);
        assert_eq!(event.event_type(), EventType::KEY);
        assert_eq!(event.code(), 30);
        assert_eq!(event.value(), 1);
    }

    #[test]
    fn test_key_action_to_input_event_release() {
        let action = KeyAction {
            keycode: 30, // KEY_A
            value: 0,    // release
        };
        let event = key_action_to_input_event(&action);
        assert_eq!(event.event_type(), EventType::KEY);
        assert_eq!(event.code(), 30);
        assert_eq!(event.value(), 0);
    }

    #[test]
    fn test_key_actions_to_input_events_batch() {
        let actions = vec![
            KeyAction {
                keycode: 30,
                value: 1,
            }, // A press
            KeyAction {
                keycode: 48,
                value: 1,
            }, // B press
            KeyAction {
                keycode: 30,
                value: 0,
            }, // A release
        ];
        let events = key_actions_to_input_events(&actions);
        assert_eq!(events.len(), 3);

        assert_eq!(events[0].event_type(), EventType::KEY);
        assert_eq!(events[0].code(), 30);
        assert_eq!(events[0].value(), 1);

        assert_eq!(events[1].event_type(), EventType::KEY);
        assert_eq!(events[1].code(), 48);
        assert_eq!(events[1].value(), 1);

        assert_eq!(events[2].event_type(), EventType::KEY);
        assert_eq!(events[2].code(), 30);
        assert_eq!(events[2].value(), 0);
    }

    #[test]
    fn test_key_actions_to_input_events_empty() {
        let events = key_actions_to_input_events(&[]);
        assert!(events.is_empty());
    }

    #[cfg(feature = "hardware")]
    #[test]
    fn test_create_uinput_device() {
        let device = create_uinput_device(
            "monsgeek-inputd-test",
            VirtualKeyboardIdentity::new(0x3151, 0x4015),
        );
        assert!(
            device.is_ok(),
            "Failed to create uinput device: {:?}",
            device.err()
        );
    }
}
