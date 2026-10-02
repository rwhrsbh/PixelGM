//! Playing: the current location on screen, the party walking about it, and the passage
//! from one place to the next — drawn by the game master the first time it is entered.

use crate::art::{Art, ground_picture, sprite_picture};
use crate::gm::{Gm, Trouble, trouble_text};
use crate::i18n::{Lang, T};
use crate::master::{Answer, Ask, Master};
use crate::menu::{Backdrop, ReturnTo, Screen, spawn_panel};
use crate::screen::CANVAS;
use crate::shot::Warp;
use crate::store::{Settings, Store};
use crate::theme::*;
use crate::widgets::{Activate, button};
use bevy::prelude::*;
use bevy::sprite::Anchor;
use world::base::{HERO_SPRITE, MARKER_SPRITE};
use world::walk::FEET;
use world::{LocationDraft, TILE, World, fake};

/// The game being played.
#[derive(Resource)]
pub struct Game {
    pub world: World,
    /// Names the folder this world's saves live in.
    pub world_id: String,
}

/// What is happening on the play screen right now.
#[derive(Resource, Clone, PartialEq, Debug, Default)]
pub enum Phase {
    #[default]
    Walking,
    /// The screen darkens as the party steps through an exit to `to`.
    Leaving {
        to: String,
        t: f32,
    },
    /// The game master is drawing `to`, which had only a name until now.
    Drawing {
        to: String,
    },
    /// The screen lightens on the new place.
    Arriving {
        t: f32,
    },
    /// The place beyond the exit could not be drawn; the player decides what to do.
    Stuck {
        to: String,
        trouble: Trouble,
    },
    Paused,
}

#[derive(Resource, Default)]
struct Redraw(bool);

/// The words shown on arrival: the place's name at once, its description letter by letter.
#[derive(Resource, Default)]
struct Arrival {
    age: f32,
}

#[derive(Resource, Default)]
struct Toast {
    text: Option<T>,
    left: f32,
}

#[derive(Resource, Default)]
struct HeroPose {
    moving: bool,
    facing_left: bool,
}

#[derive(Component)]
struct InLocation;

#[derive(Component)]
struct Hero;

/// A creature shifting its weight while it stands.
#[derive(Component)]
struct Idler {
    phase: f32,
    rest_y: f32,
}

#[derive(Component)]
struct Veil;

#[derive(Component)]
struct PlaceName;

#[derive(Component)]
struct HudHint;

#[derive(Component)]
struct ArrivalSheet;

#[derive(Component)]
struct ArrivalText;

#[derive(Component)]
struct Overlay;

#[derive(Component, Clone, Copy)]
enum PlayAction {
    Resume,
    Save,
    Keys,
    MainMenu,
    Retry,
    Sketch,
    Stay,
}

const SPEED: f32 = 72.0;
const FADE_SECONDS: f32 = 0.25;
/// How close to an exit the party must be for the interface to say where it leads.
const EXIT_HINT_REACH: f32 = 28.0;
const LETTERS_PER_SECOND: f32 = 70.0;
const ARRIVAL_LINGER: f32 = 4.0;
const HUD_HEIGHT: f32 = 14.0;

/// A point of the location, in pixels from its top-left corner, as a point of the screen.
fn on_screen(x: f32, y: f32) -> Vec2 {
    Vec2::new(x - CANVAS.x as f32 / 2.0, CANVAS.y as f32 / 2.0 - y)
}

/// Things lower on the screen are drawn over things higher up.
fn depth(y: f32) -> f32 {
    1.0 + y * 0.001
}

/// Puts a world on the play screen. Used by a new game and by loading a save.
pub fn begin(commands: &mut Commands, next: &mut NextState<Screen>, world: World, world_id: String) {
    commands.insert_resource(Game { world, world_id });
    commands.insert_resource(Phase::Arriving { t: 0.0 });
    next.set(Screen::Play);
}

fn autosave(store: &Store, game: &Game) {
    if let Err(e) = store.save_game(&game.world_id, &game.world, false) {
        error!("could not save the game: {e}");
    }
}

