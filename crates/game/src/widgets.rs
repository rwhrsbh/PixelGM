//! The shell's own controls, built from the pictures in `art`.

use crate::art::{Art, BUTTON_BORDER, ButtonLook, PANEL_BORDER, sliced};
use crate::i18n::T;
use crate::nav::NavMode;
use crate::theme::*;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::ui::{RelativeCursorPosition, UiGlobalTransform};
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigation;

#[derive(Component)]
pub struct PixelButton {
    pub enabled: bool,
    look: ButtonLook,
}

impl PixelButton {
    /// A button that can be pressed, drawn by whoever spawns it.
    pub fn plain() -> Self {
        Self { enabled: true, look: ButtonLook::Idle }
    }
}

/// A button was used: clicked with the mouse, or chosen with the keyboard or a gamepad.
/// Screens listen for this instead of raw clicks, so every way of pressing behaves the same.
#[derive(EntityEvent)]
pub struct Activate {
    pub entity: Entity,
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

fn button_node(justify: JustifyContent) -> Node {
    Node { height: px(BUTTON_HEIGHT), padding: UiRect::axes(px(8), px(0)), justify_content: justify, align_items: AlignItems::Center, flex_shrink: 0.0, ..default() }
}

/// Spawns a button with a translated label. `extra` carries whatever says what the button
/// does. Disabled buttons are left out of keyboard and gamepad navigation.
pub fn button(parent: &mut ChildSpawnerCommands, art: &Art, fonts: &Fonts, label: T, enabled: bool, extra: impl Bundle) {
    let look = if enabled { ButtonLook::Idle } else { ButtonLook::Disabled };
    let mut entity = parent.spawn((
        Button,
        PixelButton { enabled, look },
        extra,
        button_node(JustifyContent::Center),
        children![
            (backing(art.button(look), BUTTON_BORDER), ButtonFace),
            (Text::default(), label, fonts.body(), TextColor(label_color(look)), ButtonLabel),
        ],
    ));
    if enabled {
        entity.insert(AutoDirectionalNavigation::default());
    }
}

/// Spawns a button whose label is data rather than a translated string, such as a row in a
/// list. The text starts at the left and is cut off rather than wrapped.
/// Marks the one button of a sheet that Enter presses even while the mouse is steering:
/// "read on", "roll". Without the mark Enter is left to the sheet, which may use it to
/// confirm something else.
#[derive(Component, Clone, Copy)]
pub struct EnterPresses;

/// A button whose words wrap onto more lines rather than being cut off: for long choices.
pub fn wrap_button(parent: &mut ChildSpawnerCommands, art: &Art, fonts: &Fonts, label: String, extra: impl Bundle) {
    let look = ButtonLook::Idle;
    parent.spawn((
        Button,
        PixelButton { enabled: true, look },
        extra,
        AutoDirectionalNavigation::default(),
        Node { height: Val::Auto, min_height: px(BUTTON_HEIGHT), width: percent(100), padding: UiRect::axes(px(8), px(3)), ..button_node(JustifyContent::FlexStart) },
        children![
            (backing(art.button(look), BUTTON_BORDER), ButtonFace),
            (Node { width: percent(100), ..default() }, Text::new(label), fonts.body(), TextColor(label_color(look)), ButtonLabel),
        ],
    ));
}

/// A reply in a list, one slim line of words that lights up under the cursor: the old way
/// of offering choices, which leaves the telling room.
#[derive(Component)]
pub struct ReplyLine;

pub fn line_button(parent: &mut ChildSpawnerCommands, fonts: &Fonts, label: String, extra: impl Bundle) {
    parent.spawn((
        Button,
        PixelButton { enabled: true, look: ButtonLook::Idle },
        ReplyLine,
        extra,
        AutoDirectionalNavigation::default(),
        Node { width: percent(100), padding: UiRect::horizontal(px(4)), ..default() },
        BackgroundColor(Color::NONE),
        children![(Node { width: percent(100), ..default() }, Text::new(label), fonts.body(), TextColor(color(WOOD_DARK)))],
    ));
}

fn style_lines(focus: Res<InputFocus>, nav: Res<NavMode>, mut lines: Query<(Entity, &Interaction, &mut BackgroundColor), With<ReplyLine>>) {
    for (entity, interaction, mut back) in &mut lines {
        let lit = *interaction != Interaction::None || (nav.active && focus.get() == Some(entity));
        let wanted = if lit { color(BRASS_LIGHT) } else { Color::NONE };
        if back.0 != wanted {
            back.0 = wanted;
        }
    }
}

pub fn text_button(parent: &mut ChildSpawnerCommands, art: &Art, fonts: &Fonts, label: String, extra: impl Bundle) {
    let look = ButtonLook::Idle;
    parent.spawn((
        Button,
        PixelButton { enabled: true, look },
        extra,
        AutoDirectionalNavigation::default(),
        Node { overflow: Overflow::clip(), ..button_node(JustifyContent::FlexStart) },
        children![
            (backing(art.button(look), BUTTON_BORDER), ButtonFace),
            (Text::new(label), TextLayout::no_wrap(), fonts.body(), TextColor(label_color(look)), ButtonLabel),
        ],
    ));
}

/// A list that scrolls when it holds more than fits in `height`.
#[derive(Component)]
pub struct Scrolls;

pub fn scroll_list(node: Node, height: f32) -> impl Bundle {
    // Room is left on the right for the bar that appears when there is more than fits.
    let padding = UiRect { right: px(SCROLL_BAR + 2.0), ..node.padding };
    (Node { max_height: px(height), overflow: Overflow::scroll_y(), padding, ..node }, Scrolls, RelativeCursorPosition::default())
}

/// How wide the bar beside a list is, and how tall each of its arrows.
const SCROLL_BAR: f32 = 9.0;
/// Canvas pixels a held arrow moves a list by in a second.
const ARROW_SPEED: f32 = 140.0;

/// The bar beside a list that holds more than it shows: an arrow up, an arrow down and a
/// mark of where in the list one is. It is for a mouse without a wheel.
#[derive(Component)]
struct ScrollBar(Entity);

#[derive(Component)]
struct ScrollArrow {
    list: Entity,
    down: bool,
}

#[derive(Component)]
struct ScrollThumb(Entity);

#[allow(clippy::type_complexity)]
fn scroll_bars(
    mut commands: Commands,
    fonts: Res<Fonts>,
    time: Res<Time>,
    mut lists: Query<(Entity, &mut ScrollPosition, &ComputedNode, &UiGlobalTransform), With<Scrolls>>,
    mut bars: Query<(Entity, &ScrollBar, &mut Node), (Without<Scrolls>, Without<ScrollThumb>)>,
    arrows: Query<(&ScrollArrow, Ref<Interaction>)>,
    mut thumbs: Query<(&ScrollThumb, &mut Node), (Without<ScrollBar>, Without<Scrolls>)>,
) {
    // A held arrow keeps the list moving; a click moves it a row.
    for (arrow, interaction) in &arrows {
        if *interaction == Interaction::Pressed
            && let Ok((_, mut position, node, _)) = lists.get_mut(arrow.list)
        {
            // The press itself moves a row, so that a quick click is felt.
            let step = (ARROW_SPEED * time.delta_secs() + if interaction.is_changed() { WHEEL_STEP } else { 0.0 }) * if arrow.down { 1.0 } else { -1.0 };
            position.y = (position.y + step).clamp(0.0, furthest_scroll(node));
        }
    }
    let place = |node: &ComputedNode, at: &UiGlobalTransform| {
        let size = node.size() * node.inverse_scale_factor();
        let centre = at.translation * node.inverse_scale_factor();
        (centre.x + size.x / 2.0 - SCROLL_BAR, centre.y - size.y / 2.0, size.y)
    };
    let mut barred = Vec::new();
    for (entity, bar, mut node) in &mut bars {
        match lists.get(bar.0) {
            Ok((_, _, list, at)) if furthest_scroll(list) > 0.5 => {
                let (left, top, height) = place(list, at);
                if node.left != px(left) || node.top != px(top) || node.height != px(height) {
                    node.left = px(left);
                    node.top = px(top);
                    node.height = px(height);
                }
                barred.push(bar.0);
            }
            _ => commands.entity(entity).despawn(),
        }
    }
    for (thumb, mut node) in &mut thumbs {
        let Ok((_, position, list, _)) = lists.get(thumb.0) else { continue };
        let shown = list.size().y * list.inverse_scale_factor();
        let all = list.content_size().y * list.inverse_scale_factor();
        let track = (shown - 2.0 * SCROLL_BAR).max(1.0);
        let height = (track * shown / all.max(1.0)).clamp(6.0f32.min(track), track);
        let top = SCROLL_BAR + (track - height) * (position.y / furthest_scroll(list).max(1.0)).clamp(0.0, 1.0);
        if node.top != px(top) || node.height != px(height) {
            node.top = px(top);
            node.height = px(height);
        }
    }
    for (list, _, node, at) in &lists {
        if barred.contains(&list) || furthest_scroll(node) <= 0.5 {
            continue;
        }
        let (left, top, height) = place(node, at);
        let wood = Color::srgb(0.36, 0.22, 0.12);
        commands
            .spawn((
                ScrollBar(list),
                Node { position_type: PositionType::Absolute, left: px(left), top: px(top), width: px(SCROLL_BAR), height: px(height), flex_direction: FlexDirection::Column, justify_content: JustifyContent::SpaceBetween, ..default() },
                BackgroundColor(Color::srgba(0.2, 0.13, 0.08, 0.3)),
                GlobalZIndex(40),
            ))
            .with_children(|bar| {
                for down in [false, true] {
                    bar.spawn((ScrollArrow { list, down }, Button, Node { width: px(SCROLL_BAR), height: px(SCROLL_BAR), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(wood)))
                        .with_children(|arrow| {
                            arrow.spawn((Text::new(if down { "▼" } else { "▲" }), fonts.body(), TextColor(color(PARCHMENT)), Pickable::IGNORE));
                        });
                }
                bar.spawn((ScrollThumb(list), Node { position_type: PositionType::Absolute, left: px(2), top: px(SCROLL_BAR), width: px(SCROLL_BAR - 4.0), height: px(6), ..default() }, BackgroundColor(wood), Pickable::IGNORE));
            });
    }
}

/// Canvas pixels moved by one notch of the wheel: one button row.
const WHEEL_STEP: f32 = BUTTON_HEIGHT + 2.0;

fn furthest_scroll(node: &ComputedNode) -> f32 {
    ((node.content_size().y - node.size().y) * node.inverse_scale_factor()).max(0.0)
}

fn wheel_scrolls(mut wheel: MessageReader<MouseWheel>, mut lists: Query<(&mut ScrollPosition, &ComputedNode, &RelativeCursorPosition), With<Scrolls>>) {
    let moved: f32 = wheel.read().map(|w| if w.unit == MouseScrollUnit::Line { w.y * WHEEL_STEP } else { w.y }).sum();
    if moved == 0.0 {
        return;
    }
    for (mut position, node, cursor) in &mut lists {
        if cursor.cursor_over() {
            position.y = (position.y - moved).clamp(0.0, furthest_scroll(node));
        }
    }
}

/// Keeps the focused control in view when the keyboard or a gamepad moves into a part of a
/// list that is scrolled away.
fn focus_scrolls_into_view(
    focus: Res<InputFocus>,
    nav: Res<NavMode>,
    parents: Query<&ChildOf>,
    nodes: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut lists: Query<&mut ScrollPosition, With<Scrolls>>,
) {
    if !nav.active || !focus.is_changed() {
        return;
    }
    let Some(focused) = focus.get() else { return };
    let Some(list) = parents.iter_ancestors(focused).find(|e| lists.contains(*e)) else { return };
    let (Ok((item, item_at)), Ok((frame, frame_at))) = (nodes.get(focused), nodes.get(list)) else { return };
    let above = (frame_at.translation.y - frame.size().y / 2.0) - (item_at.translation.y - item.size().y / 2.0);
    let below = (item_at.translation.y + item.size().y / 2.0) - (frame_at.translation.y + frame.size().y / 2.0);
    let shift = if above > 0.0 { -above } else { below.max(0.0) } * frame.inverse_scale_factor();
    if shift != 0.0
        && let Ok(mut position) = lists.get_mut(list)
    {
        position.y = (position.y + shift).clamp(0.0, furthest_scroll(frame));
    }
}

/// A sheet of parchment: the node, to which `panel_backing` is added as the first child.
pub fn panel_node(node: Node) -> Node {
    Node { padding: UiRect::all(px(PANEL_BORDER + 4.0)), ..node }
}

pub fn panel_backing(art: &Art) -> impl Bundle {
    backing(art.panel.clone(), PANEL_BORDER)
}

/// A button looks pressed under a held mouse button, lit under the cursor, and lit when it
/// holds the focus while the keyboard or a gamepad is steering.
fn style_buttons(
    art: Res<Art>,
    focus: Res<InputFocus>,
    nav: Res<NavMode>,
    mut buttons: Query<(Entity, &Interaction, &mut PixelButton, &mut Node, &Children), Without<ReplyLine>>,
    mut faces: Query<&mut ImageNode, With<ButtonFace>>,
    mut labels: Query<&mut TextColor, With<ButtonLabel>>,
) {
    for (entity, interaction, mut button, mut node, children) in &mut buttons {
        if !button.enabled {
            continue;
        }
        let focused = nav.active && focus.get() == Some(entity);
        let look = match interaction {
            Interaction::Pressed => ButtonLook::Down,
            Interaction::Hovered => ButtonLook::Hot,
            Interaction::None if focused => ButtonLook::Hot,
            Interaction::None => ButtonLook::Idle,
        };
        if button.look == look {
            continue;
        }
        button.look = look;
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

/// A button acts when pressed. Bevy's click checks the release against what was under the
/// pointer the frame before, so a mouse moved onto a button and clicked within one slow frame
/// lost the click; the press is checked against what is under the pointer now.
fn press_activates(press: On<Pointer<Press>>, buttons: Query<&PixelButton>, mut commands: Commands) {
    if press.button == PointerButton::Primary && buttons.get(press.entity).is_ok_and(|b| b.enabled) {
        commands.trigger(Activate { entity: press.entity });
    }
}

pub struct WidgetsPlugin;

impl Plugin for WidgetsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (style_buttons, style_lines, wheel_scrolls, scroll_bars, focus_scrolls_into_view)).add_observer(press_activates);
    }
}
