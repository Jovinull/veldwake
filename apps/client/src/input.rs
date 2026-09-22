/// A combat verb, which is pressed rather than held.
///
/// These are **latched** rather than sampled, and that is not a detail. The
/// authoritative simulation runs at its own fixed rate, so a frame can produce
/// no ticks at all or four of them; a press read by sampling a held flag would be
/// lost in the first case and repeated in the second. A latch set on the key-down
/// edge and cleared by the tick that consumes it is the only shape that keeps one
/// press meaning one swing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CombatAction {
    Attack,
    Dodge,
    /// M9's exchange verb. Latched exactly like the other two, because a frame
    /// can run no ticks or four and a sampled flag would be lost in the first
    /// case and repeated in the second.
    Interact,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CameraAction {
    Forward,
    Backward,
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MovementAxes {
    pub forward: f32,
    pub right: f32,
    pub up: f32,
}

/// The combat presses waiting for a tick.
///
/// A named record rather than a tuple, because M9 makes it three booleans of
/// the same type and a positional triple is one transposition away from a swing
/// that swaps weapons.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CombatLatches {
    pub attack: bool,
    pub dodge: bool,
    pub interact: bool,
}

impl CombatLatches {
    /// Nothing pressed.
    pub const NONE: Self = Self {
        attack: false,
        dodge: false,
        interact: false,
    };

    /// Whether any verb is waiting, for the report line.
    #[must_use]
    pub const fn any(&self) -> bool {
        self.attack || self.dodge || self.interact
    }
}

#[derive(Debug, Default)]
pub struct InputState {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    look_active: bool,
    look_delta: (f32, f32),
    /// Set on a key-down edge, cleared by the combat tick that consumes it.
    attack_latched: bool,
    dodge_latched: bool,
    interact_latched: bool,
    /// Whether the key is currently held, so a repeat event cannot re-latch.
    attack_held: bool,
    dodge_held: bool,
    interact_held: bool,
}

impl InputState {
    pub fn set_action(&mut self, action: CameraAction, pressed: bool) {
        match action {
            CameraAction::Forward => self.forward = pressed,
            CameraAction::Backward => self.backward = pressed,
            CameraAction::Left => self.left = pressed,
            CameraAction::Right => self.right = pressed,
            CameraAction::Up => self.up = pressed,
            CameraAction::Down => self.down = pressed,
        }
    }

    pub fn movement_axes(&self) -> MovementAxes {
        MovementAxes {
            forward: axis(self.forward, self.backward),
            right: axis(self.right, self.left),
            up: axis(self.up, self.down),
        }
    }

    /// Records a combat key's state, latching on the down edge only.
    ///
    /// `winit` repeats a held key, so the held flag is what stops one press from
    /// becoming a swing every frame.
    pub fn set_combat_action(&mut self, action: CombatAction, pressed: bool) {
        match action {
            CombatAction::Attack => {
                if pressed && !self.attack_held {
                    self.attack_latched = true;
                }
                self.attack_held = pressed;
            }
            CombatAction::Dodge => {
                if pressed && !self.dodge_held {
                    self.dodge_latched = true;
                }
                self.dodge_held = pressed;
            }
            CombatAction::Interact => {
                if pressed && !self.interact_held {
                    self.interact_latched = true;
                }
                self.interact_held = pressed;
            }
        }
    }

    /// Takes the latched combat presses, clearing them.
    ///
    /// Called by the first combat tick of a frame. A frame that runs no tick does
    /// not call it, so the press waits rather than disappearing.
    pub fn take_combat_latches(&mut self) -> CombatLatches {
        let latched = CombatLatches {
            attack: self.attack_latched,
            dodge: self.dodge_latched,
            interact: self.interact_latched,
        };
        self.attack_latched = false;
        self.dodge_latched = false;
        self.interact_latched = false;
        latched
    }

    /// Which presses are waiting, for the report line.
    ///
    /// Reported separately because a latch that is stuck has to name itself: the
    /// first real run showed one set and the two flags are what told us which.
    pub const fn combat_latches(&self) -> CombatLatches {
        CombatLatches {
            attack: self.attack_latched,
            dodge: self.dodge_latched,
            interact: self.interact_latched,
        }
    }

    pub fn set_look_active(&mut self, active: bool) {
        self.look_active = active;
        if !active {
            self.look_delta = (0.0, 0.0);
        }
    }

    pub fn add_look_delta(&mut self, horizontal: f64, vertical: f64) {
        if !self.look_active || !horizontal.is_finite() || !vertical.is_finite() {
            return;
        }

        self.look_delta.0 += horizontal as f32;
        self.look_delta.1 += vertical as f32;
    }

    pub fn take_look_delta(&mut self) -> (f32, f32) {
        std::mem::take(&mut self.look_delta)
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    #[cfg(test)]
    fn look_active(&self) -> bool {
        self.look_active
    }
}

fn axis(positive: bool, negative: bool) -> f32 {
    f32::from(u8::from(positive)) - f32::from(u8::from(negative))
}

#[cfg(test)]
mod tests {
    use super::{CameraAction, InputState, MovementAxes};

    #[test]
    fn key_transitions_produce_signed_axes() {
        let mut input = InputState::default();
        input.set_action(CameraAction::Forward, true);
        input.set_action(CameraAction::Left, true);
        input.set_action(CameraAction::Up, true);

        assert_eq!(
            input.movement_axes(),
            MovementAxes {
                forward: 1.0,
                right: -1.0,
                up: 1.0,
            }
        );

        input.set_action(CameraAction::Forward, false);
        assert_eq!(input.movement_axes().forward, 0.0);
    }

    #[test]
    fn focus_reset_releases_keys_and_mouse_look() {
        let mut input = InputState::default();
        input.set_action(CameraAction::Forward, true);
        input.set_look_active(true);
        input.add_look_delta(12.0, -4.0);

        input.reset();

        assert_eq!(input.movement_axes(), MovementAxes::default());
        assert!(!input.look_active());
        assert_eq!(input.take_look_delta(), (0.0, 0.0));
    }

    #[test]
    fn look_delta_is_gated_consumed_and_rejects_non_finite_values() {
        let mut input = InputState::default();
        input.add_look_delta(3.0, 4.0);
        assert_eq!(input.take_look_delta(), (0.0, 0.0));

        input.set_look_active(true);
        input.add_look_delta(3.0, -2.0);
        input.add_look_delta(f64::NAN, 5.0);
        assert_eq!(input.take_look_delta(), (3.0, -2.0));
        assert_eq!(input.take_look_delta(), (0.0, 0.0));
    }
}
