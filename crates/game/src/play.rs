//! Playing: the current location on screen, the party walking about it, the passage from one
//! place to the next, and the conversation with the game master.

use crate::art::{Art, ground_picture, sprite_picture};
use crate::coop::{Down, DraftView, GmAct, Held, Intent, Session, Table, player_name};
use crate::gm::{Gm, Trouble, trouble_text};
use crate::i18n::{Lang, T};
use crate::master::{Answer, Ask, Master};
use crate::menu::{Backdrop, ReturnTo, Screen, spawn_panel};
use crate::screen::CANVAS;
use crate::shot::Warp;
use crate::store::{Initiative, Settings, Store};
use crate::theme::*;
use crate::translate::{Translator, foreign};
use crate::widgets::{Activate, BUTTON_HEIGHT, EnterPresses, backing, button, scroll_list, text_button};
use bevy::input_focus::{AutoFocus, InputFocus};
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::text::{EditableText, TextCursorStyle};
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigation;
use world::base::MARKER_SPRITE;
use world::walk::FEET;
use crate::hero::sheet_lines;
use rules::dice::Edge;
use rules::{Ability, Rng, Roll, Side, tr};
use world::{CreatureCard, LocationDraft, Speaker, TILE, Turn, TurnDraft, Whisper, World, fake};

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
    /// The player in the master's seat reads the model's draft before the others do.
    Judging,
    /// The master's panel: directions, tools, the library.
    Master,
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
    /// The master is writing abilities for a hero.
    feats_asked: bool,
    /// Seconds before the master is asked for abilities again; set after a refusal.
    feats_rest: f32,
    /// Heroes whose picture has been asked for in this sitting, by name: each is asked once.
    portraits_asked: Vec<String>,
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

/// The telling of a draft, for the master to change.
#[derive(Component)]
struct JudgeInput;

/// How the model should redo a draft.
#[derive(Component)]
struct RedoInput;

/// The parameter of the master's tools.
#[derive(Component)]
struct MasterInput;

/// What the master's panel can do. The panel's text field is each tool's parameter.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Tool {
    ToModel,
    Narrate,
    Whisper,
    NextTarget,
    Creature,
    Mood,
    Remove,
    Tile,
    Damage,
    Heal,
    Give,
    Fight,
    Roll,
    Invent,
    /// A creature of the library, by its place in the list.
    Card(usize),
}

/// The master's panel as its owner left it.
#[derive(Resource, Default)]
struct Panel {
    typed: String,
    /// Which of the other players a whisper goes to.
    target: usize,
    /// Whether creatures brought in are hostile.
    hostile: bool,
    /// Raised by every press, so the panel is drawn again and its field emptied.
    presses: u32,
}

/// How far from the master's token "nearest" reaches when removing a thing.
const REMOVE_REACH: f32 = 48.0;

#[derive(bevy::ecs::system::SystemParam)]
struct MasterFields<'w, 's> {
    judge: Query<'w, 's, &'static EditableText, With<JudgeInput>>,
    redo: Query<'w, 's, &'static EditableText, With<RedoInput>>,
    panel: Query<'w, 's, &'static EditableText, With<MasterInput>>,
}

/// The last things said at the table, in a corner of the screen.
#[derive(Component)]
struct ChatLog;

/// How long a line of table talk stays on screen, and how many are shown at once.
const CHAT_SECONDS: f32 = 14.0;
const CHAT_LINES: usize = 4;

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
    /// Say what was typed to the other players, not to the master.
    Talk,
    /// One of the choices offered with the last telling.
    Choose(usize),
    /// Back to walking.
    Close,
    AskAgain,
    /// Roll the check that is waiting.
    Roll,
    /// Put an earned point into an ability of the first hero who has points to place.
    Improve(Ability),
    /// Gather the logs and the save into a folder to send.
    Report,
    /// A night's rest.
    Rest,
    /// An hour's rest: hit dice for hit points.
    ShortRest,
    /// Make a new hero in place of one who died.
    NewHero,
    /// One of the abilities the master offered.
    Feat(usize),
    SeatTake,
    SeatLeave,
    /// The master's verdict on the draft: let it through or throw it away.
    Judge(bool),
    /// Have the model redo the draft as the direction says.
    Redo,
    Tool(Tool),
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
    commands.spawn((
        DespawnOnExit(Screen::Play),
        ChatLog,
        Node { position_type: PositionType::Absolute, left: px(4), top: px(4), max_width: px(230), padding: UiRect::axes(px(3), px(1)), display: Display::None, ..default() },
        BackgroundColor(Color::srgba(0.1, 0.07, 0.06, 0.82)),
        GlobalZIndex(9),
        Text::default(),
        fonts.body(),
        TextColor(color(PARCHMENT)),
    ));
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

/// The hero this player plays: their own at a shared table; alone, the first still alive.
pub fn my_hero(world: &World, me: &str, solo: bool) -> Option<usize> {
    let alone = || (0..world.party.len()).find(|i| !world.party[*i].dead).or((!world.party.is_empty()).then_some(0));
    world.owners.iter().position(|o| !o.is_empty() && o == me).or_else(|| solo.then(alone).flatten())
}

/// Whether this player's hero is dead and a new one is called for.
fn hero_lost(world: &World, me: &str, solo: bool) -> bool {
    if solo {
        return !world.party.is_empty() && world.party.iter().all(|h| h.dead);
    }
    world.owners.iter().position(|o| o == me).is_some_and(|i| world.party.get(i).is_some_and(|h| h.dead))
}

/// The abilities waiting for this player to choose from, if any.
fn my_offer<'a>(world: &'a World, me: &str, solo: bool) -> Option<&'a world::FeatOffer> {
    world.offers.iter().find(|offer| solo || world.owners.get(offer.hero).is_some_and(|o| o == me || o.is_empty()))
}