fn enter(mut commands: Commands, art: Res<Art>, fonts: Res<Fonts>, game: Option<Res<Game>>, store: Res<Store>, mut next: ResMut<NextState<Screen>>, mut backdrop: Query<&mut Visibility, With<Backdrop>>) {
    let Some(game) = game else {
        next.set(Screen::Main);
        return;
    };
    for mut visibility in &mut backdrop {
        *visibility = Visibility::Hidden;
    }
    autosave(&store, &game);
    commands.insert_resource(Redraw(true));
    commands.insert_resource(Arrival::default());
    commands.insert_resource(Toast::default());

    let full = Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), ..default() };
    commands.spawn((DespawnOnExit(Screen::Play), Veil, full.clone(), BackgroundColor(Color::BLACK), GlobalZIndex(5)));
    commands.spawn((DespawnOnExit(Screen::Play), Node { flex_direction: FlexDirection::Column, justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..full })).with_children(|root| {
        root.spawn((ArrivalSheet, Node { margin: UiRect::top(px(8)), ..default() })).with_children(|slot| {
            spawn_panel(slot, &art, Node { width: px(380), flex_direction: FlexDirection::Column, row_gap: px(4), ..default() }, |sheet| {
                sheet.spawn((Text::default(), PlaceName, fonts.title(1), TextColor(color(INK))));
                sheet.spawn((Text::default(), ArrivalText, fonts.body(), TextColor(color(INK))));
            });
        });
        // Pinned to the bottom edge: it must not move when the arrival sheet above it goes away.
        root.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                bottom: px(0),
                width: percent(100),
                height: px(HUD_HEIGHT),
                padding: UiRect::horizontal(px(6)),
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(color(INK)),
        ))
        .with_children(|strip| {
            strip.spawn((Text::default(), PlaceName, fonts.body(), TextColor(color(PARCHMENT))));
            strip.spawn((Text::default(), HudHint, fonts.body(), TextColor(color(PARCHMENT_SHADE))));
        });
    });
}

fn leave(mut commands: Commands, things: Query<Entity, Or<(With<InLocation>, With<Overlay>)>>, mut backdrop: Query<&mut Visibility, With<Backdrop>>) {
    for entity in &things {
        commands.entity(entity).despawn();
    }
    for mut visibility in &mut backdrop {
        *visibility = Visibility::Inherited;
    }
}

fn draw(mut commands: Commands, mut redraw: ResMut<Redraw>, game: Res<Game>, mut images: ResMut<Assets<Image>>, old: Query<Entity, With<InLocation>>) {
    if !std::mem::take(&mut redraw.0) {
        return;
    }
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let world = &game.world;
    let here = world.here();
    let ground = images.add(ground_picture(here, world).into_image());
    commands.spawn((InLocation, Sprite::from_image(ground), Anchor::TOP_LEFT, Transform::from_translation(on_screen(0.0, 0.0).extend(0.0))));

    let mut picture = |id: &str| world.sprites.get(id).or_else(|| world.sprites.get(MARKER_SPRITE)).map(|s| images.add(sprite_picture(s, &world.palette).into_image()));
    // Everything stands on the bottom edge of its cell, centred.
    let foot = |x: usize, y: usize| ((x as f32 + 0.5) * TILE as f32, (y as f32 + 1.0) * TILE as f32);
    for prop in &here.props {
        let (x, y) = foot(prop.x, prop.y);
        if let Some(image) = picture(&prop.sprite) {
            commands.spawn((InLocation, Sprite::from_image(image), Anchor::BOTTOM_CENTER, Transform::from_translation(on_screen(x, y).extend(depth(y)))));
        }
    }
    for (n, creature) in here.creatures.iter().enumerate() {
        let (x, y) = foot(creature.x, creature.y);
        if let Some(image) = picture(&creature.sprite) {
            let at = on_screen(x, y);
            commands.spawn((InLocation, Idler { phase: n as f32 * 0.37, rest_y: at.y }, Sprite::from_image(image), Anchor::BOTTOM_CENTER, Transform::from_translation(at.extend(depth(y)))));
        }
    }
    if let Some(image) = picture(HERO_SPRITE) {
        commands.spawn((InLocation, Hero, Sprite::from_image(image), Anchor::BOTTOM_CENTER, Transform::default()));
    }
}

