//! The screens before a game starts: the table and the main menu. The key and settings
//! screens live in their own modules and hang off this one.

use crate::art::{Art, CANDLE_SIZE};
use crate::i18n::T;
use crate::theme::*;
use crate::widgets::{PixelButton, button, panel_backing, panel_node};
use bevy::prelude::*;

#[derive(States, Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Screen {
    #[default]
    Main,
    Keys,
    Settings,
}

/// What a navigation button does when clicked.
#[derive(Component, Clone, Copy)]
pub enum Nav {
    Go(Screen),
    Quit,
    /// Leads to a part of the game that is not built yet; the button is shown disabled.
    Nothing,
}

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
        .spawn((Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() }, GlobalZIndex(-10)))
        .with_children(|table| {
            table.spawn((at(0.0, 0.0, crate::screen::CANVAS.x as f32, crate::screen::CANVAS.y as f32), ImageNode::new(art.table.clone())));
            for (x, y, phase) in [(112.0, 118.0, 0.0), (358.0, 118.0, 2.1), (60.0, 196.0, 4.4)] {
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

fn spawn_main(mut commands: Commands, art: Res<Art>, fonts: Res<Fonts>) {
    commands.spawn(screen_root(Screen::Main)).with_children(|root| {
        root.spawn((
            Node { margin: UiRect::top(px(28)), ..default() },
            Text::new("PIXEL GM"),
            fonts.title(2),
            TextColor(color(BRASS_LIGHT)),
            TextShadow { offset: Vec2::new(2.0, 2.0), color: color(INK) },
        ));
        root.spawn((Node { margin: UiRect::top(px(6)), ..default() }, Text::default(), T::Tagline, fonts.body(), TextColor(color(PARCHMENT_SHADE))));
        let sheet = Node { margin: UiRect::top(px(18)), width: px(156), flex_direction: FlexDirection::Column, row_gap: px(4), ..default() };
        spawn_panel(root, &art, sheet, |sheet| {
            for (label, nav) in [
                (T::NewGame, Nav::Nothing),
                (T::Continue, Nav::Nothing),
                (T::Multiplayer, Nav::Nothing),
                (T::Keys, Nav::Go(Screen::Keys)),
                (T::Settings, Nav::Go(Screen::Settings)),
                (T::Quit, Nav::Quit),
            ] {
                sheet.spawn(button(&art, &fonts, label, !matches!(nav, Nav::Nothing), nav));
            }
        });
    });
}

fn on_click(click: On<Pointer<Click>>, buttons: Query<(&Nav, &PixelButton)>, mut next: ResMut<NextState<Screen>>, mut exit: MessageWriter<AppExit>) {
    let Ok((nav, button)) = buttons.get(click.entity) else { return };
    if !button.enabled {
        return;
    }
    match nav {
        Nav::Go(screen) => next.set(*screen),
        Nav::Quit => {
            exit.write(AppExit::Success);
        }
        Nav::Nothing => {}
    }
}

/// Escape leaves a sub-screen the same way its Back button does.
fn escape_goes_back(keys: Res<ButtonInput<KeyCode>>, screen: Res<State<Screen>>, mut next: ResMut<NextState<Screen>>) {
    if keys.just_pressed(KeyCode::Escape) && *screen.get() != Screen::Main {
        next.set(Screen::Main);
    }
}

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Screen>()
            .add_systems(Startup, spawn_backdrop)
            .add_systems(OnEnter(Screen::Main), spawn_main)
            .add_systems(Update, (flicker, breathe, escape_goes_back))
            .add_observer(on_click);
    }
}
