//! The interface, drawn with egui over the world Bevy draws. A screen is a function from
//! what it shows to what the player did: it keeps nothing of its own, so nothing on it is
//! ever rebuilt under the cursor, and a test can press its buttons without a window.

// Parts of the kit wait for the screens that will use them.
#[allow(dead_code)]
pub mod kit;
pub mod screens;
pub mod theme;

pub use bevy_egui::egui;

use crate::store::Settings;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass};

/// The order of the interface's work in a frame: the look first, then the screens.
#[derive(SystemSet, Clone, PartialEq, Eq, Hash, Debug)]
pub enum UiSet {
    Theme,
    Screens,
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .configure_sets(EguiPrimaryContextPass, (UiSet::Theme, UiSet::Screens).chain())
            .add_systems(EguiPrimaryContextPass, dress.in_set(UiSet::Theme));
    }
}

/// Puts the game's type and colours on egui once, and keeps its size to the screen.
fn dress(mut egui: EguiContexts, settings: Res<Settings>, window: Single<&Window, With<PrimaryWindow>>, mut dressed: Local<bool>) {
    let Ok(ctx) = egui.ctx_mut() else { return };
    if !*dressed {
        theme::install(ctx);
        *dressed = true;
    }
    steering(ctx);
    let zoom = theme::zoom(window.physical_height(), settings.screen, settings.ui_size) / window.scale_factor();
    if (ctx.zoom_factor() - zoom).abs() > f32::EPSILON {
        ctx.set_zoom_factor(zoom);
    }
}

/// Whether the keyboard or a pad is steering: arrows and Tab move the focus, and only then
/// is it drawn; the mouse moving hands control back. ↑ and ↓ move the focus between
/// controls unless a text field has it.
fn steering(ctx: &egui::Context) {
    let (keys, mouse, up, down) = ctx.input(|i| {
        let keys = i.events.iter().any(|e| matches!(e, egui::Event::Key { key: egui::Key::Tab | egui::Key::ArrowUp | egui::Key::ArrowDown | egui::Key::ArrowLeft | egui::Key::ArrowRight, pressed: true, .. }));
        (keys, i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_pressed(), i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::ArrowDown))
    });
    if keys || mouse {
        kit::set_steering(ctx, keys && !mouse);
    }
    if !ctx.egui_wants_keyboard_input() && ctx.memory(|m| m.focused().is_some()) {
        if up {
            ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::Up));
        } else if down {
            ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::Down));
        }
    }
}
