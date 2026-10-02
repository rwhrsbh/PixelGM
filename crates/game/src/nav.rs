//! Steering the interface without a mouse: arrow keys, a gamepad's D-pad or left stick move
//! the focus between buttons, Enter, Space or the pad's bottom button press the focused one.

use crate::widgets::{Activate, PixelButton};
use bevy::input_focus::directional_navigation::DirectionalNavigationPlugin;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::math::CompassOctant;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui::UiGlobalTransform;
use bevy::ui::auto_directional_navigation::{AutoDirectionalNavigation, AutoDirectionalNavigator};
use bevy::window::CursorMoved;

/// Whether the keyboard or a gamepad is steering. The focus is only drawn while it is;
/// moving the mouse hands control back to the cursor.
#[derive(Resource, Default)]
pub struct NavMode {
    pub active: bool,
}

const STICK_THRESHOLD: f32 = 0.6;
/// A held direction repeats like a held key: a pause, then a steady rate.
const REPEAT_DELAY: f32 = 0.35;
const REPEAT_RATE: f32 = 0.12;

/// One of the four directions, from whichever device gives one. Up and down win over left
/// and right, so a stick pushed diagonally still moves through a list.
pub fn direction_of(up: bool, down: bool, left: bool, right: bool) -> Option<CompassOctant> {
    match (up, down, left, right) {
        (true, false, _, _) => Some(CompassOctant::North),
        (false, true, _, _) => Some(CompassOctant::South),
        (_, _, true, false) => Some(CompassOctant::West),
        (_, _, false, true) => Some(CompassOctant::East),
        _ => None,
    }
}

#[derive(Default)]
struct Held {
    direction: Option<CompassOctant>,
    until_repeat: f32,
}

