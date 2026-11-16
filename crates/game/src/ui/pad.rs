//! A gamepad steers the interface as the keyboard does: the D-pad and the left stick are
//! the arrows, the bottom button is Enter, the right-hand one Escape, the bumpers go to the
//! previous and next control. The game's own systems read the pad as they always did.

use super::egui::{self, Event, Key, Modifiers};
use bevy::prelude::*;

/// A stick pushed this far is a direction.
const STICK: f32 = 0.6;
/// A held direction repeats like a held key: a pause, then a steady rate.
const REPEAT_DELAY: f32 = 0.35;
const REPEAT_RATE: f32 = 0.12;

/// What the pad held last frame, and when a held direction next repeats.
#[derive(Default)]
pub struct Held {
    direction: Option<Key>,
    until_repeat: f32,
}

impl Held {
    /// Whether the direction now held is pressed this frame: at once, then on repeat.
    fn step(&mut self, now: Option<Key>, dt: f32) -> bool {
        if now != self.direction {
            self.direction = now;
            self.until_repeat = REPEAT_DELAY;
            return now.is_some();
        }
        if now.is_none() {
            return false;
        }
        self.until_repeat -= dt;
        if self.until_repeat <= 0.0 {
            self.until_repeat = REPEAT_RATE;
            return true;
        }
        false
    }
}

fn direction(pad: &Gamepad) -> Option<Key> {
    let stick = pad.left_stick();
    if pad.pressed(GamepadButton::DPadUp) || stick.y > STICK {
        Some(Key::ArrowUp)
    } else if pad.pressed(GamepadButton::DPadDown) || stick.y < -STICK {
        Some(Key::ArrowDown)
    } else if pad.pressed(GamepadButton::DPadLeft) || stick.x < -STICK {
        Some(Key::ArrowLeft)
    } else if pad.pressed(GamepadButton::DPadRight) || stick.x > STICK {
        Some(Key::ArrowRight)
    } else {
        None
    }
}

fn press(key: Key) -> Event {
    Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }
}

/// The keys the pads stand for this frame, and the focus moves they ask for.
pub fn keys(pads: &[&Gamepad], held: &mut Held, dt: f32) -> (Vec<Event>, Option<egui::FocusDirection>) {
    let mut events = Vec::new();
    let mut focus = None;
    let now = pads.iter().find_map(|pad| direction(pad));
    if held.step(now, dt)
        && let Some(key) = now
    {
        events.push(press(key));
    }
    for pad in pads {
        if pad.just_pressed(GamepadButton::South) {
            events.push(press(Key::Enter));
        }
        if pad.just_pressed(GamepadButton::East) {
            events.push(press(Key::Escape));
        }
        if pad.just_pressed(GamepadButton::LeftTrigger) {
            focus = Some(egui::FocusDirection::Previous);
        }
        if pad.just_pressed(GamepadButton::RightTrigger) {
            focus = Some(egui::FocusDirection::Next);
        }
    }
    (events, focus)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_direction_presses_once_then_repeats() {
        let mut held = Held::default();
        let dt = 0.05;
        let mut presses = 0;
        // Held for a second: once at once, then after the pause every repeat.
        for _ in 0..20 {
            if held.step(Some(Key::ArrowDown), dt) {
                presses += 1;
            }
        }
        assert_eq!(presses, 1 + ((1.0 - REPEAT_DELAY) / REPEAT_RATE) as usize);
        // Let go and pushed again: at once.
        assert!(!held.step(None, dt));
        assert!(held.step(Some(Key::ArrowDown), dt));
    }
}
