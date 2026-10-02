//! The screens before a game starts: the table and the main menu. The other screens live in
//! their own modules and hang off this one.

use crate::art::{Art, CANDLE_SIZE};
use crate::i18n::T;
use crate::store::Store;
use crate::theme::*;
use crate::widgets::{Activate, button, panel_backing, panel_node};
use bevy::prelude::*;

#[derive(States, Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Screen {
    #[default]
    Main,
    Keys,
    Settings,
    /// The form where the host says what world they want.
    NewGame,
    /// Waiting for the game master to invent and draw the world.
    Creating,
    Load,
    Play,
}

/// What a navigation button does when used.
#[derive(Component, Clone, Copy, PartialEq)]
pub enum Nav {
    Go(Screen),
    /// Back to where this screen was opened from; the main menu unless something else is
    /// recorded in `ReturnTo`.
    Back,
    /// Load the newest save.
    Continue,
    Quit,
    /// Leads to a part of the game that is not built yet; the button is shown disabled.
    Nothing,
}

/// Set by a screen that opens another one and wants to be returned to: the key screen opened
/// from a running game goes back to the game, not to the main menu.
#[derive(Resource, Default)]
pub struct ReturnTo(pub Option<Screen>);

/// The table and candles behind the menus. Hidden while a game is on screen.
#[derive(Component)]
pub struct Backdrop;

/// A full-canvas column that lives exactly as long as its screen.
pub fn screen_root(screen: Screen) -> impl Bundle {
    (
        DespawnOnExit(screen),
        Node { width: percent(100), height: percent(100), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() },
    )
}

/// Spawns a parchment sheet and lets `content` fill it.
pub fn spawn_panel(parent: &mut ChildSpawnerCommands, art: &Art, node: Node, content: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent.spawn(panel_node(node)).with_children(|sheet| {
        sheet.spawn(panel_backing(art));
        content(sheet);
    });
}

#[derive(Component)]
struct Candle {
    frame: usize,
    until_next: f32,
}

#[derive(Component)]
struct Glow {
    phase: f32,
}

const GLOW_SIZE: f32 = 80.0;

fn at(x: f32, y: f32, w: f32, h: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: px(x), top: px(y), width: px(w), height: px(h), ..default() }
}

fn spawn_backdrop(mut commands: Commands, art: Res<Art>) {
    commands
        .spawn((Backdrop, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() }, GlobalZIndex(-10)))
        .with_children(|table| {
            table.spawn((at(0.0, 0.0, crate::screen::CANVAS.x as f32, crate::screen::CANVAS.y as f32), ImageNode::new(art.table.clone())));
            for (x, y, phase) in [(96.0, 118.0, 0.0), (374.0, 118.0, 2.1), (60.0, 206.0, 4.4)] {
                // The light is centred on the flame, not on the candle.
                table.spawn((at(x + 4.5 - GLOW_SIZE / 2.0, y + 5.0 - GLOW_SIZE / 2.0, GLOW_SIZE, GLOW_SIZE), ImageNode::new(art.glow.clone()), Glow { phase }));
                table.spawn((
                    at(x, y, CANDLE_SIZE.x as f32, CANDLE_SIZE.y as f32),
                    ImageNode::new(art.candle[0].clone()),
                    Candle { frame: 0, until_next: phase * 0.05 },
                ));
            }
        });
}

/// Flames change shape at uneven intervals, never showing the same frame twice in a row.
fn flicker(time: Res<Time>, art: Res<Art>, mut candles: Query<(&mut Candle, &mut ImageNode)>, mut seed: Local<u32>) {
    for (mut candle, mut image) in &mut candles {
        candle.until_next -= time.delta_secs();
        if candle.until_next > 0.0 {
            continue;
        }
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let step = 1 + (*seed >> 16) as usize % 3;
        candle.frame = (candle.frame + step) % 4;
        candle.until_next = 0.09 + ((*seed >> 8) % 100) as f32 / 1000.0;
        image.image = art.candle[candle.frame].clone();
    }
}

