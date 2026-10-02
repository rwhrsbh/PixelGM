//! The shell's own controls, built from the pictures in `art`.

use crate::art::{Art, BUTTON_BORDER, ButtonLook, PANEL_BORDER, sliced};
use crate::i18n::T;
use crate::theme::*;
use bevy::prelude::*;

#[derive(Component)]
pub struct PixelButton {
    pub enabled: bool,
}

#[derive(Component)]
struct ButtonFace;

#[derive(Component)]
struct ButtonLabel;

pub const BUTTON_HEIGHT: f32 = 16.0;

fn label_color(look: ButtonLook) -> Color {
    color(match look {
        ButtonLook::Idle => PARCHMENT,
        ButtonLook::Hot => INK,
        ButtonLook::Down => PARCHMENT_SHADE,
        ButtonLook::Disabled => WOOD_LIGHT,
    })
}

/// A picture stretched behind its parent's content. It is a child, not a component of the
/// parent, because a node that carries an image is sized by the image, not by its children.
pub fn backing(image: Handle<Image>, border: f32) -> impl Bundle {
    (Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), ..default() }, sliced(image, border))
}

fn button_node(width: Val) -> Node {
    Node { width, height: px(BUTTON_HEIGHT), padding: UiRect::axes(px(8), px(0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, flex_shrink: 0.0, ..default() }
}

fn button_parts(enabled: bool, width: Val, extra: impl Bundle) -> (impl Bundle, ButtonLook) {
    let look = if enabled { ButtonLook::Idle } else { ButtonLook::Disabled };
    ((Button, PixelButton { enabled }, extra, button_node(width)), look)
}

/// A button with a translated label. `extra` carries whatever says what the button does.
pub fn button(art: &Art, fonts: &Fonts, label: T, enabled: bool, extra: impl Bundle) -> impl Bundle {
    let (base, look) = button_parts(enabled, auto(), extra);
    (
        base,
        children![
            (backing(art.button(look), BUTTON_BORDER), ButtonFace),
            (Text::default(), label, fonts.body(), TextColor(label_color(look)), ButtonLabel),
        ],
    )
}

/// A sheet of parchment. Children go after the returned bundle's own background child, so
/// spawn content with `with_children` or `add_children`.
pub fn panel_node(node: Node) -> Node {
    Node { padding: UiRect::all(px(PANEL_BORDER + 4.0)), ..node }
}

pub fn panel_backing(art: &Art) -> impl Bundle {
    backing(art.panel.clone(), PANEL_BORDER)
}

fn style_buttons(
    art: Res<Art>,
    mut buttons: Query<(&Interaction, &PixelButton, &mut Node, &Children), Changed<Interaction>>,
    mut faces: Query<&mut ImageNode, With<ButtonFace>>,
    mut labels: Query<&mut TextColor, With<ButtonLabel>>,
) {
    for (interaction, button, mut node, children) in &mut buttons {
        if !button.enabled {
            continue;
        }
        let look = match interaction {
            Interaction::Pressed => ButtonLook::Down,
            Interaction::Hovered => ButtonLook::Hot,
            Interaction::None => ButtonLook::Idle,
        };
        // A pressed plate sits lower, and its label goes with it.
        node.padding.top = px(if look == ButtonLook::Down { 2 } else { 0 });
        for child in children {
            if let Ok(mut face) = faces.get_mut(*child) {
                face.image = art.button(look);
            }
            if let Ok(mut c) = labels.get_mut(*child) {
                c.0 = label_color(look);
            }
        }
    }
}

pub struct WidgetsPlugin;

impl Plugin for WidgetsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, style_buttons);
    }
}