fn place_hero(time: Res<Time>, game: Res<Game>, pose: Res<HeroPose>, mut hero: Query<(&mut Transform, &mut Sprite), With<Hero>>) {
    let player = game.world.player;
    // The sprite's feet sit at the bottom of the patch of ground the party takes up.
    let feet_y = player.y + FEET.1;
    // A walking figure rises a pixel on every other beat.
    let lift = if pose.moving && (time.elapsed_secs() * 9.0) as i32 % 2 == 0 { 1.0 } else { 0.0 };
    for (mut transform, mut sprite) in &mut hero {
        let at = on_screen(player.x.round(), feet_y.round());
        transform.translation = Vec3::new(at.x, at.y + lift, depth(feet_y));
        if sprite.flip_x != pose.facing_left {
            sprite.flip_x = pose.facing_left;
        }
    }
}

fn idle(time: Res<Time>, mut idlers: Query<(&Idler, &mut Transform)>) {
    for (idler, mut transform) in &mut idlers {
        let lift = if (time.elapsed_secs() + idler.phase) % 1.4 < 0.7 { 0.0 } else { 1.0 };
        if transform.translation.y != idler.rest_y + lift {
            transform.translation.y = idler.rest_y + lift;
        }
    }
}

fn walk(time: Res<Time>, keyboard: Res<ButtonInput<KeyCode>>, gamepads: Query<&Gamepad>, mut game: ResMut<Game>, mut phase: ResMut<Phase>, mut pose: ResMut<HeroPose>, mut warp: ResMut<Warp>) {
    if *phase != Phase::Walking {
        pose.moving = false;
        return;
    }
    if let Some((x, y)) = warp.0.take() {
        game.world.player = world::Player::in_cell(x.min(world::MAP_W - 1), y.min(world::MAP_H - 1));
    }
    if let Some(exit) = game.world.exit_here() {
        *phase = Phase::Leaving { to: exit.to.clone(), t: 0.0 };
        return;
    }
    let held = |keys: [KeyCode; 2]| keyboard.any_pressed(keys) as i32 as f32;
    let mut direction = Vec2::new(
        held([KeyCode::KeyD, KeyCode::ArrowRight]) - held([KeyCode::KeyA, KeyCode::ArrowLeft]),
        held([KeyCode::KeyS, KeyCode::ArrowDown]) - held([KeyCode::KeyW, KeyCode::ArrowUp]),
    );
    for pad in &gamepads {
        let stick = pad.left_stick();
        if stick.length() > 0.3 {
            direction += Vec2::new(stick.x, -stick.y);
        }
        direction.x += pad.pressed(GamepadButton::DPadRight) as i32 as f32 - pad.pressed(GamepadButton::DPadLeft) as i32 as f32;
        direction.y += pad.pressed(GamepadButton::DPadDown) as i32 as f32 - pad.pressed(GamepadButton::DPadUp) as i32 as f32;
    }
    if direction == Vec2::ZERO {
        pose.moving = false;
        return;
    }
    let step = direction.clamp_length_max(1.0) * SPEED * time.delta_secs();
    pose.moving = game.world.walk(step.x, step.y);
    if step.x != 0.0 {
        pose.facing_left = step.x < 0.0;
    }
}

/// The party has arrived somewhere that is drawn.
fn arrive(game: &mut Game, to: &str, store: &Store, redraw: &mut Redraw, arrival: &mut Arrival) {
    game.world.travel(to);
    redraw.0 = true;
    arrival.age = 0.0;
    autosave(store, game);
}