fn breathe(time: Res<Time>, mut glows: Query<(&Glow, &mut ImageNode)>) {
    for (glow, mut image) in &mut glows {
        let t = time.elapsed_secs();
        let wave = (t * 7.3 + glow.phase).sin() * 0.5 + (t * 2.9 + glow.phase * 1.7).sin() * 0.5;
        // Steps, not a smooth fade: the light jumps the way a real flame does.
        let alpha = 0.7 + (wave * 3.0).round() / 3.0 * 0.3;
        if image.color.alpha() != alpha {
            image.color = Color::srgba(1.0, 1.0, 1.0, alpha);
        }
    }
}

fn spawn_main(mut commands: Commands, art: Res<Art>, fonts: Res<Fonts>, store: Res<Store>) {
    let has_saves = !store.list_saves().is_empty();
    commands.spawn(screen_root(Screen::Main)).with_children(|root| {
        root.spawn((
            Node { margin: UiRect::top(px(22)), ..default() },
            Text::new("PIXEL GM"),
            fonts.title(2),
            TextColor(color(BRASS_LIGHT)),
            TextShadow { offset: Vec2::new(2.0, 2.0), color: color(INK) },
        ));
        root.spawn((Node { margin: UiRect::top(px(6)), ..default() }, Text::default(), T::Tagline, fonts.body(), TextColor(color(PARCHMENT_SHADE))));
        let sheet = Node { margin: UiRect::top(px(14)), width: px(156), flex_direction: FlexDirection::Column, row_gap: px(4), ..default() };
        spawn_panel(root, &art, sheet, |sheet| {
            for (label, nav, enabled) in [
                (T::NewGame, Nav::Go(Screen::NewGame), true),
                (T::Continue, Nav::Continue, has_saves),
                (T::LoadGame, Nav::Go(Screen::Load), has_saves),
                (T::Multiplayer, Nav::Nothing, false),
                (T::Keys, Nav::Go(Screen::Keys), true),
                (T::Settings, Nav::Go(Screen::Settings), true),
                (T::Quit, Nav::Quit, true),
            ] {
                button(sheet, &art, &fonts, label, enabled, nav);
            }
        });
    });
}

fn on_activate(used: On<Activate>, buttons: Query<&Nav>, mut return_to: ResMut<ReturnTo>, mut next: ResMut<NextState<Screen>>, mut exit: MessageWriter<AppExit>) {
    let Ok(nav) = buttons.get(used.entity) else { return };
    match nav {
        Nav::Go(screen) => next.set(*screen),
        Nav::Back => next.set(return_to.0.take().unwrap_or(Screen::Main)),
        Nav::Quit => {
            exit.write(AppExit::Success);
        }
        // Loading the newest save is the save module's business.
        Nav::Continue | Nav::Nothing => {}
    }
}

/// Escape, or the right-hand button of a gamepad, leaves a menu screen the same way its Back
/// button does. The play screen and the wait for a new world handle Escape themselves.
fn escape_goes_back(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    screen: Res<State<Screen>>,
    mut return_to: ResMut<ReturnTo>,
    mut next: ResMut<NextState<Screen>>,
) {
    let back = keys.just_pressed(KeyCode::Escape) || gamepads.iter().any(|pad| pad.just_pressed(GamepadButton::East));
    if back && matches!(screen.get(), Screen::Keys | Screen::Settings | Screen::NewGame | Screen::Load) {
        next.set(return_to.0.take().unwrap_or(Screen::Main));
    }
}

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Screen>()
            .init_resource::<ReturnTo>()
            .add_systems(Startup, spawn_backdrop)
            .add_systems(OnEnter(Screen::Main), spawn_main)
            .add_systems(Update, (flicker, breathe, escape_goes_back))
            .add_observer(on_activate);
    }
}
