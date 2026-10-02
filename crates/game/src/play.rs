//! Playing: the current location on screen, the party walking about it, the passage from one
//! place to the next, and the conversation with the game master.

use crate::art::{Art, ground_picture, sprite_picture};
use crate::coop::{Down, Intent, Session, Table, player_name};
use crate::gm::{Gm, Trouble, trouble_text};
use crate::i18n::{Lang, T};
use crate::master::{Answer, Ask, Master};
use crate::menu::{Backdrop, ReturnTo, Screen, spawn_panel};
use crate::screen::CANVAS;
use crate::shot::Warp;
use crate::store::{Initiative, Settings, Store};
use crate::theme::*;
use crate::widgets::{Activate, BUTTON_HEIGHT, backing, button, scroll_list, text_button};
use bevy::input_focus::{AutoFocus, InputFocus};
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::text::{EditableText, TextCursorStyle};
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigation;
use world::base::{HERO_SPRITE, MARKER_SPRITE};
use world::walk::FEET;
use crate::hero::sheet_lines;
use rules::{Ability, Rng, Side, tr};
use world::{LocationDraft, Speaker, TILE, Turn, World, fake};

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
    Leaving { to: String, t: f32 },
    /// The game master is drawing `to`, which had only a name until now.
    Drawing { to: String },
    /// The screen lightens on the new place.
    Arriving { t: f32 },
    /// The place beyond the exit could not be drawn; the player decides what to do.
    Stuck { to: String, trouble: Trouble },
    Paused,
    /// The party is looking at a thing and choosing what to do with it.
    Looking { thing: String },
    /// The player is typing what the party does.
    Writing { near: Option<String> },
    /// The game master is working out what happens.
    Asking { said: String, near: Option<String> },
    /// The players are reading what happened.
    Reading { turn: Turn },
    /// The master could not answer what the party did.
    Refused { said: String, near: Option<String>, trouble: Trouble },
    /// The story so far, and what the party carries.
    Journal,
    /// The master called for a check; the die has not been rolled yet.
    Rolling,
    /// A fight is on; the fight module runs the screen.
    Fighting,
    /// The character sheet.
    Sheet,
}

#[derive(Resource, Default)]
pub struct Redraw(pub bool);

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

/// Work the master does without being asked: drawing places ahead, events, chapters.
#[derive(Resource, Default)]
struct Background {
    /// Seconds of play since the master last spoke.
    quiet: f32,
    /// Seconds before another place may be drawn ahead; set after a refusal.
    ahead_rest: f32,
    chapter_asked: bool,
    chapter_rest: f32,
    event_asked: bool,
}

#[derive(Component)]
struct InLocation;

#[derive(Component)]
struct Hero;

/// Another player's token.
#[derive(Component)]
struct Fellow(String);

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

#[derive(Component)]
struct SayInput;

#[derive(Component, Clone, Copy)]
enum PlayAction {
    Resume,
    Save,
    Keys,
    MainMenu,
    Retry,
    Sketch,
    Stay,
    /// A prepared action of the thing being looked at.
    Do(usize),
    /// Open the field for the party's own words.
    Own,
    Send,
    /// One of the choices offered with the last telling.
    Choose(usize),
    /// Back to walking.
    Close,
    AskAgain,
    /// Roll the check that is waiting.
    Roll,
    /// Put an earned point into an ability of the first hero who has points to place.
    Improve(Ability),
    /// A night's rest.
    Rest,
}

const SPEED: f32 = 72.0;
const FADE_SECONDS: f32 = 0.25;
/// How close to an exit the party must be for the interface to say where it leads.
const EXIT_HINT_REACH: f32 = 28.0;
/// How close to a thing the party must be to interact with it.
const THING_REACH: f32 = 26.0;
const LETTERS_PER_SECOND: f32 = 70.0;
const ARRIVAL_LINGER: f32 = 4.0;
pub const HUD_HEIGHT: f32 = 14.0;
/// A hostile creature this many cells away notices the party and attacks.
const NOTICE_CELLS: u32 = 3;
/// After the master refuses unasked-for work, how long before it is offered again.
const BACKGROUND_REST: f32 = 600.0;
const JOURNAL_LINES: usize = 80;

/// A point of the location, in pixels from its top-left corner, as a point of the screen.
pub fn on_screen(x: f32, y: f32) -> Vec2 {
    Vec2::new(x - CANVAS.x as f32 / 2.0, CANVAS.y as f32 / 2.0 - y)
}

/// Things lower on the screen are drawn over things higher up.
pub fn depth(y: f32) -> f32 {
    1.0 + y * 0.001
}

/// Seconds of silence after which the master may make something happen.
fn event_after(initiative: Initiative) -> Option<f32> {
    match initiative {
        Initiative::Off => None,
        Initiative::Rare => Some(600.0),
        Initiative::Often => Some(240.0),
    }
}

