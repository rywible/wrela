use crate::input::InputButtons;
use crate::restart_key_policy::is_restart_key;

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(crate) fn handle_key_input_state(
    buttons: &mut InputButtons,
    collect_pressed: &mut bool,
    key: &str,
    pressed: bool,
) -> bool {
    if is_restart_key(key) {
        *collect_pressed = pressed;
        true
    } else {
        buttons.set_key(key, pressed)
    }
}

#[cfg(test)]
mod tests {
    use super::handle_key_input_state;
    use crate::input::InputButtons;

    #[test]
    fn restart_keys_toggle_collect_pressed() {
        let mut buttons = InputButtons::default();
        let mut collect_pressed = false;

        assert!(handle_key_input_state(
            &mut buttons,
            &mut collect_pressed,
            "Enter",
            true
        ));
        assert!(collect_pressed);

        assert!(handle_key_input_state(
            &mut buttons,
            &mut collect_pressed,
            "Enter",
            false
        ));
        assert!(!collect_pressed);
    }

    #[test]
    fn movement_keys_only_update_buttons() {
        let mut buttons = InputButtons::default();
        let mut collect_pressed = false;

        assert!(handle_key_input_state(
            &mut buttons,
            &mut collect_pressed,
            "ArrowUp",
            true
        ));
        assert!(!collect_pressed);
        assert_eq!(
            buttons.bitmask() & InputButtons::BUTTON_UP,
            InputButtons::BUTTON_UP
        );

        assert!(handle_key_input_state(
            &mut buttons,
            &mut collect_pressed,
            "ArrowUp",
            false
        ));
        assert!(!collect_pressed);
        assert_eq!(buttons.bitmask() & InputButtons::BUTTON_UP, 0);
    }

    #[test]
    fn unknown_keys_are_not_handled() {
        let mut buttons = InputButtons::default();
        let mut collect_pressed = false;
        assert!(!handle_key_input_state(
            &mut buttons,
            &mut collect_pressed,
            "x",
            true
        ));
        assert!(!collect_pressed);
        assert_eq!(buttons.bitmask(), 0);
    }
}