#[allow(clippy::too_many_arguments)]
fn advance(
    time: Res<Time>,
    mut phase: ResMut<Phase>,
    mut game: ResMut<Game>,
    store: Res<Store>,
    gm: Res<Gm>,
    settings: Res<Settings>,
    master: Res<Master>,
    mut redraw: ResMut<Redraw>,
    mut arrival: ResMut<Arrival>,
) {
    let tick = time.delta_secs() / FADE_SECONDS;
    let next = match &*phase {
        Phase::Leaving { to, t } if t + tick >= 1.0 => {
            if game.world.locations.contains_key(to) {
                arrive(&mut game, to, &store, &mut redraw, &mut arrival);
                Phase::Arriving { t: 0.0 }
            } else {
                master.ask(&gm, &store, &settings, Ask::Location { world: Box::new(game.world.clone()), id: to.clone(), from: game.world.current.clone() });
                Phase::Drawing { to: to.clone() }
            }
        }
        Phase::Leaving { to, t } => Phase::Leaving { to: to.clone(), t: t + tick },
        Phase::Arriving { t } if t + tick >= 1.0 => Phase::Walking,
        Phase::Arriving { t } => Phase::Arriving { t: t + tick },
        _ => return,
    };
    *phase = next;
}

/// Adds a drawn place to the world and walks the party into it, or says why it cannot.
fn settle(draft: Result<Box<LocationDraft>, Trouble>, id: &str, from: &str, game: &mut Game, store: &Store, redraw: &mut Redraw, arrival: &mut Arrival) -> Phase {
    let added = draft.and_then(|d| game.world.add_location(&d, id, Some(from)).map_err(Trouble::Unreadable));
    match added {
        Ok(notes) => {
            for note in notes {
                info!("place '{id}' repaired: {note}");
            }
            arrive(game, id, store, redraw, arrival);
            Phase::Arriving { t: 0.0 }
        }
        Err(trouble) => Phase::Stuck { to: id.to_string(), trouble },
    }
}

fn receive(master: Res<Master>, mut phase: ResMut<Phase>, mut game: ResMut<Game>, store: Res<Store>, mut redraw: ResMut<Redraw>, mut arrival: ResMut<Arrival>) {
    for answer in master.take() {
        let Answer::Location { id, from, draft } = answer else { continue };
        // An answer for a place the party is no longer waiting on is of no use.
        if *phase != (Phase::Drawing { to: id.clone() }) {
            continue;
        }
        *phase = settle(draft, &id, &from, &mut game, &store, &mut redraw, &mut arrival);
    }
}

fn toggle_pause(keyboard: Res<ButtonInput<KeyCode>>, gamepads: Query<&Gamepad>, mut phase: ResMut<Phase>) {
    if !(keyboard.just_pressed(KeyCode::Escape) || gamepads.iter().any(|pad| pad.just_pressed(GamepadButton::Start))) {
        return;
    }
    match *phase {
        Phase::Walking => *phase = Phase::Paused,
        Phase::Paused => *phase = Phase::Walking,
        _ => {}
    }
}