impl Held {
    /// Whether a step should be taken this frame for the direction now held.
    fn step(&mut self, now: Option<CompassOctant>, dt: f32) -> bool {
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

fn held_direction(keyboard: &ButtonInput<KeyCode>, gamepads: &Query<&Gamepad>) -> Option<CompassOctant> {
    let mut held = [
        keyboard.pressed(KeyCode::ArrowUp),
        keyboard.pressed(KeyCode::ArrowDown),
        keyboard.pressed(KeyCode::ArrowLeft),
        keyboard.pressed(KeyCode::ArrowRight),
    ];
    for pad in gamepads {
        let stick = pad.left_stick();
        held[0] |= pad.pressed(GamepadButton::DPadUp) || stick.y > STICK_THRESHOLD;
        held[1] |= pad.pressed(GamepadButton::DPadDown) || stick.y < -STICK_THRESHOLD;
        held[2] |= pad.pressed(GamepadButton::DPadLeft) || stick.x < -STICK_THRESHOLD;
        held[3] |= pad.pressed(GamepadButton::DPadRight) || stick.x > STICK_THRESHOLD;
    }
    direction_of(held[0], held[1], held[2], held[3])
}

fn steer(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut held: Local<Held>,
    mut nav: ResMut<NavMode>,
    typing: Query<(), With<EditableText>>,
    targets: Query<(Entity, &UiGlobalTransform, &ComputedNode), With<AutoDirectionalNavigation>>,
    mut navigator: AutoDirectionalNavigator,
) {
    let direction = held_direction(&keyboard, &gamepads);
    if !held.step(direction, time.delta_secs()) {
        return;
    }
    let Some(direction) = direction else { return };
    let was_steering = std::mem::replace(&mut nav.active, true);
    let current = navigator.input_focus().filter(|e| targets.contains(*e));
    // The first press after the mouse only shows where the focus is; it does not move it.
    let Some(current) = current.filter(|_| was_steering) else {
        if current.is_none()
            && let Some(entity) = topmost(&targets)
        {
            navigator.manual_directional_navigation.focus.set(entity, FocusCause::Navigated);
        }
        return;
    };
    // Inside a text field left and right move the caret; only up and down leave it.
    if typing.contains(current) && matches!(direction, CompassOctant::East | CompassOctant::West) {
        return;
    }
    let _ = navigator.navigate(direction);
}

fn topmost(targets: &Query<(Entity, &UiGlobalTransform, &ComputedNode), With<AutoDirectionalNavigation>>) -> Option<Entity> {
    targets
        .iter()
        // A control that has not been laid out yet has no size and no real position.
        .filter(|(_, _, node)| node.size().x > 0.0)
        .min_by(|a, b| (a.1.translation.y, a.1.translation.x).partial_cmp(&(b.1.translation.y, b.1.translation.x)).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(entity, _, _)| entity)
}

/// While the keyboard or a gamepad is steering, something is always focused: when the focused
/// control disappears (a screen changed, a row was removed) the topmost one takes over.
fn keep_a_focus(nav: Res<NavMode>, mut focus: ResMut<InputFocus>, targets: Query<(Entity, &UiGlobalTransform, &ComputedNode), With<AutoDirectionalNavigation>>) {
    if !nav.active || focus.get().is_some_and(|e| targets.contains(e)) {
        return;
    }
    if let Some(entity) = topmost(&targets) {
        focus.set(entity, FocusCause::Navigated);
    }
}

fn press_focused(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    nav: Res<NavMode>,
    focus: Res<InputFocus>,
    buttons: Query<&PixelButton>,
    all: Query<(Entity, &PixelButton)>,
    typing: Query<(), With<EditableText>>,
    mut commands: Commands,
) {
    let pressed = keyboard.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) || gamepads.iter().any(|pad| pad.just_pressed(GamepadButton::South));
    if !pressed {
        return;
    }
    if !nav.active {
        // With the mouse in charge nothing is focused, but a sheet with a single button
        // (a telling to read on from) still closes on Enter.
        let mut enabled = all.iter().filter(|(_, b)| b.enabled);
        if let (Some((entity, _)), None) = (enabled.next(), enabled.next())
            && !focus.get().is_some_and(|e| typing.contains(e))
        {
            commands.trigger(Activate { entity });
        }
        return;
    }
    if let Some(entity) = focus.get().filter(|e| buttons.get(*e).is_ok_and(|b| b.enabled)) {
        commands.trigger(Activate { entity });
    }
}

fn mouse_takes_over(mut moves: MessageReader<CursorMoved>, mouse: Res<ButtonInput<MouseButton>>, mut nav: ResMut<NavMode>) {
    if (moves.read().count() > 0 || mouse.get_just_pressed().next().is_some()) && nav.active {
        nav.active = false;
    }
}

pub struct NavPlugin;

impl Plugin for NavPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(DirectionalNavigationPlugin).init_resource::<NavMode>().add_systems(PreUpdate, (mouse_takes_over, keep_a_focus, steer, press_focused).chain().after(bevy::input::InputSystems));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_wins_over_horizontal_and_opposites_cancel() {
        assert_eq!(direction_of(true, false, true, false), Some(CompassOctant::North));
        assert_eq!(direction_of(false, true, false, true), Some(CompassOctant::South));
        assert_eq!(direction_of(false, false, true, false), Some(CompassOctant::West));
        assert_eq!(direction_of(true, true, false, true), Some(CompassOctant::East));
        assert_eq!(direction_of(true, true, true, true), None);
        assert_eq!(direction_of(false, false, false, false), None);
    }

    #[test]
    fn held_direction_steps_once_then_repeats_after_a_pause() {
        let mut held = Held::default();
        let down = Some(CompassOctant::South);
        assert!(held.step(down, 0.016));
        assert!(!held.step(down, 0.2));
        assert!(held.step(down, 0.2));
        assert!(!held.step(down, 0.05));
        assert!(held.step(down, 0.08));
        assert!(!held.step(None, 0.016));
        assert!(held.step(down, 0.016));
    }

    #[test]
    fn changing_direction_steps_at_once() {
        let mut held = Held::default();
        assert!(held.step(Some(CompassOctant::South), 0.016));
        assert!(held.step(Some(CompassOctant::North), 0.016));
        assert!(!held.step(Some(CompassOctant::North), 0.016));
    }
}