/// Puts a world on the play screen. Used by a new game and by loading a save.
pub fn begin(commands: &mut Commands, next: &mut NextState<Screen>, world: World, world_id: String) {
    // A world without a character goes to the character screen first.
    let screen = if world.party.is_empty() { Screen::Hero } else { Screen::Play };
    commands.insert_resource(Phase::Arriving { t: 0.0 });
    commands.insert_resource(Game { world, world_id });
    commands.insert_resource(Background::default());
    next.set(screen);
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
    commands.spawn((DespawnOnExit(Screen::Play), Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..full })).with_children(|root| {
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
        // In a fight a creature is where the encounter says it is, and the dead are gone.
        let fighter = world.fight.as_ref().and_then(|f| f.combatants.iter().find(|c| c.id == creature.id));
        if fighter.is_some_and(|c| c.dead) {
            continue;
        }
        let (cx, cy) = fighter.map_or((creature.x, creature.y), |c| (c.x.max(0) as usize, c.y.max(0) as usize));
        let (x, y) = foot(cx, cy);
        if let Some(image) = picture(&creature.sprite) {
            let at = on_screen(x, y);
            commands.spawn((InLocation, Idler { phase: n as f32 * 0.37, rest_y: at.y }, Sprite::from_image(image), Anchor::BOTTOM_CENTER, Transform::from_translation(at.extend(depth(y)))));
        }
    }
    match &world.fight {
        // In a fight every hero stands in a cell of its own; the fallen are darkened.
        Some(fight) => {
            for c in fight.combatants.iter().filter(|c| c.side == Side::Party && !c.dead) {
                let (x, y) = foot(c.x.max(0) as usize, c.y.max(0) as usize);
                if let Some(image) = picture(HERO_SPRITE) {
                    let tint = if c.hp > 0 { Color::WHITE } else { Color::srgb(0.35, 0.3, 0.3) };
                    commands.spawn((InLocation, Sprite { color: tint, flip_y: c.hp <= 0, ..Sprite::from_image(image) }, Anchor::BOTTOM_CENTER, Transform::from_translation(on_screen(x, y).extend(depth(y)))));
                }
            }
        }
        None => {
            for other in &world.others {
                if let Some(image) = picture(HERO_SPRITE) {
                    commands.spawn((InLocation, Fellow(other.name.clone()), Sprite { color: Color::srgb(0.8, 0.85, 1.0), ..Sprite::from_image(image) }, Anchor::BOTTOM_CENTER, Transform::default()));
                }
            }
            if let Some(image) = picture(HERO_SPRITE) {
                commands.spawn((InLocation, Hero, Sprite::from_image(image), Anchor::BOTTOM_CENTER, Transform::default()));
            }
        }
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

/// Other players' tokens follow where their players last said they were. When somebody
/// joins or leaves, the picture is drawn again so that there is a token for each.
fn place_fellows(game: Res<Game>, mut redraw: ResMut<Redraw>, mut fellows: Query<(&Fellow, &mut Transform)>, mut was_fighting: Local<bool>) {
    let others = &game.world.others;
    // A fight beginning or ending changes who is drawn where.
    let fighting = game.world.fight.is_some();
    if std::mem::replace(&mut *was_fighting, fighting) != fighting {
        redraw.0 = true;
    }
    if game.world.fight.is_none() && fellows.iter().count() != others.len() {
        redraw.0 = true;
    }
    for (fellow, mut transform) in &mut fellows {
        if let Some(token) = others.iter().find(|t| t.name == fellow.0) {
            let feet_y = token.y + FEET.1;
            let at = on_screen(token.x.round(), feet_y.round());
            transform.translation = Vec3::new(at.x, at.y, depth(feet_y));
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

#[allow(clippy::too_many_arguments)]
fn walk(time: Res<Time>, keyboard: Res<ButtonInput<KeyCode>>, gamepads: Query<&Gamepad>, mut game: ResMut<Game>, mut phase: ResMut<Phase>, mut pose: ResMut<HeroPose>, mut warp: ResMut<Warp>, session: Res<Session>, mut table: ResMut<Table>) {
    if *phase != Phase::Walking {
        pose.moving = false;
        return;
    }
    // Something for everyone to read comes before walking on.
    if let Some(turn) = table.unread.pop_front() {
        *phase = Phase::Reading { turn };
        return;
    }
    if session.is_client() {
        // A guest learns of fights and checks from the world the host sends.
        let next = settled(&game.world);
        if next != Phase::Walking {
            *phase = next;
            return;
        }
        if let Some(exit) = game.world.exit_here() {
            let to = exit.to.clone();
            session.intend(Intent::Exit { to: to.clone() });
            // The host moves the party; until its world arrives the guest steps back, so
            // that a refusal does not leave it standing in the doorway asking again.
            game.world.step_back();
            let _ = to;
            return;
        }
    }
    if let Some((x, y)) = warp.0.take() {
        game.world.player = world::Player::in_cell(x.min(world::MAP_W - 1), y.min(world::MAP_H - 1));
    }
    if !session.is_client()
        && let Some(exit) = game.world.exit_here()
    {
        *phase = Phase::Leaving { to: exit.to.clone(), t: 0.0 };
        return;
    }
    // Something hostile close by does not wait to be spoken to. With guests at the table
    // any of their tokens can be the one that is noticed.
    let noticed = game.world.hostile_near(NOTICE_CELLS) || game.world.others.iter().any(|t| hostile_near_token(&game.world, t.x, t.y));
    if !session.is_client() && noticed && game.world.start_fight(Rng::from_clock()) {
        *phase = Phase::Fighting;
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

/// Keys that open something while walking: interact, the journal, the pause menu. Escape
/// also closes whatever sheet is open.
fn keys(keyboard: Res<ButtonInput<KeyCode>>, gamepads: Query<&Gamepad>, game: Res<Game>, mut phase: ResMut<Phase>, mut was_walking: Local<bool>) {
    let pad = |button: GamepadButton| gamepads.iter().any(|pad| pad.just_pressed(button));
    // The press that closes a sheet must not also act on the world behind it: a key only
    // opens something if the party was already walking on the frame before.
    let was_settled = std::mem::replace(&mut *was_walking, *phase == Phase::Walking);
    if keyboard.just_pressed(KeyCode::Escape) || pad(GamepadButton::Start) {
        let next = match &*phase {
            Phase::Walking => Phase::Paused,
            Phase::Paused | Phase::Looking { .. } | Phase::Writing { .. } | Phase::Journal | Phase::Sheet => Phase::Walking,
            // A telling with choices stays until one is made or it is closed by its button.
            Phase::Reading { turn } if turn.options.is_empty() => settled(&game.world),
            _ => return,
        };
        *phase = next;
        return;
    }
    if *phase != Phase::Walking || !was_settled {
        return;
    }
    if keyboard.any_just_pressed([KeyCode::KeyE, KeyCode::Enter, KeyCode::NumpadEnter]) || pad(GamepadButton::South) {
        *phase = match game.world.thing_near(THING_REACH) {
            Some(thing) => Phase::Looking { thing: thing.to_string() },
            None => Phase::Writing { near: None },
        };
    } else if keyboard.just_pressed(KeyCode::KeyJ) || pad(GamepadButton::North) {
        *phase = Phase::Journal;
    } else if (keyboard.just_pressed(KeyCode::KeyC) || pad(GamepadButton::West)) && !game.world.party.is_empty() {
        *phase = Phase::Sheet;
    }
}

fn hostile_near_token(world: &World, x: f32, y: f32) -> bool {
    let cell = ((x / TILE as f32).floor() as i32, (y / TILE as f32).floor() as i32);
    world.hostiles().any(|c| rules::combat::distance((c.x as i32, c.y as i32), cell) <= NOTICE_CELLS)
}

/// Puts a telling in front of this player: at once when they are free to read, otherwise
/// after what they are reading now.
pub fn show(turn: Turn, phase: &mut Phase, table: &mut Table) {
    if matches!(*phase, Phase::Walking | Phase::Asking { .. } | Phase::Looking { .. } | Phase::Rolling) {
        *phase = Phase::Reading { turn };
    } else {
        table.unread.push_back(turn);
    }
}

/// What the screen should be doing once a sheet is closed: a fight that broke out, a check
/// that is waiting, or simply walking on.
fn settled(world: &World) -> Phase {
    if world.fight.is_some() {
        Phase::Fighting
    } else if world.pending.is_some() {
        Phase::Rolling
    } else {
        Phase::Walking
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
                // A place already being drawn ahead is waited for, not asked for twice.
                if !master.is_drawing(to) {
                    master.ask(&gm, &store, &settings, Ask::Location { world: Box::new(game.world.clone()), id: to.clone(), from: game.world.current.clone(), ahead: false });
                }
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

/// The master told something: it is shown, the picture is brought up to date, the game saved.
#[allow(clippy::too_many_arguments)]
fn told(mut turn: Turn, notes: Vec<String>, game: &mut Game, store: &Store, redraw: &mut Redraw, background: &mut Background, session: &Session, table: &mut Table) -> Phase {
    for note in notes {
        info!("turn repaired: {note}");
    }
    // Levels gained from the story are told with it.
    for (name, up) in game.world.level_ups.drain(..) {
        turn.narration.push_str(&format!("\n★ {name}: {}", up.level));
    }
    redraw.0 = true;
    background.quiet = 0.0;
    autosave(store, game);
    // The whole table reads what the master tells, and may take up the choices it offers.
    table.options = turn.options.clone();
    session.broadcast(&Down::Told(turn.clone()));
    Phase::Reading { turn }
}

#[allow(clippy::too_many_arguments)]
fn receive(
    master: Res<Master>,
    gm: Res<Gm>,
    settings: Res<Settings>,
    mut phase: ResMut<Phase>,
    mut game: ResMut<Game>,
    store: Res<Store>,
    mut redraw: ResMut<Redraw>,
    mut arrival: ResMut<Arrival>,
    mut background: ResMut<Background>,
    session: Res<Session>,
    mut table: ResMut<Table>,
) {
    for answer in master.take() {
        match answer {
            Answer::Location { id, from, ahead, draft } => {
                let waited_for = *phase == (Phase::Drawing { to: id.clone() });
                if waited_for && ahead && draft.is_err() {
                    // Drawn ahead with the best models only, and they refused. Now that the
                    // party is at the door the request is made in earnest.
                    master.ask(&gm, &store, &settings, Ask::Location { world: Box::new(game.world.clone()), id, from, ahead: false });
                } else if waited_for {
                    *phase = settle(draft, &id, &from, &mut game, &store, &mut redraw, &mut arrival);
                } else if game.world.stubs.contains_key(&id) {
                    match draft.and_then(|d| game.world.add_location(&d, &id, Some(&from)).map_err(Trouble::Unreadable)) {
                        Ok(_) => autosave(&store, &game),
                        Err(trouble) => {
                            info!("place '{id}' was not drawn ahead: {trouble:?}");
                            background.ahead_rest = BACKGROUND_REST;
                        }
                    }
                }
            }
            Answer::Turn { said, near, draft } => {
                let mine = matches!(&*phase, Phase::Asking { said: asked, .. } if *asked == said);
                let for_a_guest = table.busy.as_deref() == Some(said.as_str());
                if !mine && !for_a_guest {
                    continue;
                }
                table.busy = None;
                match draft {
                    Ok(draft) => {
                        let (turn, notes) = game.world.take_turn(&said, &draft);
                        let reading = told(turn.clone(), notes, &mut game, &store, &mut redraw, &mut background, &session, &mut table);
                        if mine {
                            *phase = reading;
                        } else {
                            show(turn, &mut phase, &mut table);
                        }
                    }
                    Err(trouble) if mine => *phase = Phase::Refused { said, near, trouble },
                    Err(trouble) => session.broadcast(&Down::Trouble(trouble_text(&trouble, gm.dispatcher.now(), settings.lang))),
                }
            }
            Answer::Event(draft) => {
                background.event_asked = false;
                background.quiet = 0.0;
                // An event interrupts a walk, never a conversation or a menu.
                if let (Ok(draft), Phase::Walking) = (draft, &*phase)
                    && !draft.narration.trim().is_empty()
                {
                    let (turn, notes) = game.world.take_turn("", &draft);
                    *phase = told(turn, notes, &mut game, &store, &mut redraw, &mut background, &session, &mut table);
                }
            }
            Answer::Chapter(draft) => {
                background.chapter_asked = false;
                match draft {
                    Ok(draft) if !draft.chapter.trim().is_empty() => {
                        game.world.story.close_chapter(&draft.chapter, &draft.facts);
                        autosave(&store, &game);
                    }
                    _ => background.chapter_rest = BACKGROUND_REST,
                }
            }
            Answer::World(_) => {}
        }
    }
}

/// Work done unasked while the party walks: the nearest undrawn neighbour is drawn, a long
/// story is written up, and after a long silence the master may make something happen.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn background_work(time: Res<Time>, phase: Res<Phase>, game: Res<Game>, gm: Res<Gm>, store: Res<Store>, settings: Res<Settings>, master: Res<Master>, mut background: ResMut<Background>, session: Res<Session>) {
    if session.is_client() {
        return;
    }
    let dt = time.delta_secs();
    background.ahead_rest = (background.ahead_rest - dt).max(0.0);
    background.chapter_rest = (background.chapter_rest - dt).max(0.0);
    if *phase != Phase::Walking {
        return;
    }
    background.quiet += dt;
    let world = &game.world;
    if !background.chapter_asked && background.chapter_rest == 0.0 && world.story.needs_chapter(settings.memory_budget) {
        background.chapter_asked = true;
        master.ask(&gm, &store, &settings, Ask::Chapter { world: Box::new(world.clone()) });
    }
    if settings.predraw && background.ahead_rest == 0.0 && !master.is_drawing_anything() {
        // Exits are tried in the order the master listed them.
        if let Some(exit) = world.here().exits.iter().find(|e| world.stubs.contains_key(&e.to)) {
            master.ask(&gm, &store, &settings, Ask::Location { world: Box::new(world.clone()), id: exit.to.clone(), from: world.current.clone(), ahead: true });
        }
    }
    if let Some(after) = event_after(settings.initiative)
        && background.quiet >= after
        && !background.event_asked
    {
        background.event_asked = true;
        master.ask(&gm, &store, &settings, Ask::Event { world: Box::new(world.clone()) });
    }
}

fn veil(phase: Res<Phase>, mut veils: Query<&mut BackgroundColor, With<Veil>>) {
    let darkness = match &*phase {
        Phase::Walking | Phase::Looking { .. } | Phase::Writing { .. } | Phase::Reading { .. } | Phase::Asking { .. } | Phase::Rolling | Phase::Fighting => 0.0,
        Phase::Leaving { t, .. } => *t,
        Phase::Arriving { t } => 1.0 - t,
        Phase::Drawing { .. } | Phase::Stuck { .. } => 0.8,
        Phase::Paused | Phase::Journal | Phase::Refused { .. } | Phase::Sheet => 0.55,
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
    let thing_name = world.thing_near(THING_REACH).and_then(|id| here.props.iter().map(|p| (&p.id, &p.name)).chain(here.creatures.iter().map(|c| (&c.id, &c.name))).find(|(i, _)| i.as_str() == id).map(|(_, name)| name.clone()));
    let hint = match (toast.text.filter(|_| toast.left > 0.0), thing_name, world.exit_near(EXIT_HINT_REACH)) {
        (Some(said), _, _) => said.text(*lang).to_string(),
        (None, Some(name), _) => format!("E: {name}"),
        (None, None, Some(exit)) if exit.label.is_empty() => format!("→ {}", world.place_name(&exit.to)),
        (None, None, Some(exit)) => format!("{} → {}", exit.label, world.place_name(&exit.to)),
        (None, None, None) => T::HudHint.text(*lang).to_string(),
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

/// Which sheet, if any, lies over the game, with everything it shows: when this value
/// changes the sheet is rebuilt.
#[derive(Clone, PartialEq, Default)]
enum Sheet {
    #[default]
    None,
    Drawing(String),
    Stuck(String),
    Paused,
    Looking { name: String, description: String, actions: Vec<String> },
    Writing,
    Asking(String),
    Reading(Turn),
    Refused(String),
    Journal { carried: String, lines: Vec<(Speaker, String)> },
    /// What is being rolled, in words.
    Rolling(String),
    /// The sheets of the party, and whether there are points to place.
    Character { lines: Vec<String>, points: bool },
}

fn sheet_for(phase: &Phase, world: &World, now: u64, lang: Lang) -> Sheet {
    match phase {
        Phase::Drawing { to } => Sheet::Drawing(world.place_name(to).to_string()),
        Phase::Stuck { trouble, .. } => Sheet::Stuck(trouble_text(trouble, now, lang)),
        Phase::Paused => Sheet::Paused,
        Phase::Looking { thing } => {
            let here = world.here();
            let found = here
                .props
                .iter()
                .find(|p| &p.id == thing)
                .map(|p| (&p.name, &p.description, &p.actions))
                .or_else(|| here.creatures.iter().find(|c| &c.id == thing).map(|c| (&c.name, &c.description, &c.actions)));
            match found {
                Some((name, description, actions)) => Sheet::Looking { name: name.clone(), description: description.clone(), actions: actions.iter().map(|a| a.label.clone()).collect() },
                None => Sheet::None,
            }
        }
        Phase::Writing { .. } => Sheet::Writing,
        Phase::Asking { said, .. } => Sheet::Asking(said.clone()),
        Phase::Reading { turn } => Sheet::Reading(turn.clone()),
        Phase::Refused { trouble, .. } => Sheet::Refused(trouble_text(trouble, now, lang)),
        Phase::Journal => {
            let carried = world.story.inventory.iter().map(|i| if i.count > 1 { format!("{} ×{}", i.name, i.count) } else { i.name.clone() }).collect::<Vec<_>>().join(", ");
            let skip = world.story.log.len().saturating_sub(JOURNAL_LINES);
            Sheet::Journal { carried, lines: world.story.log.iter().skip(skip).map(|l| (l.speaker, l.text.clone())).collect() }
        }
        Phase::Rolling => match &world.pending {
            Some(check) => {
                let ru = lang == Lang::Ru;
                let skill = check.skill.as_deref().and_then(|s| rules::srd().skill(s)).map(|s| format!(" ({})", tr(ru, &s.name))).unwrap_or_default();
                Sheet::Rolling(format!("{}{skill} — {} {}", tr(ru, check.ability.name()), T::Difficulty.text(lang), check.difficulty))
            }
            None => Sheet::None,
        },
        Phase::Sheet => {
            let mut lines = Vec::new();
            for hero in &world.party {
                lines.extend(sheet_lines(hero, lang));
                lines.push(String::new());
            }
            lines.pop();
            Sheet::Character { lines, points: world.party.iter().any(|h| h.points_to_place > 0) }
        }
        _ => Sheet::None,
    }
}

#[allow(clippy::too_many_arguments)]
fn overlay(mut commands: Commands, phase: Res<Phase>, game: Res<Game>, gm: Res<Gm>, lang: Res<Lang>, art: Res<Art>, fonts: Res<Fonts>, mut shown: Local<Sheet>, old: Query<Entity, With<Overlay>>) {
    // A countdown in a refusal would rebuild the sheet every second and swallow clicks, so
    // the time is fixed when the sheet first appears.
    let wanted = match (&*phase, &*shown) {
        (Phase::Stuck { .. }, Sheet::Stuck(_)) | (Phase::Refused { .. }, Sheet::Refused(_)) if !old.is_empty() => shown.clone(),
        _ => sheet_for(&phase, &game.world, gm.dispatcher.now(), *lang),
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
    let faded = TextColor(color(FADED));
    // Sheets about the story sit at the bottom, where the eye rests while walking; the rest
    // take the middle of the screen.
    let low = matches!(wanted, Sheet::Looking { .. } | Sheet::Writing | Sheet::Asking(_) | Sheet::Reading(_) | Sheet::Rolling(_));
    let full = Node {
        position_type: PositionType::Absolute,
        left: px(0),
        top: px(0),
        width: percent(100),
        height: percent(100),
        padding: UiRect::bottom(px(HUD_HEIGHT + 4.0)),
        justify_content: JustifyContent::Center,
        align_items: if low { AlignItems::FlexEnd } else { AlignItems::Center },
        ..default()
    };
    commands.spawn((Overlay, full, GlobalZIndex(10))).with_children(|root| {
        let sheet = Node { width: px(if low || matches!(wanted, Sheet::Journal { .. } | Sheet::Character { .. }) { 440.0 } else { 240.0 }), flex_direction: FlexDirection::Column, row_gap: px(4), ..default() };
        spawn_panel(root, &art, sheet, |sheet| match &wanted {
            Sheet::Drawing(place) => {
                sheet.spawn((Text::default(), T::Drawing, fonts.body(), ink));
                sheet.spawn((Text::new(place.clone()), fonts.title(1), ink));
                sheet.spawn((Text::default(), T::DrawingHint, fonts.body(), faded));
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
                button(sheet, &art, &fonts, T::LongRest, !game.world.party.is_empty(), PlayAction::Rest);
                button(sheet, &art, &fonts, T::Keys, true, PlayAction::Keys);
                button(sheet, &art, &fonts, T::ToMainMenu, true, PlayAction::MainMenu);
            }
            Sheet::Looking { name, description, actions } => {
                sheet.spawn((Text::new(name.clone()), fonts.title(1), ink));
                sheet.spawn((Text::new(description.clone()), fonts.body(), ink));
                for (n, label) in actions.iter().enumerate() {
                    text_button(sheet, &art, &fonts, label.clone(), PlayAction::Do(n));
                }
                sheet.spawn(Node { column_gap: px(6), ..default() }).with_children(|line| {
                    button(line, &art, &fonts, T::OwnAction, true, PlayAction::Own);
                    button(line, &art, &fonts, T::Leave, true, PlayAction::Close);
                });
            }
            Sheet::Writing => {
                sheet.spawn((Text::default(), T::WhatDoYouDo, fonts.body(), ink));
                sheet.spawn(Node { column_gap: px(4), align_items: AlignItems::Center, ..default() }).with_children(|line| {
                    line.spawn(Node { flex_grow: 1.0, height: px(BUTTON_HEIGHT), padding: UiRect::axes(px(5), px(0)), align_items: AlignItems::Center, overflow: Overflow::clip(), ..default() }).with_children(|field| {
                        field.spawn(backing(art.field.clone(), 4.0));
                        field.spawn((
                            SayInput,
                            AutoFocus,
                            AutoDirectionalNavigation::default(),
                            Node { width: percent(100), ..default() },
                            EditableText { allow_newlines: false, max_characters: Some(300), ..default() },
                            TextLayout::no_wrap(),
                            fonts.body(),
                            ink,
                            TextCursorStyle { color: color(INK), selection_color: color(BRASS_LIGHT), ..default() },
                        ));
                    });
                    button(line, &art, &fonts, T::Say, true, PlayAction::Send);
                });
                sheet.spawn((Text::default(), T::WritingHint, fonts.body(), faded));
            }
            Sheet::Asking(said) => {
                sheet.spawn((Text::new(format!("«{said}»")), fonts.body(), faded));
                sheet.spawn((Text::default(), T::TestRunning, fonts.body(), ink));
            }
            Sheet::Reading(turn) => {
                sheet.spawn((Text::new(turn.narration.clone()), fonts.body(), ink));
                for (n, option) in turn.options.iter().enumerate() {
                    text_button(sheet, &art, &fonts, option.label.clone(), PlayAction::Choose(n));
                }
                sheet.spawn(Node { column_gap: px(6), ..default() }).with_children(|line| {
                    if !turn.options.is_empty() {
                        button(line, &art, &fonts, T::OwnAction, true, PlayAction::Own);
                    }
                    button(line, &art, &fonts, T::Onward, true, PlayAction::Close);
                });
            }
            Sheet::Refused(why) => {
                sheet.spawn((Text::default(), T::RefusedTitle, fonts.title(1), ink));
                sheet.spawn((Text::new(why.clone()), fonts.body(), TextColor(color(WAX))));
                button(sheet, &art, &fonts, T::Recheck, true, PlayAction::AskAgain);
                button(sheet, &art, &fonts, T::Keys, true, PlayAction::Keys);
                button(sheet, &art, &fonts, T::Leave, true, PlayAction::Close);
            }
            Sheet::Journal { carried, lines } => {
                sheet.spawn((Text::default(), T::Journal, fonts.title(1), ink));
                let bag = if carried.is_empty() { T::CarryNothing.text(*lang).to_string() } else { format!("{} {carried}", T::Carried.text(*lang)) };
                sheet.spawn((Text::new(bag), fonts.body(), TextColor(color(WAX_DARK))));
                sheet.spawn(scroll_list(Node { flex_direction: FlexDirection::Column, row_gap: px(3), ..default() }, 150.0)).with_children(|rows| {
                    if lines.is_empty() {
                        rows.spawn((Text::default(), T::JournalEmpty, fonts.body(), faded));
                    }
                    for (speaker, text) in lines {
                        let (prefix, tone) = if *speaker == Speaker::Player { ("» ", WAX_DARK) } else { ("", INK) };
                        rows.spawn((Text::new(format!("{prefix}{text}")), fonts.body(), TextColor(color(tone))));
                    }
                });
                sheet.spawn(Node::default()).with_children(|slot| button(slot, &art, &fonts, T::Leave, true, PlayAction::Close));
            }
            Sheet::Rolling(what) => {
                sheet.spawn((Text::default(), T::CheckTitle, fonts.title(1), ink));
                sheet.spawn((Text::new(what.clone()), fonts.body(), ink));
                sheet.spawn(Node::default()).with_children(|slot| button(slot, &art, &fonts, T::RollDie, true, PlayAction::Roll));
            }
            Sheet::Character { lines, points } => {
                for line in lines {
                    sheet.spawn((Text::new(line.clone()), fonts.body(), ink));
                }
                if *points {
                    let ru = *lang == Lang::Ru;
                    sheet.spawn(Node { column_gap: px(3), row_gap: px(2), flex_wrap: FlexWrap::Wrap, ..default() }).with_children(|line| {
                        for ability in Ability::ALL {
                            text_button(line, &art, &fonts, format!("+1 {}", tr(ru, ability.name())), PlayAction::Improve(ability));
                        }
                    });
                }
                sheet.spawn(Node::default()).with_children(|slot| button(slot, &art, &fonts, T::Leave, true, PlayAction::Close));
            }
            Sheet::None => {}
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn send(said: String, near: Option<String>, game: &Game, gm: &Gm, store: &Store, settings: &Settings, master: &Master, session: &Session) -> Phase {
    let said = said.trim().to_string();
    if said.is_empty() {
        return Phase::Writing { near };
    }
    if session.is_client() {
        session.intend(Intent::Say { text: said.clone(), near: near.clone() });
        return Phase::Asking { said, near };
    }
    // At a table with guests the master is told who is acting.
    let said = match (session.is_host(), hero_of(&game.world, &player_name(settings))) {
        (true, Some(hero)) => format!("{hero}: {said}"),
        _ => said,
    };
    master.ask(gm, store, settings, Ask::Turn { world: Box::new(game.world.clone()), said: said.clone(), near: near.clone() });
    Phase::Asking { said, near }
}

/// Enter in the field sends what was typed.
#[allow(clippy::too_many_arguments)]
fn submit(
    keyboard: Res<ButtonInput<KeyCode>>,
    focus: Res<InputFocus>,
    input: Query<(Entity, &EditableText), With<SayInput>>,
    mut phase: ResMut<Phase>,
    game: Res<Game>,
    gm: Res<Gm>,
    store: Res<Store>,
    settings: Res<Settings>,
    master: Res<Master>,
    session: Res<Session>,
) {
    if !keyboard.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
        return;
    }
    let (Phase::Writing { near }, Ok((entity, input))) = (&*phase, input.single()) else { return };
    if focus.get() == Some(entity) {
        *phase = send(input.value().to_string(), near.clone(), &game, &gm, &store, &settings, &master, &session);
    }
}

#[allow(clippy::too_many_arguments)]
fn on_activate(
    used: On<Activate>,
    buttons: Query<&PlayAction>,
    input: Query<&EditableText, With<SayInput>>,
    mut phase: ResMut<Phase>,
    game: Option<ResMut<Game>>,
    mut toast: ResMut<Toast>,
    mut redraw: ResMut<Redraw>,
    mut arrival: ResMut<Arrival>,
    mut background: ResMut<Background>,
    mut return_to: ResMut<ReturnTo>,
    mut next: ResMut<NextState<Screen>>,
    store: Res<Store>,
    gm: Res<Gm>,
    settings: Res<Settings>,
    master: Res<Master>,
    mut session: ResMut<Session>,
    mut table: ResMut<Table>,
) {
    // This observer hears every button in the game; outside a game there is nothing to act on.
    let (Ok(action), Some(mut game)) = (buttons.get(used.entity), game) else { return };
    let outcome_of = |text: String| Turn { narration: text, options: Vec::new() };
    // A guest does not change the world: it asks the host, and reads the answer with
    // everyone else when it comes.
    if session.is_client() {
        let asked = match (*action, phase.clone()) {
            (PlayAction::Do(index), Phase::Looking { thing }) => Some(Intent::Do { thing, index }),
            (PlayAction::Choose(index), Phase::Reading { .. }) => Some(Intent::Choose(index)),
            (PlayAction::Roll, Phase::Rolling) => Some(Intent::Roll),
            (PlayAction::Improve(ability), _) => Some(Intent::Improve(ability)),
            (PlayAction::MainMenu, _) => {
                // Leaving the table.
                *session = Session::Solo;
                *phase = Phase::Walking;
                next.set(Screen::Main);
                return;
            }
            (PlayAction::Rest | PlayAction::Retry | PlayAction::Sketch | PlayAction::Stay, _) => return,
            _ => None,
        };
        if let Some(intent) = asked {
            session.intend(intent);
            if !matches!(*action, PlayAction::Improve(_)) {
                *phase = Phase::Walking;
            }
            return;
        }
    }
    match (*action, phase.clone()) {
        (PlayAction::Resume, _) => *phase = Phase::Walking,
        (PlayAction::Close, _) => *phase = settled(&game.world),
        (PlayAction::Roll, Phase::Rolling) => {
            if let Some(done) = game.world.roll_check(&mut Rng::from_clock()) {
                let verdict = if done.success { T::CheckSuccess } else { T::CheckFailure };
                let line = format!("{}: d20 {}{:+} = {} / {} — {}\n{}", done.hero, done.roll.die, done.roll.bonus, done.roll.total(), done.difficulty, verdict.text(settings.lang), done.narration);
                *phase = told(outcome_of(line), Vec::new(), &mut game, &store, &mut redraw, &mut background, &session, &mut table);
            }
        }
        (PlayAction::Improve(ability), _) => {
            if let Some(hero) = game.world.party.iter_mut().find(|h| h.points_to_place > 0) {
                hero.improve(ability);
            }
            autosave(&store, &game);
        }
        (PlayAction::Rest, _) => {
            game.world.long_rest();
            *toast = Toast { text: Some(T::Rested), left: 3.0 };
            autosave(&store, &game);
            *phase = Phase::Walking;
        }
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
            master.ask(&gm, &store, &settings, Ask::Location { world: Box::new(game.world.clone()), id: to.clone(), from: game.world.current.clone(), ahead: false });
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
        (PlayAction::Do(index), Phase::Looking { thing }) => {
            if let Some(outcome) = game.world.perform(&thing, index) {
                *phase = told(outcome_of(outcome), Vec::new(), &mut game, &store, &mut redraw, &mut background, &session, &mut table);
            }
        }
        (PlayAction::Own, Phase::Looking { thing }) => *phase = Phase::Writing { near: Some(thing) },
        (PlayAction::Own, _) => *phase = Phase::Writing { near: None },
        (PlayAction::Send, Phase::Writing { near }) => {
            if let Ok(input) = input.single() {
                *phase = send(input.value().to_string(), near, &game, &gm, &store, &settings, &master, &session);
            }
        }
        (PlayAction::Choose(index), Phase::Reading { turn }) => {
            if let Some(option) = turn.options.get(index) {
                let outcome = game.world.choose(option);
                *phase = told(outcome_of(outcome), Vec::new(), &mut game, &store, &mut redraw, &mut background, &session, &mut table);
            }
        }
        (PlayAction::AskAgain, Phase::Refused { said, near, .. }) => *phase = send(said, near, &game, &gm, &store, &settings, &master, &session),
        _ => {}
    }
}

/// The name of the hero a player plays, if they have one in this world.
fn hero_of<'a>(world: &'a World, player: &str) -> Option<&'a str> {
    let index = world.owners.iter().position(|o| o == player)?;
    world.party.get(index).map(|h| h.name.as_str())
}

/// Carries out what guests asked for, the same way the host's own presses are carried out.
#[allow(clippy::too_many_arguments)]
fn guests(
    mut phase: ResMut<Phase>,
    mut game: ResMut<Game>,
    mut table: ResMut<Table>,
    mut redraw: ResMut<Redraw>,
    mut background: ResMut<Background>,
    mut view: ResMut<crate::fight::FightView>,
    session: Res<Session>,
    store: Res<Store>,
    gm: Res<Gm>,
    settings: Res<Settings>,
    master: Res<Master>,
) {
    let outcome_of = |text: String| Turn { narration: text, options: Vec::new() };
    for (player, intent) in std::mem::take(&mut table.inbox) {
        let shown = match intent {
            Intent::Do { thing, index } => game.world.perform(&thing, index).map(outcome_of),
            Intent::Choose(index) => table.options.get(index).cloned().map(|option| outcome_of(game.world.choose(&option))),
            Intent::Roll => game.world.roll_check(&mut Rng::from_clock()).map(|done| {
                let verdict = if done.success { T::CheckSuccess } else { T::CheckFailure };
                outcome_of(format!("{}: d20 {}{:+} = {} / {} — {}\n{}", done.hero, done.roll.die, done.roll.bonus, done.roll.total(), done.difficulty, verdict.text(settings.lang), done.narration))
            }),
            Intent::Say { text, near } => {
                // One question to the master at a time; a second one is told to wait.
                if table.busy.is_some() || matches!(*phase, Phase::Asking { .. }) {
                    session.broadcast(&Down::Trouble(T::MasterBusy.text(settings.lang).to_string()));
                } else {
                    let said = match hero_of(&game.world, &player) {
                        Some(hero) => format!("{hero}: {}", text.trim()),
                        None => format!("{player}: {}", text.trim()),
                    };
                    master.ask(&gm, &store, &settings, Ask::Turn { world: Box::new(game.world.clone()), said: said.clone(), near });
                    session.broadcast(&Down::Thinking(said.clone()));
                    table.busy = Some(said);
                }
                None
            }
            Intent::Exit { to } => {
                // The party travels together: one of them at a way out takes everyone.
                if *phase == Phase::Walking && game.world.here().exits.iter().any(|e| e.to == to) {
                    *phase = Phase::Leaving { to, t: 0.0 };
                }
                None
            }
            Intent::Fight(act) => {
                // Only the player whose hero is acting may act for it.
                if game.world.acting_owner() == Some(player.as_str()) {
                    let events = game.world.fight_act(&act);
                    if !events.is_empty() {
                        session.broadcast(&Down::FightEvents(events.clone()));
                        view.extend(events);
                        redraw.0 = true;
                    }
                }
                None
            }
            Intent::Improve(ability) => {
                if let Some(index) = game.world.owners.iter().position(|o| *o == player)
                    && let Some(hero) = game.world.party.get_mut(index)
                {
                    hero.improve(ability);
                }
                None
            }
        };
        if let Some(turn) = shown {
            told(turn.clone(), Vec::new(), &mut game, &store, &mut redraw, &mut background, &session, &mut table);
            show(turn, &mut phase, &mut table);
        }
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
            .init_resource::<Background>()
            .add_systems(OnEnter(Screen::Play), enter)
            .add_systems(OnExit(Screen::Play), leave)
            .add_systems(
                Update,
                (submit, keys, walk, advance, receive, guests, background_work, draw, place_hero, place_fellows, idle, hud, veil, overlay).chain().run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>),
            )
            .add_observer(on_activate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let draft: world::WorldDraft = serde_json::from_str(&fake::world("play")).unwrap();
        world::build_world(&draft, "play", "English").unwrap().0
    }

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

    #[test]
    fn looking_at_a_thing_shows_its_name_and_what_can_be_done_with_it() {
        let w = world();
        let sheet = sheet_for(&Phase::Looking { thing: "cat".into() }, &w, 0, Lang::En);
        let Sheet::Looking { name, actions, .. } = sheet else { panic!("expected the looking sheet") };
        assert_eq!(name, "Grey cat");
        assert_eq!(actions, ["Pet the cat", "Shoo it away"]);
    }

    #[test]
    fn looking_at_something_that_is_gone_shows_nothing() {
        assert!(sheet_for(&Phase::Looking { thing: "ghost".into() }, &world(), 0, Lang::En) == Sheet::None);
    }

    #[test]
    fn the_journal_lists_what_is_carried_and_what_was_said() {
        let mut w = world();
        w.perform("cat", 1);
        let Sheet::Journal { carried, lines } = sheet_for(&Phase::Journal, &w, 0, Lang::En) else { panic!("expected the journal") };
        assert_eq!(carried, "Cat hair");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], (Speaker::Player, "Shoo it away".to_string()));
    }

    #[test]
    fn the_master_speaks_unprompted_only_when_the_host_allows_it() {
        assert_eq!(event_after(Initiative::Off), None);
        assert!(event_after(Initiative::Often) < event_after(Initiative::Rare));
    }
}
