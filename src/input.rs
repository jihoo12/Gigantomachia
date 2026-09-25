//! Input state independent of camera bindings or scene behavior.

use glam::Vec2;
use std::collections::HashSet;
pub use winit::{event::MouseButton, keyboard::KeyCode};

#[derive(Default)]
pub struct Input {
    held: HashSet<KeyCode>,
    pressed: HashSet<KeyCode>,
    mouse: HashSet<MouseButton>,
    cursor: Option<Vec2>,
    delta: Vec2,
}

impl Input {
    pub fn held(&self, key: KeyCode) -> bool {
        self.held.contains(&key)
    }
    pub fn pressed(&self, key: KeyCode) -> bool {
        self.pressed.contains(&key)
    }
    pub fn mouse_held(&self, button: MouseButton) -> bool {
        self.mouse.contains(&button)
    }
    pub fn mouse_delta(&self) -> Vec2 {
        self.delta
    }
    pub(crate) fn key(&mut self, key: KeyCode, down: bool) {
        if down {
            if self.held.insert(key) {
                self.pressed.insert(key);
            }
        } else {
            self.held.remove(&key);
        }
    }
    pub(crate) fn mouse_button(&mut self, button: MouseButton, down: bool) {
        if down {
            self.mouse.insert(button);
        } else {
            self.mouse.remove(&button);
        }
        // Do not apply movement recorded before a drag started/ended.
        self.delta = Vec2::ZERO;
    }
    pub(crate) fn cursor(&mut self, position: Vec2) {
        if let Some(previous) = self.cursor {
            self.delta += position - previous;
        }
        self.cursor = Some(position);
    }
    pub(crate) fn cursor_left(&mut self) {
        self.cursor = None;
        self.mouse.clear();
        self.delta = Vec2::ZERO;
    }
    pub(crate) fn end_frame(&mut self) {
        self.pressed.clear();
        self.delta = Vec2::ZERO;
    }
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quick_taps_survive_until_update_and_repeats_do_not_retrigger() {
        let mut input = Input::default();
        input.key(KeyCode::Space, true);
        input.key(KeyCode::Space, false);
        assert!(input.pressed(KeyCode::Space));
        assert!(!input.held(KeyCode::Space));
        input.end_frame();
        assert!(!input.pressed(KeyCode::Space));
        input.key(KeyCode::KeyW, true);
        input.end_frame();
        input.key(KeyCode::KeyW, true);
        assert!(input.held(KeyCode::KeyW));
        assert!(!input.pressed(KeyCode::KeyW));
        input.clear();
        assert!(!input.held(KeyCode::KeyW));
    }
}