fn draw(mut commands: Commands, mut redraw: ResMut<Redraw>, game: Res<Game>, mut images: ResMut<Assets<Image>>, old: Query<Entity, With<InLocation>>, session: Res<Session>, settings: Res<Settings>) {
    if !std::mem::take(&mut redraw.0) {
        return;
    }
    let me = player_name(&settings);
    let solo = matches!(*session, Session::Solo);
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
        let fighter = world.fight.as_ref().filter(|_| world.fight_here()).and_then(|f| f.combatants.iter().find(|c| c.id == creature.id));
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
    match world.fight.as_ref().filter(|_| world.fight_here()) {
        // In a fight every hero stands in a cell of its own; the fallen are darkened.
        Some(fight) => {
            for c in fight.combatants.iter().filter(|c| c.side == Side::Party && !c.dead) {
                let (x, y) = foot(c.x.max(0) as usize, c.y.max(0) as usize);
                let hero = c.id.strip_prefix("hero_").and_then(|n| n.parse::<usize>().ok());
                if let Some(image) = picture(&world.hero_sprite(hero)) {
                    let tint = if c.hp > 0 { Color::WHITE } else { Color::srgb(0.35, 0.3, 0.3) };
                    commands.spawn((InLocation, Sprite { color: tint, flip_y: c.hp <= 0, ..Sprite::from_image(image) }, Anchor::BOTTOM_CENTER, Transform::from_translation(on_screen(x, y).extend(depth(y)))));
                }
            }
        }
        None => {
            for other in world.tokens_here() {
                if let Some(image) = picture(&world.hero_sprite(world.owners.iter().position(|o| *o == other.name))) {
                    commands.spawn((InLocation, Fellow(other.name.clone()), Sprite { color: Color::srgb(0.8, 0.85, 1.0), ..Sprite::from_image(image) }, Anchor::BOTTOM_CENTER, Transform::default()));
                }
            }
            if let Some(image) = picture(&world.hero_sprite(my_hero(world, &me, solo))) {
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
    let others: Vec<&world::Token> = game.world.tokens_here().collect();
    // A fight beginning or ending changes who is drawn where.
    let fighting = game.world.fight_here();
    if std::mem::replace(&mut *was_fighting, fighting) != fighting {
        redraw.0 = true;
    }
    // So does somebody walking in or out of this place.
    if !fighting && fellows.iter().count() != others.len() {
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
    // A fight or a check can begin mid-walk: a master declares one by hand, and a guest
    // learns of them from the world the host sends.
    let next = settled(&game.world);
    if next != Phase::Walking {
        *phase = next;
        return;
    }
    if session.is_client() {
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
    // Orders from a test script.
    for order in std::mem::take(&mut warp.1) {
        let (what, amount) = order.split_once('=').map_or((order.as_str(), 0), |(what, n)| (what, n.trim().parse::<i32>().unwrap_or(0)));
        let world = &mut game.world;
        match what {
            "xp" => {
                for hero in &mut world.party {
                    let ups = hero.gain_xp(amount.max(0) as u32);
                    world.level_ups.extend(ups.into_iter().map(|up| (hero.name.clone(), up)));
                }
            }
            "hurt" => world.party.iter_mut().for_each(|hero| hero.take_damage(amount)),
            "kill" => world.party.iter_mut().for_each(|hero| {
                hero.hp = 0;
                hero.dead = true;
            }),
            "deed" => world.secrets_due.push(0),
            _ => {}
        }
    }
    if !session.is_client()
        && let Some(exit) = game.world.exit_here()
    {
        *phase = Phase::Leaving { to: exit.to.clone(), t: 0.0 };
        return;
    }
    // Something hostile close by does not wait to be spoken to. With guests at the table
    // any of their tokens can be the one that is noticed.
    let noticed = game.world.fight.is_none() && (game.world.hostile_near(NOTICE_CELLS) || game.world.tokens_here().any(|t| hostile_near_token(&game.world, t.x, t.y)));
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
#[allow(clippy::too_many_arguments)]
fn keys(keyboard: Res<ButtonInput<KeyCode>>, gamepads: Query<&Gamepad>, game: Res<Game>, mut phase: ResMut<Phase>, mut was_walking: Local<bool>, mut table: ResMut<Table>, settings: Res<Settings>, session: Res<Session>, store: Res<Store>) {
    let pad = |button: GamepadButton| gamepads.iter().any(|pad| pad.just_pressed(button));
    // The draft went away while its sheet was open (judged, or the seat changed hands).
    if *phase == Phase::Judging && table.judging.is_none() {
        *phase = Phase::Walking;
    }
    // The press that closes a sheet must not also act on the world behind it: a key only
    // opens something if the party was already walking on the frame before.
    let was_settled = std::mem::replace(&mut *was_walking, *phase == Phase::Walking);
    if keyboard.just_pressed(KeyCode::Escape) || pad(GamepadButton::Start) {
        let next = match &*phase {
            Phase::Walking => Phase::Paused,
            Phase::Paused | Phase::Looking { .. } | Phase::Writing { .. } | Phase::Journal | Phase::Sheet | Phase::Judging | Phase::Master => Phase::Walking,
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
    } else if (keyboard.just_pressed(KeyCode::KeyG) || pad(GamepadButton::Select)) && seat_state(&game.world, &session, &player_name(&settings)) == SeatState::Mine {
        // A waiting draft comes first; otherwise the panel.
        if table.judging.is_some() {
            *phase = Phase::Judging;
        } else {
            table.cards = store.load_cards();
            *phase = Phase::Master;
        }
    }
}

fn hostile_near_token(world: &World, x: f32, y: f32) -> bool {
    let cell = ((x / TILE as f32).floor() as i32, (y / TILE as f32).floor() as i32);
    world.hostiles().any(|c| rules::combat::distance((c.x as i32, c.y as i32), cell) <= NOTICE_CELLS)
}

/// Puts a telling in front of this player: at once when they are free to read, otherwise
/// after what they are reading now.
pub fn show(turn: Turn, phase: &mut Phase, table: &mut Table) {
    // What happens in a place the host is not in is not for the host's screen.
    if table.elsewhere {
        return;
    }
    if matches!(*phase, Phase::Walking | Phase::Asking { .. } | Phase::Looking { .. } | Phase::Rolling) {
        *phase = Phase::Reading { turn };
    } else {
        table.unread.push_back(turn);
    }
}

/// What the screen should be doing once a sheet is closed: a fight that broke out, a check
/// that is waiting, or simply walking on.
fn settled(world: &World) -> Phase {
    if world.fight_here() {
        Phase::Fighting
    } else if world.pending_here() {
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
    // Everyone in the place reads what the master tells, and may take up the choices it
    // offers. A party that has split up is told scene by scene.
    let place = game.world.current.clone();
    for player in session.players() {
        if game.world.place_of(&player) == place {
            session.send_to(&player, &Down::Told(turn.clone()));
        }
    }
    table.options.insert(place, turn.options.clone());
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
    mut view: ResMut<crate::fight::FightView>,
    mut toast: ResMut<Toast>,
    mut translator: ResMut<Translator>,
) {
    for answer in master.take() {
        match answer {
            Answer::Translated { texts, translated } => match translated {
                Ok(translated) => {
                    translator.learn(&texts, translated);
                    if let Err(e) = store.save_translations(settings.lang, translator.cache()) {
                        error!("could not keep the translations: {e}");
                    }
                }
                Err(trouble) => {
                    info!("nothing was translated: {trouble:?}");
                    translator.refused();
                }
            },
            Answer::Feats { hero, secret, feats } => {
                background.feats_asked = false;
                match feats {
                    Ok(feats) => {
                        let name = game.world.party.get(hero).map(|h| h.name.clone()).unwrap_or_default();
                        match game.world.take_feats(hero, secret, feats) {
                            // A secret skill is told to the whole table as it is earned.
                            Some(skill) => {
                                let line = game.world.party.get(hero).and_then(|h| h.feats.last()).map(|f| crate::hero::feat_line(f, settings.lang)).unwrap_or(skill);
                                let turn = Turn { narration: format!("★ {name}: {} — {line}", T::SecretSkill.text(settings.lang)), options: Vec::new() };
                                let mut desk = Desk { game: &mut game, table: &mut table, phase: &mut phase, redraw: &mut redraw, background: &mut background, session: &session, settings: &settings, store: &store, gm: &gm, master: &master };
                                let draft = TurnDraft { narration: turn.narration, ..TurnDraft::default() };
                                finish(&mut desk, "", &draft);
                            }
                            None => {
                                if my_offer(&game.world, &player_name(&settings), matches!(*session, Session::Solo)).is_some() {
                                    *toast = Toast { text: Some(T::FeatOffer), left: 8.0 };
                                }
                                autosave(&store, &game);
                            }
                        }
                    }
                    Err(trouble) => {
                        info!("no abilities were written: {trouble:?}");
                        background.feats_rest = BACKGROUND_REST;
                    }
                }
            }
            Answer::Stunt { spell, draft } => {
                let lang = settings.lang;
                let line = match draft {
                    Ok(draft) => {
                        let name = spell.as_ref().and_then(|(index, _)| rules::srd().spells.get(index)).map_or_else(|| T::StuntName.text(Lang::En).to_string(), |s| s.name.clone());
                        let caster = game.world.fight.as_ref().and_then(|f| f.actor().id.strip_prefix("hero_").and_then(|n| n.parse::<usize>().ok()));
                        let Some(stunt) = game.world.fight_stunt(&draft, &name, &mut Rng::from_clock()) else {
                            view.note(String::new());
                            continue;
                        };
                        // The slot goes whether or not the spell did what was hoped.
                        if let (Some((_, slot)), Some(hero)) = (&spell, caster.and_then(|i| game.world.party.get_mut(i))) {
                            hero.spend_slot(*slot);
                        }
                        session.broadcast(&Down::FightEvents(stunt.events.clone()));
                        view.extend(stunt.events);
                        let verdict = if stunt.success { T::CheckSuccess } else { T::CheckFailure };
                        format!("{}: d20 {}{:+} = {} / {} — {}. {}", stunt.hero, stunt.roll.die, stunt.roll.bonus, stunt.roll.total(), stunt.difficulty, verdict.text(lang), stunt.narration)
                    }
                    Err(trouble) => trouble_text(&trouble, gm.dispatcher.now(), lang),
                };
                session.broadcast(&Down::FightNote(line.clone()));
                view.note(line);
                redraw.0 = true;
            }
            Answer::Portrait { hero, rows } => {
                if let Ok(rows) = rows
                    && game.world.set_portrait(hero, &rows)
                {
                    redraw.0 = true;
                    autosave(&store, &game);
                }
            }
            Answer::Location { id, from, ahead, draft } => {
                let waited_for = *phase == (Phase::Drawing { to: id.clone() });
                if waited_for && ahead && draft.is_err() {
                    // Drawn ahead with the best models only, and they refused. Now that the
                    // party is at the door the request is made in earnest.
                    master.ask(&gm, &store, &settings, Ask::Location { world: Box::new(game.world.clone()), id: id.clone(), from, ahead: false });
                } else if waited_for {
                    *phase = settle(draft, &id, &from, &mut game, &store, &mut redraw, &mut arrival);
                } else if game.world.stubs.contains_key(&id) {
                    let wanted = table.travellers.iter().any(|(_, to)| *to == id);
                    match draft.and_then(|d| game.world.add_location(&d, &id, Some(&from)).map_err(Trouble::Unreadable)) {
                        Ok(_) => autosave(&store, &game),
                        // Drawn ahead with the best models only, and a guest is at the door:
                        // now it is asked for in earnest.
                        Err(_) if wanted && ahead => {
                            let mut seen = game.world.clone();
                            if seen.locations.contains_key(&from) {
                                seen.current = from.clone();
                            }
                            master.ask(&gm, &store, &settings, Ask::Location { world: Box::new(seen), id: id.clone(), from, ahead: false });
                        }
                        Err(trouble) => {
                            info!("place '{id}' was not drawn: {trouble:?}");
                            background.ahead_rest = BACKGROUND_REST;
                            // Guests who were waiting for it stay where they are.
                            let why = trouble_text(&trouble, gm.dispatcher.now(), settings.lang);
                            for (player, _) in table.travellers.iter().filter(|(_, to)| *to == id) {
                                session.send_to(player, &Down::Trouble(why.clone()));
                            }
                            table.travellers.retain(|(_, to)| *to != id);
                        }
                    }
                }
                // Guests who were waiting for the place walk in.
                if game.world.locations.contains_key(&id) {
                    for (player, _) in table.travellers.iter().filter(|(_, to)| *to == id) {
                        game.world.places.insert(player.clone(), id.clone());
                    }
                    table.travellers.retain(|(_, to)| *to != id);
                }
            }
            Answer::Turn { said, near, draft } => {
                let mine = matches!(&*phase, Phase::Asking { said: asked, .. } if *asked == said);
                let for_a_guest = table.busy.as_deref() == Some(said.as_str());
                if !mine && !for_a_guest {
                    continue;
                }
                table.busy = None;
                // What a guest did happens where that guest is.
                let scene = table.busy_for.take().filter(|_| for_a_guest && !mine);
                let mut desk = Desk { game: &mut game, table: &mut table, phase: &mut phase, redraw: &mut redraw, background: &mut background, session: &session, settings: &settings, store: &store, gm: &gm, master: &master };
                match draft {
                    // A human master reads the model's answer before the players do.
                    Ok(draft) if seated(&desk.game.world).is_some() => {
                        desk.table.drafts.push_back(Held { said, near, draft: *draft, scene });
                        if desk.table.drafts.len() == 1 {
                            offer(&mut desk);
                        }
                    }
                    Ok(draft) => {
                        let home = scene.as_deref().map(|player| enter_scene(desk.game, desk.table, player));
                        finish(&mut desk, &said, &draft);
                        leave_scene(desk.game, desk.table, home);
                    }
                    Err(trouble) if mine => *desk.phase = Phase::Refused { said, near, trouble },
                    Err(trouble) => {
                        let why = trouble_text(&trouble, gm.dispatcher.now(), settings.lang);
                        // A master's own direction that failed is the master's to hear about.
                        desk.table.master_note = why.clone();
                        if let Some(master) = seated(&desk.game.world) {
                            session.send_to(master, &Down::Card(Err(why.clone())));
                        }
                        session.broadcast(&Down::Trouble(why));
                    }
                }
            }
            Answer::Event(draft) => {
                background.event_asked = false;
                background.quiet = 0.0;
                // An event interrupts a walk, never a conversation or a menu.
                if let (Ok(draft), Phase::Walking) = (draft, &*phase)
                    && !draft.narration.trim().is_empty()
                    && seated(&game.world).is_none()
                {
                    let mut desk = Desk { game: &mut game, table: &mut table, phase: &mut phase, redraw: &mut redraw, background: &mut background, session: &session, settings: &settings, store: &store, gm: &gm, master: &master };
                    finish(&mut desk, "", &draft);
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
            Answer::Card { for_player, card } => {
                let card = card.map(|card| *card).map_err(|trouble| trouble_text(&trouble, gm.dispatcher.now(), settings.lang));
                if for_player == player_name(&settings) {
                    table.keep_card(&store, card, settings.lang);
                } else {
                    session.send_to(&for_player, &Down::Card(card));
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
fn background_work(time: Res<Time>, phase: Res<Phase>, mut game: ResMut<Game>, gm: Res<Gm>, store: Res<Store>, settings: Res<Settings>, master: Res<Master>, mut background: ResMut<Background>, session: Res<Session>) {
    if session.is_client() {
        return;
    }
    let dt = time.delta_secs();
    background.ahead_rest = (background.ahead_rest - dt).max(0.0);
    background.chapter_rest = (background.chapter_rest - dt).max(0.0);
    background.feats_rest = (background.feats_rest - dt).max(0.0);
    if *phase != Phase::Walking {
        return;
    }
    // A table that plays by the book lets the levels pass without abilities of its own.
    if game.world.table.book_only && (game.world.party.iter().any(|h| h.feat_due) || !game.world.secrets_due.is_empty()) {
        game.world.settle_feats();
    }
    if !background.feats_asked
        && background.feats_rest == 0.0
        && let Some((hero, secret)) = game.world.feat_wanted()
    {
        background.feats_asked = true;
        master.ask(&gm, &store, &settings, Ask::Feats { world: Box::new(game.world.clone()), hero, secret });
    }
    // Each hero is drawn once by the master, when the table lets the master draw unasked.
    if settings.predraw {
        let undrawn = (0..game.world.party.len()).find(|i| {
            let name = &game.world.party[*i].name;
            !game.world.sprites.contains_key(&world::portrait_id(name)) && !background.portraits_asked.contains(name)
        });
        if let Some(hero) = undrawn {
            background.portraits_asked.push(game.world.party[hero].name.clone());
            master.ask(&gm, &store, &settings, Ask::Portrait { world: Box::new(game.world.clone()), hero });
        }
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
        && seated(world).is_none()
    {
        background.event_asked = true;
        master.ask(&gm, &store, &settings, Ask::Event { world: Box::new(world.clone()) });
    }
}

fn veil(phase: Res<Phase>, mut veils: Query<&mut BackgroundColor, With<Veil>>) {
    let darkness = match &*phase {
        Phase::Walking | Phase::Looking { .. } | Phase::Writing { .. } | Phase::Reading { .. } | Phase::Asking { .. } | Phase::Rolling | Phase::Fighting | Phase::Judging | Phase::Master => 0.0,
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
    session: Res<Session>,
    settings: Res<Settings>,
    mut translator: ResMut<Translator>,
) {
    let world = &game.world;
    let here = world.here();
    // A player who reads another language than the world is told in reads translations.
    let other_tongue = foreign(world, *lang);
    let translator = translator.bypass_change_detection();
    let mut read = |text: &str| translator.read(other_tongue, text);
    let place = read(&here.name);
    for mut text in &mut names {
        if text.0 != place {
            text.0 = place.clone();
        }
    }
    toast.left = (toast.left - time.delta_secs()).max(0.0);
    let thing_name = world.thing_near(THING_REACH).and_then(|id| here.props.iter().map(|p| (&p.id, &p.name)).chain(here.creatures.iter().map(|c| (&c.id, &c.name))).find(|(i, _)| i.as_str() == id).map(|(_, name)| name.clone()));
    let hint = match (toast.text.filter(|_| toast.left > 0.0), thing_name, world.exit_near(EXIT_HINT_REACH)) {
        (Some(said), _, _) => said.text(*lang).to_string(),
        (None, Some(name), _) => format!("E: {}", read(&name)),
        (None, None, Some(exit)) if exit.label.is_empty() => format!("→ {}", read(world.place_name(&exit.to))),
        (None, None, Some(exit)) => format!("{} → {}", read(&exit.label), read(world.place_name(&exit.to))),
        (None, None, None) if my_offer(world, &player_name(&settings), matches!(*session, Session::Solo)).is_some() => T::FeatOffer.text(*lang).to_string(),
        (None, None, None) if seat_state(world, &session, &player_name(&settings)) == SeatState::Mine => format!("{}{}", T::HudHint.text(*lang), T::HudMaster.text(*lang)),
        (None, None, None) => T::HudHint.text(*lang).to_string(),
    };
    for mut text in &mut hints {
        if text.0 != hint {
            text.0 = hint.clone();
        }
    }
    arrival.age += time.delta_secs();
    let description = read(&here.description);
    let letters = description.chars().count();
    let shown: String = description.chars().take((arrival.age * LETTERS_PER_SECOND) as usize).collect();
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

/// Table talk fades from the corner of the screen a while after it was said.
fn chat_log(time: Res<Time>, mut table: ResMut<Table>, mut logs: Query<(&mut Text, &mut Node), With<ChatLog>>) {
    if table.chat.is_empty() {
        return;
    }
    let table = table.bypass_change_detection();
    for line in &mut table.chat {
        line.age += time.delta_secs();
    }
    let fresh: Vec<String> = table.chat.iter().filter(|l| l.age < CHAT_SECONDS).map(|l| format!("{}: {}", l.from, l.text)).collect();
    let shown = fresh[fresh.len().saturating_sub(CHAT_LINES)..].join("\n");
    for (mut text, mut node) in &mut logs {
        let display = if shown.is_empty() { Display::None } else { Display::Flex };
        if node.display != display {
            node.display = display;
        }
        if text.0 != shown {
            text.0 = shown.clone();
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
    Paused { seat: SeatState, lost: bool },
    Judging(DraftView),
    Master(PanelView),
    Looking { name: String, description: String, actions: Vec<String> },
    /// Whether there are other players to talk to.
    Writing { table: bool },
    Asking(String),
    Reading(Turn),
    Refused(String),
    Journal { carried: String, lines: Vec<(Speaker, String)> },
    /// What is being rolled, in words.
    Rolling(String),
    /// The sheets of the party, whether there are points to place, and the abilities the
    /// master offers this player to choose from.
    Character { lines: Vec<String>, points: bool, offer: Vec<String> },
}

/// Where this player stands with the master's seat.
#[derive(Clone, Copy, PartialEq, Default, Debug)]
enum SeatState {
    /// Playing alone: there is no seat to take.
    #[default]
    Alone,
    Free,
    Mine,
    Taken,
}

fn seat_state(world: &World, session: &Session, me: &str) -> SeatState {
    if matches!(session, Session::Solo) {
        return SeatState::Alone;
    }
    match seated(world) {
        Some(master) if master == me => SeatState::Mine,
        Some(_) => SeatState::Taken,
        None => SeatState::Free,
    }
}

/// The master's panel as it is drawn.
#[derive(Clone, PartialEq, Default)]
struct PanelView {
    typed: String,
    /// Who a whisper would go to.
    target: String,
    hostile: bool,
    note: String,
    cards: Vec<String>,
    presses: u32,
}

/// What the sheets show that is not in the world.
#[derive(Default)]
struct Extra {
    seat: SeatState,
    judging: Option<DraftView>,
    panel: PanelView,
    /// This player's hero is dead.
    lost: bool,
    /// The abilities offered to this player, each in a line.
    offer: Vec<String>,
}

fn others_at_table(world: &World, me: &str) -> Vec<String> {
    world.present.iter().filter(|p| p.as_str() != me).cloned().collect()
}

/// What the master wrote on a sheet, in the player's language where a translation is known.
/// What is not known yet is shown as written and asked for.
fn translated(sheet: Sheet, other_tongue: bool, translator: &mut Translator) -> Sheet {
    if !other_tongue {
        return sheet;
    }
    let mut read = |text: &str| translator.read(true, text);
    match sheet {
        Sheet::Looking { name, description, actions } => Sheet::Looking { name: read(&name), description: read(&description), actions: actions.iter().map(|a| read(a)).collect() },
        Sheet::Reading(turn) => Sheet::Reading(Turn {
            narration: read(&turn.narration),
            options: turn
                .options
                .into_iter()
                .map(|mut option| {
                    option.label = read(&option.label);
                    option
                })
                .collect(),
        }),
        Sheet::Journal { carried, lines } => Sheet::Journal { carried, lines: lines.into_iter().map(|(speaker, text)| (speaker, read(&text))).collect() },
        other => other,
    }
}

fn sheet_for(phase: &Phase, world: &World, now: u64, lang: Lang, extra: &Extra) -> Sheet {
    match phase {
        Phase::Drawing { to } => Sheet::Drawing(world.place_name(to).to_string()),
        Phase::Stuck { trouble, .. } => Sheet::Stuck(trouble_text(trouble, now, lang)),
        Phase::Paused => Sheet::Paused { seat: extra.seat, lost: extra.lost },
        Phase::Judging => extra.judging.clone().map_or(Sheet::None, Sheet::Judging),
        Phase::Master => Sheet::Master(extra.panel.clone()),
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
        Phase::Writing { .. } => Sheet::Writing { table: extra.seat != SeatState::Alone },
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
            Sheet::Character { lines, points: world.party.iter().any(|h| h.points_to_place > 0), offer: extra.offer.clone() }
        }
        _ => Sheet::None,
    }
}

#[allow(clippy::too_many_arguments)]
fn overlay(
    mut commands: Commands,
    phase: Res<Phase>,
    game: Res<Game>,
    gm: Res<Gm>,
    lang: Res<Lang>,
    art: Res<Art>,
    fonts: Res<Fonts>,
    session: Res<Session>,
    table: Res<Table>,
    panel: Res<Panel>,
    settings: Res<Settings>,
    mut translator: ResMut<Translator>,
    mut shown: Local<Sheet>,
    old: Query<Entity, With<Overlay>>,
) {
    let me = player_name(&settings);
    let solo = matches!(*session, Session::Solo);
    let mut extra = Extra {
        seat: seat_state(&game.world, &session, &me),
        judging: table.judging.clone(),
        panel: PanelView::default(),
        lost: hero_lost(&game.world, &me, solo),
        offer: my_offer(&game.world, &me, solo).map(|offer| offer.feats.iter().map(|f| crate::hero::feat_line(f, *lang)).collect()).unwrap_or_default(),
    };
    if *phase == Phase::Master {
        let others = others_at_table(&game.world, &me);
        extra.panel = PanelView {
            typed: panel.typed.clone(),
            target: others.get(panel.target % others.len().max(1)).cloned().unwrap_or_default(),
            hostile: panel.hostile,
            note: table.master_note.clone(),
            cards: table.cards.iter().map(|card| card.name.clone()).collect(),
            presses: panel.presses,
        };
    }
    // A countdown in a refusal would rebuild the sheet every second and swallow clicks, so
    // the time is fixed when the sheet first appears.
    let wanted = match (&*phase, &*shown) {
        (Phase::Stuck { .. }, Sheet::Stuck(_)) | (Phase::Refused { .. }, Sheet::Refused(_)) if !old.is_empty() => shown.clone(),
        _ => translated(sheet_for(&phase, &game.world, gm.dispatcher.now(), *lang, &extra), foreign(&game.world, *lang), translator.bypass_change_detection()),
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
    let low = matches!(wanted, Sheet::Looking { .. } | Sheet::Writing { .. } | Sheet::Asking(_) | Sheet::Reading(_) | Sheet::Rolling(_) | Sheet::Judging(_) | Sheet::Master(_));
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
            Sheet::Judging(view) => {
                sheet.spawn((Text::default(), T::DraftTitle, fonts.title(1), ink));
                if !view.said.is_empty() {
                    sheet.spawn((Text::new(format!("«{}»", view.said)), fonts.body(), faded));
                }
                sheet.spawn(Node { height: px(72), padding: UiRect::all(px(4)), overflow: Overflow::clip(), ..default() }).with_children(|field| {
                    field.spawn(backing(art.field.clone(), 4.0));
                    field.spawn((
                        JudgeInput,
                        AutoFocus,
                        AutoDirectionalNavigation::default(),
                        Node { width: percent(100), height: percent(100), ..default() },
                        EditableText { allow_newlines: false, max_characters: Some(2000), ..EditableText::new(&view.narration) },
                        TextLayout { linebreak: LineBreak::WordOrCharacter, ..default() },
                        fonts.body(),
                        ink,
                        TextCursorStyle { color: color(INK), selection_color: color(BRASS_LIGHT), ..default() },
                    ));
                });
                if !view.effects.is_empty() || !view.options.is_empty() {
                    sheet.spawn(scroll_list(Node { flex_direction: FlexDirection::Column, row_gap: px(1), ..default() }, 34.0)).with_children(|rows| {
                        for effect in &view.effects {
                            rows.spawn((Text::new(format!("• {effect}")), fonts.body(), faded));
                        }
                        for option in &view.options {
                            rows.spawn((Text::new(format!("→ {option}")), fonts.body(), ink));
                        }
                    });
                }
                sheet.spawn(Node { column_gap: px(4), align_items: AlignItems::Center, ..default() }).with_children(|line| {
                    button(line, &art, &fonts, T::Accept, true, PlayAction::Judge(true));
                    button(line, &art, &fonts, T::Reject, true, PlayAction::Judge(false));
                    line.spawn((Text::default(), T::RedoLabel, fonts.body(), faded));
                    line.spawn(Node { flex_grow: 1.0, height: px(BUTTON_HEIGHT), padding: UiRect::axes(px(5), px(0)), align_items: AlignItems::Center, overflow: Overflow::clip(), ..default() }).with_children(|field| {
                        field.spawn(backing(art.field.clone(), 4.0));
                        field.spawn((
                            RedoInput,
                            AutoDirectionalNavigation::default(),
                            Node { width: percent(100), ..default() },
                            EditableText { allow_newlines: false, max_characters: Some(300), ..default() },
                            TextLayout::no_wrap(),
                            fonts.body(),
                            ink,
                            TextCursorStyle { color: color(INK), selection_color: color(BRASS_LIGHT), ..default() },
                        ));
                    });
                    button(line, &art, &fonts, T::Redo, true, PlayAction::Redo);
                });
            }
            Sheet::Master(view) => {
                sheet.spawn((Text::default(), T::MasterTitle, fonts.title(1), ink));
                sheet.spawn(Node { height: px(BUTTON_HEIGHT), padding: UiRect::axes(px(5), px(0)), align_items: AlignItems::Center, overflow: Overflow::clip(), ..default() }).with_children(|field| {
                    field.spawn(backing(art.field.clone(), 4.0));
                    field.spawn((
                        MasterInput,
                        AutoFocus,
                        AutoDirectionalNavigation::default(),
                        Node { width: percent(100), ..default() },
                        EditableText { allow_newlines: false, max_characters: Some(400), ..EditableText::new(&view.typed) },
                        TextLayout::no_wrap(),
                        fonts.body(),
                        ink,
                        TextCursorStyle { color: color(INK), selection_color: color(BRASS_LIGHT), ..default() },
                    ));
                });
                let tool = |tool: Tool| PlayAction::Tool(tool);
                let wrap = || Node { column_gap: px(3), row_gap: px(2), flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, ..default() };
                sheet.spawn(wrap()).with_children(|line| {
                    button(line, &art, &fonts, T::ToModel, true, tool(Tool::ToModel));
                    button(line, &art, &fonts, T::Narrate, true, tool(Tool::Narrate));
                    let target = if view.target.is_empty() { "—" } else { view.target.as_str() };
                    text_button(line, &art, &fonts, format!("{} {target}", T::WhisperTo.text(*lang)), tool(Tool::Whisper));
                    button(line, &art, &fonts, T::NextPlayer, true, tool(Tool::NextTarget));
                });
                sheet.spawn(wrap()).with_children(|line| {
                    button(line, &art, &fonts, T::ToolCreature, true, tool(Tool::Creature));
                    button(line, &art, &fonts, if view.hostile { T::MoodHostile } else { T::MoodNeutral }, true, tool(Tool::Mood));
                    button(line, &art, &fonts, T::ToolRemove, true, tool(Tool::Remove));
                    button(line, &art, &fonts, T::ToolTile, true, tool(Tool::Tile));
                    button(line, &art, &fonts, T::ToolDamage, true, tool(Tool::Damage));
                    button(line, &art, &fonts, T::ToolHeal, true, tool(Tool::Heal));
                    button(line, &art, &fonts, T::ToolGive, true, tool(Tool::Give));
                    button(line, &art, &fonts, T::ToolFight, true, tool(Tool::Fight));
                    button(line, &art, &fonts, T::ToolRoll, true, tool(Tool::Roll));
                });
                sheet.spawn(wrap()).with_children(|line| {
                    button(line, &art, &fonts, T::ToolInvent, true, tool(Tool::Invent));
                    for (n, card) in view.cards.iter().enumerate() {
                        text_button(line, &art, &fonts, card.clone(), tool(Tool::Card(n)));
                    }
                });
                // The line is there even when empty, so the buttons do not move under the cursor.
                let note = if view.note.is_empty() { " ".to_string() } else { view.note.clone() };
                sheet.spawn((Text::new(note), TextLayout::no_wrap(), Node { overflow: Overflow::clip(), ..default() }, fonts.body(), TextColor(color(WAX_DARK))));
                sheet.spawn((Text::default(), T::MasterHint, fonts.body(), faded));
                sheet.spawn(Node { column_gap: px(6), ..default() }).with_children(|line| {
                    button(line, &art, &fonts, T::SeatLeave, true, PlayAction::SeatLeave);
                    button(line, &art, &fonts, T::Leave, true, PlayAction::Close);
                });
            }
            Sheet::Paused { seat, lost } => {
                sheet.spawn((Text::default(), T::Paused, fonts.title(1), ink));
                button(sheet, &art, &fonts, T::Continue, true, PlayAction::Resume);
                if *lost {
                    button(sheet, &art, &fonts, T::NewHero, true, PlayAction::NewHero);
                }
                button(sheet, &art, &fonts, T::SaveGame, true, PlayAction::Save);
                button(sheet, &art, &fonts, T::ShortRest, !game.world.party.is_empty(), PlayAction::ShortRest);
                match seat {
                    SeatState::Alone => {}
                    SeatState::Mine => button(sheet, &art, &fonts, T::SeatLeave, true, PlayAction::SeatLeave),
                    SeatState::Free | SeatState::Taken => button(sheet, &art, &fonts, T::SeatTake, *seat == SeatState::Free, PlayAction::SeatTake),
                }
                button(sheet, &art, &fonts, T::LongRest, !game.world.party.is_empty(), PlayAction::Rest);
                button(sheet, &art, &fonts, T::Keys, true, PlayAction::Keys);
                button(sheet, &art, &fonts, T::Report, true, PlayAction::Report);
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
            Sheet::Writing { table } => {
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
                    if *table {
                        button(line, &art, &fonts, T::TableTalk, true, PlayAction::Talk);
                    }
                });
                sheet.spawn((Text::default(), if *table { T::WritingHintTable } else { T::WritingHint }, fonts.body(), faded));
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
                    button(line, &art, &fonts, T::Onward, true, (PlayAction::Close, EnterPresses));
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
                sheet.spawn(Node::default()).with_children(|slot| button(slot, &art, &fonts, T::RollDie, true, (PlayAction::Roll, EnterPresses)));
            }
            Sheet::Character { lines, points, offer } => {
                sheet.spawn(scroll_list(Node { flex_direction: FlexDirection::Column, row_gap: px(2), ..default() }, 150.0)).with_children(|rows| {
                    for line in lines {
                        rows.spawn((Text::new(line.clone()), fonts.body(), ink));
                    }
                });
                if !offer.is_empty() {
                    sheet.spawn((Text::default(), T::FeatChoose, fonts.body(), TextColor(color(WAX_DARK))));
                    for (n, line) in offer.iter().enumerate() {
                        text_button(sheet, &art, &fonts, line.clone(), PlayAction::Feat(n));
                    }
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
fn send(said: String, near: Option<String>, game: &Game, gm: &Gm, store: &Store, settings: &Settings, master: &Master, session: &Session, table: &Table) -> Phase {
    let said = said.trim().to_string();
    if said.is_empty() {
        return Phase::Writing { near };
    }
    // One question to the master at a time, the host's own included.
    if session.is_host() && (table.busy.is_some() || !table.drafts.is_empty()) {
        return Phase::Reading { turn: Turn { narration: T::MasterBusy.text(settings.lang).to_string(), options: Vec::new() } };
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
    master.ask(gm, store, settings, Ask::Turn { world: Box::new(game.world.clone()), said: said.clone(), near: near.clone(), direction: None });
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
    table: Res<Table>,
) {
    if !keyboard.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
        return;
    }
    let (Phase::Writing { near }, Ok((entity, input))) = (&*phase, input.single()) else { return };
    if focus.get() == Some(entity) {
        *phase = send(input.value().to_string(), near.clone(), &game, &gm, &store, &settings, &master, &session, &table);
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
    if let PlayAction::Report = *action {
        *phase = Phase::Walking;
        match store.collect_report(&game.world_id, Some(&game.world)) {
            Ok(folder) => {
                info!("a report was collected in {}", folder.display());
                *toast = Toast { text: Some(T::ReportDone), left: 5.0 };
                // Shown to the player, unless this is a test instance with a home of its own.
                if std::env::var_os("PIXELGM_HOME").is_none() {
                    let opener = if cfg!(windows) { "explorer" } else if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
                    let _ = std::process::Command::new(opener).arg(&folder).spawn();
                }
            }
            Err(e) => {
                error!("could not collect a report: {e}");
                *toast = Toast { text: Some(T::ReportFailed), left: 5.0 };
            }
        }
        return;
    }
    if let (PlayAction::Talk, Phase::Writing { .. }) = (*action, &*phase) {
        let typed = input.single().map(|i| i.value().to_string()).unwrap_or_default();
        if !typed.trim().is_empty() {
            session.chat(&mut table, &player_name(&settings), typed.trim());
        }
        *phase = Phase::Walking;
        return;
    }
    // A guest does not change the world: it asks the host, and reads the answer with
    // everyone else when it comes.
    if session.is_client() {
        let asked = match (*action, phase.clone()) {
            (PlayAction::Do(index), Phase::Looking { thing }) => Some(Intent::Do { thing, index }),
            (PlayAction::Choose(index), Phase::Reading { .. }) => Some(Intent::Choose(index)),
            (PlayAction::Roll, Phase::Rolling) => Some(Intent::Roll),
            (PlayAction::Improve(ability), _) => Some(Intent::Improve(ability)),
            (PlayAction::Feat(choice), _) => Some(Intent::Feat(choice)),
            (PlayAction::NewHero, _) => {
                *phase = Phase::Walking;
                next.set(Screen::Hero);
                return;
            }
            (PlayAction::MainMenu, _) => {
                // Leaving the table.
                *session = Session::Solo;
                *phase = Phase::Walking;
                next.set(Screen::Main);
                return;
            }
            (PlayAction::Rest | PlayAction::ShortRest | PlayAction::Retry | PlayAction::Sketch | PlayAction::Stay, _) => return,
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
        (PlayAction::ShortRest, _) => {
            game.world.short_rest(&mut Rng::from_clock());
            *toast = Toast { text: Some(T::ShortRested), left: 3.0 };
            autosave(&store, &game);
            *phase = Phase::Walking;
        }
        (PlayAction::NewHero, _) => {
            autosave(&store, &game);
            *phase = Phase::Walking;
            next.set(Screen::Hero);
        }
        (PlayAction::Feat(choice), _) => {
            let me = player_name(&settings);
            if let Some(hero) = my_offer(&game.world, &me, matches!(*session, Session::Solo)).map(|offer| offer.hero) {
                game.world.choose_feat(hero, choice);
                autosave(&store, &game);
            }
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
                *phase = send(input.value().to_string(), near, &game, &gm, &store, &settings, &master, &session, &table);
            }
        }
        (PlayAction::Choose(index), Phase::Reading { turn }) => {
            if let Some(option) = turn.options.get(index) {
                let outcome = game.world.choose(option);
                *phase = told(outcome_of(outcome), Vec::new(), &mut game, &store, &mut redraw, &mut background, &session, &mut table);
            }
        }
        (PlayAction::AskAgain, Phase::Refused { said, near, .. }) => *phase = send(said, near, &game, &gm, &store, &settings, &master, &session, &table),
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
        // What a guest does to the world happens where that guest stands.
        let home = matches!(intent, Intent::Do { .. } | Intent::Choose(_) | Intent::Roll | Intent::Say { .. } | Intent::Gm(_)).then(|| enter_scene(&mut game, &mut table, &player));
        let shown = match intent {
            Intent::Do { thing, index } => game.world.perform(&thing, index).map(outcome_of),
            Intent::Choose(index) => table.options.get(&game.world.current).and_then(|options| options.get(index)).cloned().map(|option| outcome_of(game.world.choose(&option))),
            Intent::Roll if !game.world.pending_here() => None,
            Intent::Roll => game.world.roll_check(&mut Rng::from_clock()).map(|done| {
                let verdict = if done.success { T::CheckSuccess } else { T::CheckFailure };
                outcome_of(format!("{}: d20 {}{:+} = {} / {} — {}\n{}", done.hero, done.roll.die, done.roll.bonus, done.roll.total(), done.difficulty, verdict.text(settings.lang), done.narration))
            }),
            Intent::Say { text, near } => {
                // One question to the master at a time; a second one is told to wait.
                if table.busy.is_some() || !table.drafts.is_empty() || matches!(*phase, Phase::Asking { .. }) {
                    session.broadcast(&Down::Trouble(T::MasterBusy.text(settings.lang).to_string()));
                } else {
                    let said = match hero_of(&game.world, &player) {
                        Some(hero) => format!("{hero}: {}", text.trim()),
                        None => format!("{player}: {}", text.trim()),
                    };
                    master.ask(&gm, &store, &settings, Ask::Turn { world: Box::new(game.world.clone()), said: said.clone(), near, direction: None });
                    session.broadcast(&Down::Thinking(said.clone()));
                    table.busy = Some(said);
                    table.busy_for = Some(player.clone());
                }
                None
            }
            Intent::Exit { to } => {
                // A guest walks on alone: the party may split up. A place that is only a
                // name is drawn first, and the guest waits at the door.
                let from = game.world.place_of(&player).to_string();
                let leads_there = game.world.locations.get(&from).is_some_and(|place| place.exits.iter().any(|e| e.to == to));
                if leads_there && game.world.locations.contains_key(&to) {
                    game.world.places.insert(player.clone(), to);
                } else if leads_there && !table.travellers.iter().any(|(who, _)| *who == player) {
                    if !master.is_drawing(&to) {
                        let mut seen = game.world.clone();
                        seen.current = from.clone();
                        master.ask(&gm, &store, &settings, Ask::Location { world: Box::new(seen), id: to.clone(), from, ahead: false });
                    }
                    session.send_to(&player, &Down::Drawing(to.clone()));
                    table.travellers.push((player.clone(), to));
                }
                None
            }
            Intent::Fight(act) => {
                // Only the player whose hero is acting may act for it.
                if game.world.acting_owner() == Some(player.as_str()) {
                    let events = game.world.fight_act(&act);
                    if !events.is_empty() {
                        session.broadcast(&Down::FightEvents(events.clone()));
                        // The host watches only a fight it is in.
                        if game.world.fight_here() {
                            view.extend(events);
                            redraw.0 = true;
                        }
                    }
                }
                None
            }
            Intent::Feat(choice) => {
                if let Some(hero) = game.world.owners.iter().position(|o| *o == player) {
                    game.world.choose_feat(hero, choice);
                }
                None
            }
            Intent::Stunt { said, spell } => {
                // Only the player whose hero is acting may try something for it.
                if game.world.acting_owner() == Some(player.as_str()) {
                    view.wait_for_master();
                    master.ask(&gm, &store, &settings, Ask::Stunt { world: Box::new(game.world.clone()), said, spell });
                }
                None
            }
            Intent::Gm(act) => {
                let mut desk = Desk { game: &mut game, table: &mut table, phase: &mut phase, redraw: &mut redraw, background: &mut background, session: &session, settings: &settings, store: &store, gm: &gm, master: &master };
                gm_act(&mut desk, &player, act);
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
        leave_scene(&mut game, &mut table, home);
    }
    // A hostile creature that notices a guest in a place the host is not in starts a fight
    // there. One fight at a time: the others wait their turn.
    if session.is_host() && game.world.fight.is_none() {
        let elsewhere: Vec<world::Token> = game.world.others.iter().filter(|t| !t.place.is_empty() && t.place != game.world.current).cloned().collect();
        for token in elsewhere {
            let home = game.world.enter(&token.place, world::Player { x: token.x, y: token.y });
            let started = game.world.hostile_near(NOTICE_CELLS) && game.world.start_fight(Rng::from_clock());
            game.world.leave(home);
            if started {
                break;
            }
        }
    }
    // A master who left the table leaves no draft waiting: the model's word stands.
    if !session.is_client() && !table.drafts.is_empty() && seated(&game.world).is_none() {
        let gone = game.world.gm.clone().unwrap_or_default();
        let mut desk = Desk { game: &mut game, table: &mut table, phase: &mut phase, redraw: &mut redraw, background: &mut background, session: &session, settings: &settings, store: &store, gm: &gm, master: &master };
        release(&mut desk, &gone);
    }
}

/// Everything the host has at hand when it changes the world and tells the table.
struct Desk<'a> {
    game: &'a mut Game,
    table: &'a mut Table,
    phase: &'a mut Phase,
    redraw: &'a mut Redraw,
    background: &'a mut Background,
    session: &'a Session,
    settings: &'a Settings,
    store: &'a Store,
    gm: &'a Gm,
    master: &'a Master,
}

/// Looks at the world from where a guest stands, for as long as what that guest did is
/// being carried out. While the place is not the host's own, nothing is shown to the host.
fn enter_scene(game: &mut Game, table: &mut Table, player: &str) -> (String, world::Player) {
    let place = game.world.place_of(player).to_string();
    let at = game.world.others.iter().find(|t| t.name == player).map_or(game.world.player, |t| world::Player { x: t.x, y: t.y });
    table.elsewhere = place != game.world.current;
    game.world.enter(&place, at)
}

fn leave_scene(game: &mut Game, table: &mut Table, home: Option<(String, world::Player)>) {
    if let Some(home) = home {
        game.world.leave(home);
        table.elsewhere = false;
    }
}

/// The player in the master's seat, if they are at the table now.
fn seated(world: &World) -> Option<&str> {
    world.gm.as_deref().filter(|master| world.present.iter().any(|p| p == master))
}

/// Applies a turn and tells the table; whispers go only to those they are for. Gives the
/// telling back when there was something to tell.
fn carry_out(desk: &mut Desk, said: &str, draft: &TurnDraft) -> Option<Turn> {
    let (turn, notes) = desk.game.world.take_turn(said, draft);
    let told_turn = if turn.narration.is_empty() && turn.options.is_empty() {
        for note in notes {
            info!("turn repaired: {note}");
        }
        desk.redraw.0 = true;
        autosave(desk.store, desk.game);
        None
    } else {
        match told(turn, notes, desk.game, desk.store, desk.redraw, desk.background, desk.session, desk.table) {
            Phase::Reading { turn } => Some(turn),
            _ => None,
        }
    };
    whisper(desk, &draft.whispers);
    told_turn
}

/// Carries a turn out and puts its telling in front of the host's own player: at once in
/// place of the sheet they were waiting on, otherwise when they are free to read.
fn finish(desk: &mut Desk, said: &str, draft: &TurnDraft) {
    // In a place the host is not in, the host's own screen stays as it is.
    if desk.table.elsewhere {
        carry_out(desk, said, draft);
        return;
    }
    let waiting = matches!(*desk.phase, Phase::Asking { .. } | Phase::Judging);
    match carry_out(desk, said, draft) {
        Some(turn) if waiting => *desk.phase = Phase::Reading { turn },
        Some(turn) => show(turn, desk.phase, desk.table),
        None if waiting => *desk.phase = Phase::Walking,
        None => {}
    }
}

/// Words for one player alone. `to` names a character or a player.
fn whisper(desk: &mut Desk, whispers: &[Whisper]) {
    let me = player_name(desk.settings);
    for said in whispers.iter().filter(|w| !w.text.trim().is_empty()) {
        let world = &desk.game.world;
        let wanted = said.to.trim().to_lowercase();
        let owner = world
            .party
            .iter()
            .position(|hero| hero.name.to_lowercase() == wanted)
            .and_then(|index| world.owners.get(index).cloned())
            .or_else(|| world.present.iter().find(|p| p.to_lowercase() == wanted).cloned());
        match owner {
            Some(owner) if !owner.is_empty() && owner != me => {
                desk.session.send_to(&owner, &Down::Whisper(said.text.trim().to_string()));
            }
            // The host's own, a hero nobody owns, or a name nobody answers to.
            _ => desk.table.unread.push_back(Turn { narration: format!("{} {}", T::WhisperMark.text(desk.settings.lang), said.text.trim()), options: Vec::new() }),
        }
    }
}

/// Puts the draft at the front of the queue before the master.
fn offer(desk: &mut Desk) {
    let Some(view) = desk.table.drafts.front().map(Held::view) else { return };
    let Some(master) = seated(&desk.game.world).map(str::to_string) else { return };
    if master == player_name(desk.settings) {
        desk.table.judging = Some(view);
        if matches!(*desk.phase, Phase::Walking | Phase::Asking { .. } | Phase::Master) {
            *desk.phase = Phase::Judging;
        }
    } else {
        desk.session.send_to(&master, &Down::Draft(Some(view)));
    }
}

/// Takes the judged draft's sheet away from the master.
fn clear_judging(desk: &mut Desk, master: &str) {
    if master == player_name(desk.settings) {
        desk.table.judging = None;
    } else {
        desk.session.send_to(master, &Down::Draft(None));
    }
}

/// The master has judged the draft at the front of the queue.
fn conclude(desk: &mut Desk, master: &str, accept: bool, narration: &str) {
    let Some(held) = desk.table.drafts.pop_front() else { return };
    clear_judging(desk, master);
    if accept {
        let mut draft = held.draft;
        if !narration.trim().is_empty() {
            draft.narration = narration.trim().to_string();
        }
        let home = held.scene.as_deref().map(|player| enter_scene(desk.game, desk.table, player));
        finish(desk, &held.said, &draft);
        leave_scene(desk.game, desk.table, home);
    } else {
        desk.session.broadcast(&Down::Trouble(T::MasterDeclined.text(desk.settings.lang).to_string()));
        if matches!(*desk.phase, Phase::Asking { .. } | Phase::Judging) {
            *desk.phase = Phase::Walking;
        }
    }
    offer(desk);
}

/// With the seat empty the drafts that were waiting go through as the model wrote them.
fn release(desk: &mut Desk, master: &str) {
    while !desk.table.drafts.is_empty() {
        conclude(desk, master, true, "");
    }
}

/// What the player in the master's seat asked for, done on the host. Anyone may take an
/// empty seat; everything else is only the master's to do.
fn gm_act(desk: &mut Desk, player: &str, act: GmAct) {
    if let GmAct::Seat(true) = act {
        if seated(&desk.game.world).is_none() {
            desk.game.world.gm = Some(player.to_string());
        }
        return;
    }
    if desk.game.world.gm.as_deref() != Some(player) {
        return;
    }
    match act {
        GmAct::Seat(_) => {
            desk.game.world.gm = None;
            release(desk, player);
        }
        GmAct::Verdict { accept, narration } => conclude(desk, player, accept, &narration),
        GmAct::Direct(text) => {
            let text = text.trim().to_string();
            if text.is_empty() {
                return;
            }
            if desk.table.busy.is_some() {
                let busy = T::MasterBusy.text(desk.settings.lang).to_string();
                if player == player_name(desk.settings) {
                    desk.table.master_note = busy;
                } else {
                    desk.session.send_to(player, &Down::Card(Err(busy)));
                }
                return;
            }
            // Over a waiting draft the direction says how that answer is to be redone.
            let (said, near) = match desk.table.drafts.pop_front() {
                Some(held) => {
                    clear_judging(desk, player);
                    desk.table.busy_for = held.scene;
                    (held.said, held.near)
                }
                None => {
                    // A direction of the master's own happens where the master is.
                    desk.table.busy_for = Some(player.to_string()).filter(|_| player != player_name(desk.settings));
                    (String::new(), None)
                }
            };
            if *desk.phase == Phase::Judging {
                *desk.phase = Phase::Walking;
            }
            desk.master.ask(desk.gm, desk.store, desk.settings, Ask::Turn { world: Box::new(desk.game.world.clone()), said: said.clone(), near, direction: Some(text.clone()) });
            desk.session.broadcast(&Down::Thinking(text));
            desk.table.busy = Some(said);
        }
        GmAct::Apply(draft) => finish(desk, "", &draft),
        GmAct::Whisper { to, text } => whisper(desk, &[Whisper { to, text }]),
        GmAct::Invent(request) => {
            desk.master.ask(desk.gm, desk.store, desk.settings, Ask::Invent { world: Box::new(desk.game.world.clone()), request, for_player: player.to_string() });
        }
    }
}

/// What a tool of the master's panel comes to, given the text in the panel's field.
/// `None` when the press only changed the panel, or could not be carried out.
fn tool_act(tool: Tool, typed: &str, world: &World, panel: &mut Panel, table: &mut Table, lang: Lang, me: &str) -> Option<GmAct> {
    let typed = typed.trim();
    panel.typed = typed.to_string();
    panel.presses = panel.presses.wrapping_add(1);
    let (x, y) = world.player.cell();
    let (x, y) = (x.max(0) as usize, y.max(0) as usize);
    let apply = |effects: serde_json::Value| GmAct::Apply(Box::new(TurnDraft { effects: effects.as_array().cloned().unwrap_or_default(), ..TurnDraft::default() }));
    let needs_text = matches!(tool, Tool::ToModel | Tool::Narrate | Tool::Whisper | Tool::Creature | Tool::Tile | Tool::Damage | Tool::Heal | Tool::Give | Tool::Invent);
    if needs_text && typed.is_empty() {
        table.master_note = T::NeedText.text(lang).to_string();
        return None;
    }
    table.master_note.clear();
    let act = match tool {
        Tool::ToModel => GmAct::Direct(typed.to_string()),
        Tool::Narrate => GmAct::Apply(Box::new(TurnDraft { narration: typed.to_string(), ..TurnDraft::default() })),
        Tool::Whisper => {
            let others = others_at_table(world, me);
            let Some(to) = others.get(panel.target % others.len().max(1)) else {
                table.master_note = T::NobodyToWhisper.text(lang).to_string();
                return None;
            };
            GmAct::Whisper { to: to.clone(), text: typed.to_string() }
        }
        Tool::NextTarget => {
            panel.target = panel.target.wrapping_add(1);
            return None;
        }
        Tool::Mood => {
            panel.hostile = !panel.hostile;
            return None;
        }
        Tool::Creature => {
            // A monster of the rules by its index, or anything at all by its name.
            let index = typed.to_lowercase().replace([' ', '_'], "-");
            let card = match rules::srd().monsters.get(index.as_str()) {
                Some(monster) => CreatureCard { id: index.clone(), name: tr(lang == Lang::Ru, &monster.name).to_string(), monster: Some(index), ..CreatureCard::default() },
                None => CreatureCard { id: typed.to_string(), name: typed.to_string(), ..CreatureCard::default() },
            };
            GmAct::Apply(Box::new(card.summon(x + 1, y, panel.hostile)))
        }
        Tool::Remove => match world.thing_near(REMOVE_REACH) {
            Some(id) => apply(serde_json::json!([{ "op": "remove", "id": id }])),
            None => {
                table.master_note = T::NothingNear.text(lang).to_string();
                return None;
            }
        },
        Tool::Tile => apply(serde_json::json!([{ "op": "tile", "x": x, "y": y, "tile": typed }])),
        Tool::Damage => apply(serde_json::json!([{ "op": "damage", "amount": typed }])),
        Tool::Heal => apply(serde_json::json!([{ "op": "heal", "amount": typed }])),
        Tool::Give => apply(serde_json::json!([{ "op": "give", "name": typed }])),
        Tool::Fight => apply(serde_json::json!([{ "op": "fight" }])),
        Tool::Roll => {
            // Nobody else sees this roll. The field, if it holds a number, is the bonus.
            let roll = Roll::d20(&mut Rng::from_clock(), typed.parse().unwrap_or(0), Edge::Plain);
            table.master_note = format!("d20: {}{:+} = {}", roll.die, roll.bonus, roll.total());
            return None;
        }
        Tool::Invent => {
            table.master_note = T::Inventing.text(lang).to_string();
            GmAct::Invent(typed.to_string())
        }
        Tool::Card(n) => GmAct::Apply(Box::new(table.cards.get(n)?.summon(x + 1, y, panel.hostile))),
    };
    // The parameter was used up.
    panel.typed.clear();
    Some(act)
}

/// The buttons of the master's seat, on whichever machine the master sits at.
#[allow(clippy::too_many_arguments)]
fn on_master_activate(
    used: On<Activate>,
    buttons: Query<&PlayAction>,
    fields: MasterFields,
    mut phase: ResMut<Phase>,
    game: Option<ResMut<Game>>,
    mut table: ResMut<Table>,
    mut panel: ResMut<Panel>,
    mut toast: ResMut<Toast>,
    mut redraw: ResMut<Redraw>,
    mut background: ResMut<Background>,
    session: Res<Session>,
    settings: Res<Settings>,
    store: Res<Store>,
    gm: Res<Gm>,
    master: Res<Master>,
) {
    let (Ok(action), Some(mut game)) = (buttons.get(used.entity), game) else { return };
    let text_of = |field: Result<&EditableText, bevy::ecs::query::QuerySingleError>| field.map(|f| f.value().to_string()).unwrap_or_default();
    let me = player_name(&settings);
    let guest = session.is_client();
    let act = match (*action, phase.clone()) {
        (PlayAction::SeatTake, _) => {
            *toast = Toast { text: Some(T::SeatYours), left: 6.0 };
            *phase = Phase::Walking;
            GmAct::Seat(true)
        }
        (PlayAction::SeatLeave, _) => {
            *phase = Phase::Walking;
            GmAct::Seat(false)
        }
        (PlayAction::Judge(accept), Phase::Judging) => {
            let narration = text_of(fields.judge.single());
            if guest {
                table.judging = None;
                *phase = Phase::Walking;
            }
            GmAct::Verdict { accept, narration }
        }
        (PlayAction::Redo, Phase::Judging) => {
            let direction = text_of(fields.redo.single());
            if direction.trim().is_empty() {
                return;
            }
            if guest {
                table.judging = None;
                *phase = Phase::Walking;
            }
            GmAct::Direct(direction)
        }
        (PlayAction::Tool(tool), Phase::Master) => {
            let typed = text_of(fields.panel.single());
            let Some(act) = tool_act(tool, &typed, &game.world, &mut panel, &mut table, settings.lang, &me) else { return };
            // A direction is answered with a draft: the panel steps aside for it.
            if tool == Tool::ToModel {
                *phase = Phase::Walking;
            }
            act
        }
        _ => return,
    };
    if guest {
        session.intend(Intent::Gm(act));
    } else {
        let mut desk = Desk { game: &mut game, table: &mut table, phase: &mut phase, redraw: &mut redraw, background: &mut background, session: &session, settings: &settings, store: &store, gm: &gm, master: &master };
        gm_act(&mut desk, &me, act);
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
            .init_resource::<Panel>()
            .add_systems(OnEnter(Screen::Play), enter)
            .add_systems(OnExit(Screen::Play), leave)
            .add_systems(
                Update,
                (submit, keys, walk, advance, receive, guests, background_work, draw, place_hero, place_fellows, idle, hud, chat_log, veil, overlay).chain().run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>),
            )
            .add_observer(on_activate)
            .add_observer(on_master_activate);
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
        let sheet = sheet_for(&Phase::Looking { thing: "cat".into() }, &w, 0, Lang::En, &Extra::default());
        let Sheet::Looking { name, actions, .. } = sheet else { panic!("expected the looking sheet") };
        assert_eq!(name, "Grey cat");
        assert_eq!(actions, ["Pet the cat", "Shoo it away"]);
    }

    #[test]
    fn looking_at_something_that_is_gone_shows_nothing() {
        assert!(sheet_for(&Phase::Looking { thing: "ghost".into() }, &world(), 0, Lang::En, &Extra::default()) == Sheet::None);
    }

    #[test]
    fn the_journal_lists_what_is_carried_and_what_was_said() {
        let mut w = world();
        w.perform("cat", 1);
        let Sheet::Journal { carried, lines } = sheet_for(&Phase::Journal, &w, 0, Lang::En, &Extra::default()) else { panic!("expected the journal") };
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
