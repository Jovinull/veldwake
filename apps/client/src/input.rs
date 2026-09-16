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
