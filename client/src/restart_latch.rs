#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RestartLatchDecision {
    pub(crate) restart_requested: bool,
    pub(crate) restart_button_latched: bool,
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(crate) fn decide_restart_signal(
    collect_pressed: bool,
    game_won: bool,
    restart_button_latched: bool,
) -> RestartLatchDecision {
    if !collect_pressed {
        return RestartLatchDecision {
            restart_requested: false,
            restart_button_latched: false,
        };
    }

    if !game_won || restart_button_latched {
        return RestartLatchDecision {
            restart_requested: false,
            restart_button_latched,
        };
    }

    RestartLatchDecision {
        restart_requested: true,
        restart_button_latched: true,
    }
}

#[cfg(test)]
mod tests {
    use super::decide_restart_signal;

    #[test]
    fn no_restart_when_not_won() {
        let decision = decide_restart_signal(true, false, false);
        assert!(!decision.restart_requested);
        assert!(!decision.restart_button_latched);
    }

    #[test]
    fn single_restart_edge_when_collect_pressed_while_won() {
        let decision = decide_restart_signal(true, true, false);
        assert!(decision.restart_requested);
        assert!(decision.restart_button_latched);
    }

    #[test]
    fn no_repeated_restart_while_held_latched() {
        let first = decide_restart_signal(true, true, false);
        assert!(first.restart_requested);
        assert!(first.restart_button_latched);

        let held = decide_restart_signal(true, true, first.restart_button_latched);
        assert!(!held.restart_requested);
        assert!(held.restart_button_latched);
    }

    #[test]
    fn latch_clears_on_release_and_can_trigger_again() {
        let pressed = decide_restart_signal(true, true, false);
        assert!(pressed.restart_requested);
        assert!(pressed.restart_button_latched);

        let released = decide_restart_signal(false, true, pressed.restart_button_latched);
        assert!(!released.restart_requested);
        assert!(!released.restart_button_latched);

        let pressed_again = decide_restart_signal(true, true, released.restart_button_latched);
        assert!(pressed_again.restart_requested);
        assert!(pressed_again.restart_button_latched);
    }
}
