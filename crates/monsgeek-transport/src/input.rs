use std::collections::HashMap;
use std::time::Instant;

use crate::keymap::{HID_TO_LINUX, MODIFIER_KEYCODES};

/// Abstract key action output from HID report processing.
/// Later layers can map these to input-subsystem events when userspace-input
/// mode is intentionally active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyAction {
    pub keycode: u16,
    pub value: i32,
}

/// Pure HID report processor with state tracking and software debounce.
///
/// Translates 8-byte boot protocol HID reports into key press/release actions.
/// Tracks modifier bitmap and 6-key array state across reports.
/// Emits releases before presses for correct rollover handling.
pub struct InputProcessor {
    prev_modifiers: u8,
    prev_keys: [u8; 6],
    debounce_ms: u64,
    release_times: HashMap<u8, Instant>,
}

impl InputProcessor {
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            prev_modifiers: 0,
            prev_keys: [0; 6],
            debounce_ms,
            release_times: HashMap::new(),
        }
    }

    /// Process an 8-byte boot protocol HID report, using the current time for debounce.
    pub fn process_report(&mut self, report: &[u8]) -> Vec<KeyAction> {
        self.process_report_at(report, Instant::now())
    }

    /// Process an 8-byte boot protocol HID report with an explicit timestamp.
    /// Enables deterministic debounce testing without sleeping.
    pub fn process_report_at(&mut self, report: &[u8], now: Instant) -> Vec<KeyAction> {
        if report.len() < 8 {
            return Vec::new();
        }

        let modifiers = report[0];
        let keys: [u8; 6] = [
            report[2], report[3], report[4], report[5], report[6], report[7],
        ];

        let mut actions = Vec::new();

        // Process modifier releases (emit before presses for correct ordering)
        for bit in 0..8u8 {
            let mask = 1 << bit;
            let was_pressed = self.prev_modifiers & mask != 0;
            let is_pressed = modifiers & mask != 0;

            if was_pressed && !is_pressed {
                let keycode = MODIFIER_KEYCODES[bit as usize];
                log::trace!("modifier release: bit {} -> keycode {}", bit, keycode);
                actions.push(KeyAction { keycode, value: 0 });
            }
        }

        // Process modifier presses
        for bit in 0..8u8 {
            let mask = 1 << bit;
            let was_pressed = self.prev_modifiers & mask != 0;
            let is_pressed = modifiers & mask != 0;

            if !was_pressed && is_pressed {
                let keycode = MODIFIER_KEYCODES[bit as usize];
                log::trace!("modifier press: bit {} -> keycode {}", bit, keycode);
                actions.push(KeyAction { keycode, value: 1 });
            }
        }

        // Process key releases FIRST: HID codes in prev_keys but NOT in current keys
        for &prev_hid in &self.prev_keys {
            if prev_hid == 0 {
                continue;
            }
            if !keys.contains(&prev_hid) {
                let keycode = HID_TO_LINUX[prev_hid as usize];
                if keycode != 0 {
                    log::trace!("key release: HID {:#04X} -> keycode {}", prev_hid, keycode);
                    actions.push(KeyAction { keycode, value: 0 });
                }
                self.release_times.insert(prev_hid, now);
            }
        }

        // Process key presses SECOND: HID codes in current keys but NOT in prev_keys
        // Build the new prev_keys from current keys, skipping debounce-suppressed ones
        let mut new_prev_keys = [0u8; 6];
        for (i, &hid) in keys.iter().enumerate() {
            if hid == 0 {
                continue;
            }

            if self.prev_keys.contains(&hid) {
                // Key was already held, carry forward
                new_prev_keys[i] = hid;
                continue;
            }

            // New key -- check debounce
            let keycode = HID_TO_LINUX[hid as usize];
            if keycode == 0 {
                // Unmapped: still track in prev_keys to avoid re-processing
                new_prev_keys[i] = hid;
                continue;
            }

            if self.debounce_ms > 0 {
                if let Some(&release_time) = self.release_times.get(&hid) {
                    let elapsed = now.duration_since(release_time);
                    if elapsed.as_millis() < self.debounce_ms as u128 {
                        log::debug!(
                            "debounce: suppressed keycode {} re-press, {}ms since release",
                            keycode,
                            elapsed.as_millis()
                        );
                        // Do NOT add to new_prev_keys so next report re-checks debounce
                        continue;
                    }
                }
            }

            log::trace!("key press: HID {:#04X} -> keycode {}", hid, keycode);
            actions.push(KeyAction { keycode, value: 1 });
            new_prev_keys[i] = hid;
        }

        self.prev_modifiers = modifiers;
        self.prev_keys = new_prev_keys;

        actions
    }

    /// Release all currently tracked keys and modifiers.
    /// Returns release KeyActions for every held key and modifier, then resets state.
    pub fn release_all_keys(&mut self) -> Vec<KeyAction> {
        let mut actions = Vec::new();

        // Release all held regular keys
        for &hid in &self.prev_keys {
            if hid == 0 {
                continue;
            }
            let keycode = HID_TO_LINUX[hid as usize];
            if keycode != 0 {
                log::trace!("release_all: keycode {}", keycode);
                actions.push(KeyAction { keycode, value: 0 });
            }
        }

        // Release all held modifiers
        for bit in 0..8u8 {
            if self.prev_modifiers & (1 << bit) != 0 {
                let keycode = MODIFIER_KEYCODES[bit as usize];
                log::trace!("release_all: modifier keycode {}", keycode);
                actions.push(KeyAction { keycode, value: 0 });
            }
        }

        self.prev_modifiers = 0;
        self.prev_keys = [0; 6];
        self.release_times.clear();

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // Test 1: Single key press
    #[test]
    fn test_single_key_press() {
        let mut proc = InputProcessor::new(0);
        let report = [0x00, 0x00, 0x04, 0, 0, 0, 0, 0]; // HID 0x04 = KEY_A
        let actions = proc.process_report(&report);

        assert_eq!(actions.len(), 1, "Expected 1 action, got {}", actions.len());
        assert_eq!(actions[0].keycode, 30, "Expected KEY_A (30)");
        assert_eq!(actions[0].value, 1, "Expected press (1)");
    }

    // Test 2: Single key release
    #[test]
    fn test_single_key_release() {
        let mut proc = InputProcessor::new(0);

        // First press A
        let press_report = [0x00, 0x00, 0x04, 0, 0, 0, 0, 0];
        proc.process_report(&press_report);

        // Then release A
        let release_report = [0x00, 0x00, 0, 0, 0, 0, 0, 0];
        let actions = proc.process_report(&release_report);

        assert_eq!(actions.len(), 1, "Expected 1 action, got {}", actions.len());
        assert_eq!(actions[0].keycode, 30, "Expected KEY_A (30)");
        assert_eq!(actions[0].value, 0, "Expected release (0)");
    }

    // Test 3: Modifier press (Left Shift)
    #[test]
    fn test_modifier_press() {
        let mut proc = InputProcessor::new(0);
        let report = [0x02, 0x00, 0, 0, 0, 0, 0, 0]; // bit 1 = Left Shift
        let actions = proc.process_report(&report);

        assert_eq!(actions.len(), 1, "Expected 1 action, got {}", actions.len());
        assert_eq!(actions[0].keycode, 42, "Expected KEY_LEFTSHIFT (42)");
        assert_eq!(actions[0].value, 1, "Expected press (1)");
    }

    // Test 4: Modifier release
    #[test]
    fn test_modifier_release() {
        let mut proc = InputProcessor::new(0);

        // First press Left Shift
        let press_report = [0x02, 0x00, 0, 0, 0, 0, 0, 0];
        proc.process_report(&press_report);

        // Then release it
        let release_report = [0x00, 0x00, 0, 0, 0, 0, 0, 0];
        let actions = proc.process_report(&release_report);

        assert_eq!(actions.len(), 1, "Expected 1 action, got {}", actions.len());
        assert_eq!(actions[0].keycode, 42, "Expected KEY_LEFTSHIFT (42)");
        assert_eq!(actions[0].value, 0, "Expected release (0)");
    }

    // Test 5: Multiple simultaneous modifiers
    #[test]
    fn test_multiple_modifiers() {
        let mut proc = InputProcessor::new(0);
        let report = [0x05, 0x00, 0, 0, 0, 0, 0, 0]; // bits 0+2 = LCtrl+LAlt
        let actions = proc.process_report(&report);

        assert_eq!(
            actions.len(),
            2,
            "Expected 2 actions, got {}",
            actions.len()
        );

        let keycodes: Vec<u16> = actions.iter().map(|a| a.keycode).collect();
        assert!(keycodes.contains(&29), "Missing KEY_LEFTCTRL (29)");
        assert!(keycodes.contains(&56), "Missing KEY_LEFTALT (56)");

        for action in &actions {
            assert_eq!(
                action.value, 1,
                "Expected press (1) for keycode {}",
                action.keycode
            );
        }
    }

    // Test 6: Releases before presses (rollover)
    #[test]
    fn test_releases_before_presses() {
        let mut proc = InputProcessor::new(0);

        // Press A
        let press_a = [0x00, 0x00, 0x04, 0, 0, 0, 0, 0];
        proc.process_report(&press_a);

        // Release A, press B
        let swap = [0x00, 0x00, 0x05, 0, 0, 0, 0, 0];
        let actions = proc.process_report(&swap);

        assert_eq!(
            actions.len(),
            2,
            "Expected 2 actions, got {}",
            actions.len()
        );
        // Release MUST come before press
        assert_eq!(
            actions[0].keycode, 30,
            "First action should be KEY_A (30) release"
        );
        assert_eq!(actions[0].value, 0, "First action should be release (0)");
        assert_eq!(
            actions[1].keycode, 48,
            "Second action should be KEY_B (48) press"
        );
        assert_eq!(actions[1].value, 1, "Second action should be press (1)");
    }

    // Test 7: Unmapped HID scancode ignored
    #[test]
    fn test_unmapped_scancode_ignored() {
        let mut proc = InputProcessor::new(0);
        let report = [0x00, 0x00, 0x01, 0, 0, 0, 0, 0]; // HID 0x01 is unmapped
        let actions = proc.process_report(&report);

        assert!(
            actions.is_empty(),
            "Unmapped scancode should produce no events"
        );
    }

    // Test 8: Six simultaneous keys
    #[test]
    fn test_six_simultaneous_keys() {
        let mut proc = InputProcessor::new(0);
        let report = [0x00, 0x00, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09]; // A,B,C,D,E,F
        let actions = proc.process_report(&report);

        assert_eq!(
            actions.len(),
            6,
            "Expected 6 actions, got {}",
            actions.len()
        );

        let expected_keycodes: Vec<u16> = vec![30, 48, 46, 32, 18, 33];
        let actual_keycodes: Vec<u16> = actions.iter().map(|a| a.keycode).collect();

        for kc in &expected_keycodes {
            assert!(actual_keycodes.contains(kc), "Missing keycode {kc}");
        }

        for action in &actions {
            assert_eq!(
                action.value, 1,
                "Expected press (1) for keycode {}",
                action.keycode
            );
        }
    }

    // Test 9: Debounce suppresses fast re-press
    #[test]
    fn test_debounce_suppresses_fast_repress() {
        let mut proc = InputProcessor::new(10); // 10ms debounce

        let base_time = Instant::now();

        // Press A at t=0
        let press_a = [0x00, 0x00, 0x04, 0, 0, 0, 0, 0];
        proc.process_report_at(&press_a, base_time);

        // Release A at t=2ms
        let release = [0x00, 0x00, 0, 0, 0, 0, 0, 0];
        let t_release = base_time + Duration::from_millis(2);
        proc.process_report_at(&release, t_release);

        // Re-press A at t=5ms (within 10ms debounce window from release)
        let repress = [0x00, 0x00, 0x04, 0, 0, 0, 0, 0];
        let t_repress = base_time + Duration::from_millis(5);
        let actions = proc.process_report_at(&repress, t_repress);

        assert!(
            actions.is_empty(),
            "Re-press within debounce window should be suppressed, got {} actions",
            actions.len()
        );

        // Verify A is NOT in prev_keys (was suppressed)
        assert!(
            !proc.prev_keys.contains(&0x04),
            "Suppressed key should not be in prev_keys"
        );
    }

    // Test 10: Debounce allows press after window
    #[test]
    fn test_debounce_allows_press_after_window() {
        let mut proc = InputProcessor::new(10); // 10ms debounce

        let base_time = Instant::now();

        // Press A at t=0
        let press_a = [0x00, 0x00, 0x04, 0, 0, 0, 0, 0];
        proc.process_report_at(&press_a, base_time);

        // Release A at t=2ms
        let release = [0x00, 0x00, 0, 0, 0, 0, 0, 0];
        let t_release = base_time + Duration::from_millis(2);
        proc.process_report_at(&release, t_release);

        // Re-press A at t=15ms (after 10ms debounce window from release)
        let repress = [0x00, 0x00, 0x04, 0, 0, 0, 0, 0];
        let t_repress = base_time + Duration::from_millis(15);
        let actions = proc.process_report_at(&repress, t_repress);

        assert_eq!(
            actions.len(),
            1,
            "Re-press after debounce window should pass through"
        );
        assert_eq!(actions[0].keycode, 30, "Expected KEY_A (30)");
        assert_eq!(actions[0].value, 1, "Expected press (1)");
    }

    // Test 11: Debounce disabled (0ms)
    #[test]
    fn test_debounce_disabled() {
        let mut proc = InputProcessor::new(0); // Debounce disabled

        let base_time = Instant::now();

        // Press A
        let press_a = [0x00, 0x00, 0x04, 0, 0, 0, 0, 0];
        proc.process_report_at(&press_a, base_time);

        // Release A
        let release = [0x00, 0x00, 0, 0, 0, 0, 0, 0];
        let t_release = base_time + Duration::from_millis(1);
        proc.process_report_at(&release, t_release);

        // Immediate re-press A (should NOT be suppressed)
        let repress = [0x00, 0x00, 0x04, 0, 0, 0, 0, 0];
        let t_repress = base_time + Duration::from_millis(2);
        let actions = proc.process_report_at(&repress, t_repress);

        assert_eq!(
            actions.len(),
            1,
            "With debounce disabled, immediate re-press should pass through"
        );
        assert_eq!(actions[0].keycode, 30, "Expected KEY_A (30)");
        assert_eq!(actions[0].value, 1, "Expected press (1)");
    }

    // Test 12: Release all with keys and modifiers held
    #[test]
    fn test_release_all_keys_with_state() {
        let mut proc = InputProcessor::new(0);

        // Press LCtrl + LShift + A + B
        let report = [0x03, 0x00, 0x04, 0x05, 0, 0, 0, 0];
        proc.process_report(&report);

        // Release all
        let actions = proc.release_all_keys();

        assert_eq!(
            actions.len(),
            4,
            "Expected 4 release actions, got {}",
            actions.len()
        );

        let keycodes: Vec<u16> = actions.iter().map(|a| a.keycode).collect();
        assert!(keycodes.contains(&30), "Missing KEY_A (30) release");
        assert!(keycodes.contains(&48), "Missing KEY_B (48) release");
        assert!(keycodes.contains(&29), "Missing KEY_LEFTCTRL (29) release");
        assert!(keycodes.contains(&42), "Missing KEY_LEFTSHIFT (42) release");

        for action in &actions {
            assert_eq!(
                action.value, 0,
                "All release_all events should be releases (0)"
            );
        }

        // Verify state is reset
        assert_eq!(proc.prev_modifiers, 0, "prev_modifiers should be reset");
        assert_eq!(proc.prev_keys, [0; 6], "prev_keys should be reset");
    }

    // Test 13: Release all with nothing held
    #[test]
    fn test_release_all_keys_empty() {
        let mut proc = InputProcessor::new(0);
        let actions = proc.release_all_keys();

        assert!(
            actions.is_empty(),
            "release_all with no held keys should return empty"
        );
    }

    // Test 14: Short report handling
    #[test]
    fn test_short_report_ignored() {
        let mut proc = InputProcessor::new(0);
        let short_report = [0x00, 0x00, 0x04]; // Only 3 bytes
        let actions = proc.process_report(&short_report);

        assert!(actions.is_empty(), "Short report should produce no events");
    }
}