fn veil(phase: Res<Phase>, mut veils: Query<&mut BackgroundColor, With<Veil>>) {
    let darkness = match &*phase {
        Phase::Walking => 0.0,
        Phase::Leaving { t, .. } => *t,
        Phase::Arriving { t } => 1.0 - t,
        Phase::Drawing { .. } | Phase::Stuck { .. } => 0.8,
        Phase::Paused => 0.55,
    };
    // A fade in a few flat steps reads as pixel art; a smooth one does not.
    let stepped = (darkness.clamp(0.0, 1.0) * 5.0).round() / 5.0;
    for mut background in &mut veils {
        if background.0.alpha() != stepped {
            background.0 = Color::srgba(0.0, 0.0, 0.0, stepped);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn hud(
    time: Res<Time>,
    game: Res<Game>,
    phase: Res<Phase>,
    lang: Res<Lang>,
    mut toast: ResMut<Toast>,
    mut arrival: ResMut<Arrival>,
    mut names: Query<&mut Text, (With<PlaceName>, Without<HudHint>, Without<ArrivalText>)>,
    mut hints: Query<&mut Text, (With<HudHint>, Without<PlaceName>, Without<ArrivalText>)>,
    mut described: Query<&mut Text, (With<ArrivalText>, Without<PlaceName>, Without<HudHint>)>,
    mut sheets: Query<&mut Node, With<ArrivalSheet>>,
) {
    let world = &game.world;
    let here = world.here();
    for mut text in &mut names {
        if text.0 != here.name {
            text.0 = here.name.clone();
        }
    }
    toast.left = (toast.left - time.delta_secs()).max(0.0);
    let hint = match (toast.text.filter(|_| toast.left > 0.0), world.exit_near(EXIT_HINT_REACH)) {
        (Some(said), _) => said.text(*lang).to_string(),
        (None, Some(exit)) if exit.label.is_empty() => format!("→ {}", world.place_name(&exit.to)),
        (None, Some(exit)) => format!("{} → {}", exit.label, world.place_name(&exit.to)),
        (None, None) => T::HudMenuHint.text(*lang).to_string(),
    };
    for mut text in &mut hints {
        if text.0 != hint {
            text.0 = hint.clone();
        }
    }
    arrival.age += time.delta_secs();
    let letters = here.description.chars().count();
    let shown: String = here.description.chars().take((arrival.age * LETTERS_PER_SECOND) as usize).collect();
    for mut text in &mut described {
        if text.0 != shown {
            text.0 = shown.clone();
        }
    }
    // The sheet is for the place the party is in; it steps aside for any other sheet.
    let in_the_open = matches!(*phase, Phase::Walking | Phase::Arriving { .. });
    let visible = in_the_open && arrival.age < letters as f32 / LETTERS_PER_SECOND + ARRIVAL_LINGER;
    for mut node in &mut sheets {
        let display = if visible { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
}

/// Which sheet, if any, lies over the game.
#[derive(Clone, PartialEq, Default)]
enum Sheet {
    #[default]
    None,
    Drawing(String),
    Stuck(String),
    Paused,
}

#[allow(clippy::too_many_arguments)]
fn overlay(mut commands: Commands, phase: Res<Phase>, game: Res<Game>, gm: Res<Gm>, lang: Res<Lang>, art: Res<Art>, fonts: Res<Fonts>, mut shown: Local<Sheet>, old: Query<Entity, With<Overlay>>) {
    let wanted = match &*phase {
        Phase::Drawing { to } => Sheet::Drawing(game.world.place_name(to).to_string()),
        Phase::Stuck { trouble, .. } => Sheet::Stuck(trouble_text(trouble, gm.dispatcher.now(), *lang)),
        Phase::Paused => Sheet::Paused,
        _ => Sheet::None,
    };
    if *shown == wanted && (wanted == Sheet::None || !old.is_empty()) {
        return;
    }
    for entity in &old {
        commands.entity(entity).despawn();
    }
    *shown = wanted.clone();
    if wanted == Sheet::None {
        return;
    }
    let ink = TextColor(color(INK));
    let full = Node {
        position_type: PositionType::Absolute,
        left: px(0),
        top: px(0),
        width: percent(100),
        height: percent(100),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    };
    commands.spawn((Overlay, full, GlobalZIndex(10))).with_children(|root| {
        let sheet = Node { width: px(220), flex_direction: FlexDirection::Column, row_gap: px(4), ..default() };
        spawn_panel(root, &art, sheet, |sheet| match &wanted {
            Sheet::Drawing(place) => {
                sheet.spawn((Text::default(), T::Drawing, fonts.body(), ink));
                sheet.spawn((Text::new(place.clone()), fonts.title(1), ink));
                sheet.spawn((Text::default(), T::DrawingHint, fonts.body(), TextColor(color(FADED))));
            }
            Sheet::Stuck(why) => {
                sheet.spawn((Text::default(), T::StuckTitle, fonts.title(1), ink));
                sheet.spawn((Text::new(why.clone()), fonts.body(), TextColor(color(WAX))));
                button(sheet, &art, &fonts, T::Recheck, true, PlayAction::Retry);
                button(sheet, &art, &fonts, T::Sketch, true, PlayAction::Sketch);
                button(sheet, &art, &fonts, T::Keys, true, PlayAction::Keys);
                button(sheet, &art, &fonts, T::StayHere, true, PlayAction::Stay);
            }
            Sheet::Paused => {
                sheet.spawn((Text::default(), T::Paused, fonts.title(1), ink));
                button(sheet, &art, &fonts, T::Continue, true, PlayAction::Resume);
                button(sheet, &art, &fonts, T::SaveGame, true, PlayAction::Save);
                button(sheet, &art, &fonts, T::Keys, true, PlayAction::Keys);
                button(sheet, &art, &fonts, T::ToMainMenu, true, PlayAction::MainMenu);
            }
            Sheet::None => {}
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn on_activate(
    used: On<Activate>,
    buttons: Query<&PlayAction>,
    mut phase: ResMut<Phase>,
    game: Option<ResMut<Game>>,
    mut toast: ResMut<Toast>,
    mut redraw: ResMut<Redraw>,
    mut arrival: ResMut<Arrival>,
    mut return_to: ResMut<ReturnTo>,
    mut next: ResMut<NextState<Screen>>,
    store: Res<Store>,
    gm: Res<Gm>,
    settings: Res<Settings>,
    master: Res<Master>,
) {
    // This observer hears every button in the game; outside a game there is nothing to act on.
    let (Ok(action), Some(mut game)) = (buttons.get(used.entity), game) else { return };
    match (*action, phase.clone()) {
        (PlayAction::Resume, _) => *phase = Phase::Walking,
        (PlayAction::Save, _) => {
            match store.save_game(&game.world_id, &game.world, true) {
                Ok(_) => *toast = Toast { text: Some(T::Saved), left: 3.0 },
                Err(e) => error!("could not save the game: {e}"),
            }
            *phase = Phase::Walking;
        }
        (PlayAction::Keys, _) => {
            // The game stays in memory; the key screen's Back button comes back here.
            return_to.0 = Some(Screen::Play);
            next.set(Screen::Keys);
        }
        (PlayAction::MainMenu, _) => {
            autosave(&store, &game);
            *phase = Phase::Walking;
            next.set(Screen::Main);
        }
        (PlayAction::Retry, Phase::Stuck { to, .. }) => {
            master.ask(&gm, &store, &settings, Ask::Location { world: Box::new(game.world.clone()), id: to.clone(), from: game.world.current.clone() });
            *phase = Phase::Drawing { to };
        }
        (PlayAction::Sketch, Phase::Stuck { to, .. }) => {
            // The same rule-made place the fake master draws: plain, but the way on is open.
            let from = game.world.current.clone();
            let name = game.world.place_name(&to).to_string();
            let draft = serde_json::from_str::<LocationDraft>(&fake::location(&to, &name, &from)).map(Box::new).map_err(|e| Trouble::Unreadable(e.to_string()));
            *phase = settle(draft, &to, &from, &mut game, &store, &mut redraw, &mut arrival);
        }
        (PlayAction::Stay, _) => {
            game.world.step_back();
            *phase = Phase::Arriving { t: 0.0 };
        }
        _ => {}
    }
}

pub struct PlayPlugin;

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Phase>()
            .init_resource::<Redraw>()
            .init_resource::<Arrival>()
            .init_resource::<Toast>()
            .init_resource::<HeroPose>()
            .add_systems(OnEnter(Screen::Play), enter)
            .add_systems(OnExit(Screen::Play), leave)
            .add_systems(Update, (toggle_pause, walk, advance, receive, draw, place_hero, idle, hud, veil, overlay).chain().run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>))
            .add_observer(on_activate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_locations_corner_is_the_screens_corner_and_y_is_flipped() {
        assert_eq!(on_screen(0.0, 0.0), Vec2::new(-240.0, 135.0));
        assert_eq!(on_screen(480.0, 256.0), Vec2::new(240.0, -121.0));
    }

    #[test]
    fn lower_things_are_drawn_in_front_and_all_stand_above_the_ground() {
        assert!(depth(200.0) > depth(100.0));
        assert!(depth(0.0) > 0.0);
    }
}
