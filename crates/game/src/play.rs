//! Playing: the current location on screen, the party walking about it, the passage from one
//! place to the next, and the conversation with the game master.

use crate::audio::Sfx;
use crate::coop::{Down, DraftView, GmAct, Held, Intent, Session, Table, Want, player_name};
use crate::fx::{Actor, Fx};
use crate::gm::{Gm, Trouble, trouble_text};
use crate::i18n::{Lang, T};
use crate::master::{Answer, Ask, Master};
use crate::menu::{Backdrop, ReturnTo, Screen};
use crate::screen::CANVAS;
use crate::shot::Warp;
use crate::store::{Initiative, Settings, Store};
use crate::theme::*;
use crate::translate::{Translator, foreign};
use bevy::prelude::*;
use bevy::sprite::Anchor;
use world::base::MARKER_SPRITE;
use world::walk::FEET;
use crate::hero::sheet_lines;
use rules::dice::Edge;
use rules::{Ability, Rng, Roll, Side, tr};
use world::{CreatureCard, LocationDraft, Speaker, TILE, Turn, TurnDraft, Whisper, World, fake};

mod hud;
mod sheets;

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
    /// The party's bag and what the hero wears.
    Bag,
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
pub struct HeroPose {
    pub moving: bool,
    pub facing: crate::doll::Facing,
    /// Seconds of walking, which pick the step of the walk.
    pub clock: f32,
}

/// The pictures of the place being shown, kept while the place does not change.
#[derive(Resource, Default)]
struct SceneCache {
    key: u64,
    ground: Vec<Handle<Image>>,
    standing: Vec<(Handle<Image>, (f32, f32))>,
    hour: Option<crate::scene::Hour>,
    lamps: Vec<crate::scene::Lamp>,
    dark: Handle<Image>,
    warm: Handle<Image>,
    shadow: Handle<Image>,
    vignette: Handle<Image>,
    grid: Handle<Image>,
    water: Vec<(usize, usize)>,
    sky: crate::weather::Sky,
    roofs: Vec<(Handle<Image>, (i32, i32), Vec<(usize, usize)>)>,
    /// Seconds since the place was painted: the grid shows for the first few.
    age: f32,
}

#[derive(Component)]
struct WalkGrid;

/// What the heroes notice of what is hidden, and the traps they walk into: the host looks
/// where each player stands, and tells each what they found.
#[allow(clippy::too_many_arguments)]
fn notice(time: Res<Time>, mut game: ResMut<Game>, mut table: ResMut<Table>, phase: Res<Phase>, session: Res<Session>, settings: Res<Settings>, mut redraw: ResMut<Redraw>, mut since: Local<f32>) {
    if session.is_client() || !matches!(*phase, Phase::Walking | Phase::Bag) {
        return;
    }
    *since += time.delta_secs();
    if *since < 0.4 {
        return;
    }
    *since = 0.0;
    let me = player_name(&settings);
    let mut players: Vec<String> = vec![me.clone()];
    players.extend(session.players());
    for player in players {
        let home = (player != me).then(|| enter_scene(&mut game, &mut table, &player));
        let found = game.world.notice();
        leave_scene(&mut game, &mut table, home);
        for note in found {
            redraw.0 = true;
            if player == me {
                table.heard("★", &note);
            } else {
                session.send_to(&player, &Down::Chat { from: "★".into(), text: note });
            }
        }
    }
}

/// Where the story put heroes, and where it sent them: the host moves its own and tells each
/// guest, and a guest sent to a place not drawn yet waits at the door while it is drawn.
#[allow(clippy::too_many_arguments)]
fn relocate(mut game: ResMut<Game>, session: Res<Session>, settings: Res<Settings>, mut leave: ResMut<Leave>, mut table: ResMut<Table>, master: Res<Master>, gm: Res<Gm>, store: Res<Store>) {
    if session.is_client() || (game.world.moves.is_empty() && game.world.travel.is_none()) {
        return;
    }
    let me = player_name(&settings);
    let owner = |world: &World, h: usize| world.owners.get(h).filter(|o| !o.is_empty()).cloned().unwrap_or_else(|| me.clone());
    for (h, (x, y)) in std::mem::take(&mut game.world.moves) {
        let who = owner(&game.world, h);
        if who == me {
            if game.world.hero_place(h) == game.world.current || game.world.owners.is_empty() {
                game.world.player = world::Player::in_cell(x, y);
            }
        } else {
            session.send_to(&who, &Down::MoveTo { x, y });
        }
    }
    if let Some((to, heroes)) = game.world.travel.take() {
        for h in heroes {
            let who = owner(&game.world, h);
            if who == me {
                leave.0 = Some(to.clone());
            } else if game.world.locations.contains_key(&to) {
                game.world.places.insert(who, to.clone());
            } else if !table.travellers.iter().any(|(p, _)| *p == who) {
                if !master.is_drawing(&to) {
                    let mut seen = game.world.clone();
                    let from = game.world.place_of(&who).to_string();
                    seen.current = from.clone();
                    master.ask(&gm, &store, &settings, Ask::Location { world: Box::new(seen), id: to.clone(), from, ahead: false });
                }
                session.send_to(&who, &Down::Drawing(to.clone()));
                table.travellers.push((who, to.clone()));
            }
        }
    }
}

/// The outcome of a choice is shown only while its continuation is awaited.
fn forget_going_on(phase: Res<Phase>, mut table: ResMut<Table>) {
    if table.going_on.is_some() && !matches!(*phase, Phase::Asking { .. }) {
        table.going_on = None;
    }
}

/// Things gained and lost, a line each in the corner for a few seconds.
/// The arrow over a way out, bobbing towards where it leads.
#[derive(Component)]
struct ExitMarker {
    home: Vec3,
    dir: Vec2,
}

/// The cell of a way out, for its arrow and its words.
#[derive(Component)]
struct ExitAt(usize, usize);

/// In the night a way out shows only once the party has seen where it is.
fn exits_seen(game: Res<Game>, settings: Res<Settings>, cache: Res<SceneCache>, mut marks: Query<(&ExitAt, &mut Visibility)>) {
    let dark = (settings.gfx.night && cache.hour.is_some_and(|h| h.dark >= 0.8)) || (settings.gfx.weather && cache.sky == crate::weather::Sky::Fog);
    let here = game.world.here();
    for (at, mut visibility) in &mut marks {
        let wanted = if !dark || here.was_seen(at.0, at.1) { Visibility::Inherited } else { Visibility::Hidden };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

fn exit_markers(time: Res<Time>, mut markers: Query<(&ExitMarker, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (marker, mut transform) in &mut markers {
        let nudge = ((t * 3.0).sin() * 3.0).round() * 2.0;
        transform.translation = marker.home + (marker.dir * nudge).extend(0.0);
    }
}

/// The roof of a building, by its number in the place.
#[derive(Component)]
struct RoofOver(usize);

/// A roof fades away while the hero is under it, and comes back when the hero leaves.
fn roofs(time: Res<Time>, game: Res<Game>, cache: Res<SceneCache>, mut roofs: Query<(&RoofOver, &mut Sprite)>) {
    let (x, y) = game.world.player.cell();
    let feet = ((game.world.player.x + FEET.0) / TILE as f32, (game.world.player.y + FEET.1) / TILE as f32);
    let cells = [(x.max(0) as usize, y.max(0) as usize), (feet.0.max(0.0) as usize, feet.1.max(0.0) as usize)];
    for (roof, mut sprite) in &mut roofs {
        // Seen through from a couple of cells away, so the hero sees what is inside before stepping in.
        let under = cache.roofs.get(roof.0).is_some_and(|(_, _, inside)| cells.iter().any(|c| crate::scene::Roof::near(inside, *c)));
        let wanted = if under { 0.18 } else { 1.0 };
        let now = sprite.color.alpha();
        let next = if (now - wanted).abs() < 0.05 { wanted } else { now + (wanted - now).signum() * time.delta_secs() * 3.0 };
        if (now - next).abs() > f32::EPSILON {
            sprite.color = Color::srgba(1.0, 1.0, 1.0, next);
        }
    }
}

/// The frames of every hero drawn so far, by what shows on the hero.
#[derive(Resource, Default)]
struct Dolls(std::collections::HashMap<String, Vec<Handle<Image>>>);

impl Dolls {
    fn of(&mut self, hero: &rules::Character, light: bool, images: &mut Assets<Image>) -> Vec<Handle<Image>> {
        self.0.entry(format!("{}|{light}", crate::doll::key(hero))).or_insert_with(|| crate::doll::frames_holding(hero, light).into_iter().map(|frame| images.add(frame.into_image())).collect()).clone()
    }
}

/// A hero's frames, on the sprite that shows the hero.
#[derive(Component)]
struct Frames(Vec<Handle<Image>>);

/// A hero who put on other armour, took up another weapon or a torch looks it at once in
/// the world too, not only in the bag.
fn redress(game: Res<Game>, settings: Res<Settings>, session: Res<Session>, mut dolls: ResMut<Dolls>, mut images: ResMut<Assets<Image>>, mut shown: Query<(&mut Frames, Option<&Fellow>), Or<(With<Hero>, With<Fellow>)>>) {
    let world = &game.world;
    let me = player_name(&settings);
    for (mut frames, fellow) in &mut shown {
        // The one led in a hero's place keeps its own picture.
        if !dolls.0.values().any(|known| *known == frames.0) {
            continue;
        }
        let index = match fellow {
            Some(other) => world.owners.iter().position(|o| *o == other.0),
            None => my_hero(world, &me, matches!(*session, Session::Solo)),
        };
        let Some((i, hero)) = index.and_then(|i| world.party.get(i).map(|hero| (i, hero))) else { continue };
        let now = dolls.of(hero, world.held_light(i).is_some(), &mut images);
        if now != frames.0 {
            frames.0 = now;
        }
    }
}

/// How another player's hero is walking: which way, which step, and where it was.
#[derive(Component, Default)]
struct Gait {
    facing: crate::doll::Facing,
    clock: f32,
    last: Vec2,
    still: f32,
    /// Where the figure is drawn, between the places its player was last heard to be.
    glide: Glide,
}

/// Another player's place comes a few times a second, and never evenly. Drawn where it was
/// last heard to be, the figure jumps; so it is walked from where it is drawn to where it
/// was heard, over as long as the news took to come.
#[derive(Default, Clone, Copy, PartialEq, Debug)]
struct Glide {
    from: Vec2,
    to: Vec2,
    /// Seconds since the last news, and how long the walk to it is given.
    since: f32,
    span: f32,
    /// Seconds between the last two pieces of news.
    gap: f32,
    shown: Vec2,
    started: bool,
}

/// News comes about this often; a walk is never given less than half of it nor more than
/// a quarter of a second, so that late news is caught up with and a pause is not a crawl.
const GLIDE_USUAL: f32 = 0.1;
/// Farther than this in one piece of news is not a walk: the story set the hero down there.
const GLIDE_JUMP: f32 = 3.0 * TILE as f32;

impl Glide {
    fn at(place: Vec2) -> Self {
        Self { from: place, to: place, shown: place, span: GLIDE_USUAL, gap: GLIDE_USUAL, since: 0.0, started: true }
    }

    /// One frame: `heard` is where the player was last heard to be. Gives where to draw.
    fn step(&mut self, heard: Vec2, dt: f32) -> Vec2 {
        if !self.started || heard.distance(self.shown) > GLIDE_JUMP {
            *self = Self::at(heard);
            return heard;
        }
        self.since += dt;
        self.gap += dt;
        if heard != self.to {
            // The walk to the new place takes as long as the news took to come: the figure
            // arrives as the next news is due, and so never stops between two of them.
            self.span = self.gap.clamp(GLIDE_USUAL * 0.5, 0.25);
            self.gap = 0.0;
            self.from = self.shown;
            self.to = heard;
            self.since = dt;
        }
        self.shown = self.from.lerp(self.to, (self.since / self.span).min(1.0));
        self.shown
    }
}

#[derive(Component)]
struct GroundLayer;

#[derive(Component)]
struct DarkLayer;

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
    /// Sounds, and a die to throw, asked for by code that has only this at hand.
    cues: Vec<Sfx>,
    die: Option<u32>,
    /// A hero rose a level: something to see as well as to read.
    level_up: bool,
}

#[derive(Component)]
struct InLocation;

#[derive(Component)]
pub struct Hero;

/// Another player's token.
#[derive(Component)]
pub struct Fellow(pub String);

#[derive(Component)]
struct Veil;

/// What the master's panel can do. The panel's text field is each tool's parameter.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool {
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
    /// Coins to the party (or from it, with a minus).
    Gold,
    /// So many hours pass.
    Hours,
    /// The sky, or the hour: rain, fog, clear… or day, dusk, night, dark.
    Sky,
    /// Whoever is nearest says the words in the field, as a bubble.
    Say,
    /// The tile named in the field is painted with clicks on the map, until pressed again.
    Brush,
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
    /// The tile the brush lays with each click on the map, while it is on.
    brush: Option<String>,
}

/// With the brush on, the master paints the map: each cell clicked, or dragged over with the
/// button held, takes the tile.
fn brush_paint(phase: Res<Phase>, panel: Res<Panel>, mouse: Res<ButtonInput<MouseButton>>, window: Single<&Window, With<bevy::window::PrimaryWindow>>, frame: Res<crate::view::View>, session: Res<Session>, settings: Res<Settings>, mut table: ResMut<Table>, mut last: Local<Option<(usize, usize)>>) {
    let (Phase::Master, Some(tile), true) = (&*phase, &panel.brush, mouse.pressed(MouseButton::Left)) else {
        *last = None;
        return;
    };
    let Some(point) = crate::view::map_under_cursor(&window, &frame) else { return };
    let side = TILE as f32 * crate::scene::PX as f32;
    let cell = ((point.x / side).floor() as i32, (point.y / side).floor() as i32);
    if cell.0 < 0 || cell.1 < 0 || cell.0 >= world::MAX_W as i32 || cell.1 >= world::MAX_H as i32 {
        return;
    }
    let cell = (cell.0 as usize, cell.1 as usize);
    if *last == Some(cell) {
        return;
    }
    *last = Some(cell);
    let act = GmAct::Apply(Box::new(TurnDraft { effects: vec![serde_json::json!({ "op": "tile", "x": cell.0, "y": cell.1, "tile": tile })], ..TurnDraft::default() }));
    if session.is_client() {
        session.intend(Intent::Gm(act));
    } else {
        table.inbox.push((player_name(&settings), Intent::Gm(act)));
    }
}

/// How far from the master's token "nearest" reaches when removing a thing.
const REMOVE_REACH: f32 = 48.0;

/// How long a line of table talk stays on screen, and how many are shown at once.
const CHAT_SECONDS: f32 = 14.0;
const CHAT_LINES: usize = 4;

#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub enum PlayAction {
    Resume,
    Save,
    Keys,
    MainMenu,
    Retry,
    Sketch,
    Stay,
    /// A prepared action of the thing being looked at.
    Do(usize),
    /// Open, pick or force the container being looked at.
    Chest(world::bag::Opening),
    /// Walk up to what a choice needs, then take it.
    Approach(usize),
    /// See what the one being looked at has to sell.
    Trade,
    /// Have the master draw the place the party is in once more.
    RedrawPlace,
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
    /// Turn the next levels towards another class.
    Advance,
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
pub const HUD_HEIGHT: f32 = 18.0;
/// A hostile creature this many cells away notices the party and attacks.
const NOTICE_CELLS: u32 = 3;
/// After the master refuses unasked-for work, how long before it is offered again.
const BACKGROUND_REST: f32 = 600.0;
const JOURNAL_LINES: usize = 80;

/// A point of the location, in pixels from its top-left corner, as a point of the screen.
pub fn on_screen(x: f32, y: f32) -> Vec2 {
    let px = crate::scene::PX as f32;
    Vec2::new(x * px - CANVAS.x as f32 / 2.0, CANVAS.y as f32 / 2.0 - y * px)
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

/// Plays by the edition of the rules the world was made with.
pub fn use_rules_of(world: &World) {
    rules::use_edition(if world.table.srd_2024 { rules::Edition::Srd52 } else { rules::Edition::Srd51 });
}

/// Puts a world on the play screen. Used by a new game and by loading a save.
pub fn begin(commands: &mut Commands, next: &mut NextState<Screen>, world: World, world_id: String) {
    use_rules_of(&world);
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

fn enter(mut commands: Commands, game: Option<Res<Game>>, store: Res<Store>, mut next: ResMut<NextState<Screen>>, mut backdrop: Query<&mut Visibility, With<Backdrop>>) {
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
    commands.spawn((DespawnOnExit(Screen::Play), GlobalZIndex(10), Pickable::IGNORE, Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..full })).with_children(|root| {
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
        ;
    });
}

fn leave(mut commands: Commands, things: Query<Entity, With<InLocation>>, mut backdrop: Query<&mut Visibility, With<Backdrop>>) {
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

/// A key that changes when anything painted into the ground of a place changes.
fn scene_key(world: &World) -> u64 {
    use std::hash::{Hash, Hasher};
    let here = world.here();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    world.current.hash(&mut hasher);
    here.map.hash(&mut hasher);
    here.legend.hash(&mut hasher);
    here.light.hash(&mut hasher);
    world.companions.len().hash(&mut hasher);
    world.party.iter().filter(|h| h.dead || h.hp <= 0).count().hash(&mut hasher);
    here.marks.hash(&mut hasher);
    for prop in &here.props {
        (&prop.id, prop.x, prop.y, &prop.sprite, &prop.name).hash(&mut hasher);
    }
    world.palette.colors.hash(&mut hasher);
    for (id, tile) in &world.tiles {
        (id, &tile.sprite.rows).hash(&mut hasher);
    }
    hasher.finish()
}

/// A soft shadow under anything that walks, as a child of its sprite.
fn shadow_under(parent: &mut ChildSpawnerCommands, image: &Handle<Image>) {
    parent.spawn((Sprite::from_image(image.clone()), Transform::from_xyz(0.0, 1.0, -0.0005)));
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    mut redraw: ResMut<Redraw>,
    game: Res<Game>,
    mut images: ResMut<Assets<Image>>,
    old: Query<Entity, With<InLocation>>,
    session: Res<Session>,
    settings: Res<Settings>,
    mut cache: ResMut<SceneCache>,
    mut dolls: ResMut<Dolls>,
    fonts: Res<Fonts>,
) {
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
    // The ground and what stands on it are painted once for a place and kept.
    let key = scene_key(world);
    if cache.key != key || cache.ground.is_empty() {
        let scene = crate::scene::paint(here, world);
        cache.ground = scene.ground.into_iter().map(|picture| images.add(picture.into_image())).collect();
        cache.standing = scene.standing.into_iter().map(|s| (images.add(s.picture.into_image()), s.foot)).collect();
        cache.hour = Some(scene.hour);
        cache.water = scene.water.clone();
        // Indoors and underground there is no sky to rain or cast cloud shadows.
        cache.sky = if scene.indoors || scene.hour.dark >= 0.8 { crate::weather::Sky::Clear } else { crate::weather::Sky::of(&here.weather) };
        cache.roofs = scene.roofs.into_iter().map(|r| (images.add(r.picture.into_image()), r.at, r.inside)).collect();
        cache.lamps = scene.lamps;
        cache.key = key;
        if cache.shadow == Handle::default() {
            cache.shadow = images.add(crate::scene::foot_shadow().into_image());
            cache.warm = images.add(crate::scene::warm_pool(120).into_image());
            cache.vignette = images.add(crate::scene::vignette(CANVAS.x as i32, (CANVAS.y as f32 - HUD_HEIGHT * crate::screen::UI_PIXEL) as i32).into_image());
        }
        cache.dark = images.add(crate::scene::darkness_seen(here.size(), scene.hour, &[], 0, None, None).into_image());
        cache.grid = images.add(crate::scene::walk_grid(here, world).into_image());
        cache.age = 0.0;
    }
    let corner = on_screen(0.0, 0.0);
    commands.spawn((InLocation, GroundLayer, Sprite::from_image(cache.ground[0].clone()), Anchor::TOP_LEFT, Transform::from_translation(corner.extend(0.0))));
    for (image, (x, y)) in &cache.standing {
        commands.spawn((InLocation, Sprite::from_image(image.clone()), Anchor::BOTTOM_CENTER, Transform::from_translation(on_screen(*x, *y).extend(depth(*y)))));
    }
    let hour = cache.hour.unwrap_or(crate::scene::Hour { dark: 0.0, tint: [0, 0, 0] });
    let foggy = cache.sky == crate::weather::Sky::Fog && settings.gfx.weather;
    if (hour.dark > 0.0 && settings.gfx.night) || foggy {
        // The dark over the place, and the warmth around its lamps.
        // As large as the place itself, whatever its size.
        let side = (TILE * crate::scene::PX as u32) as f32;
        let map = Vec2::new(here.size().0 as f32 * side, here.size().1 as f32 * side);
        commands.spawn((InLocation, DarkLayer, Sprite { custom_size: Some(map), ..Sprite::from_image(cache.dark.clone()) }, Anchor::TOP_LEFT, Transform::from_translation(corner.extend(1.6))));
        for lamp in &cache.lamps {
            commands.spawn((InLocation, Sprite { color: Color::linear_rgb(1.6, 1.4, 1.1), ..Sprite::from_image(cache.warm.clone()) }, Transform::from_translation(on_screen(lamp.x, lamp.y).extend(1.62))));
        }
    }
    if settings.gfx.vignette {
        commands.spawn((InLocation, crate::view::ScreenFixed(corner), Sprite::from_image(cache.vignette.clone()), Anchor::TOP_LEFT, Transform::from_translation(corner.extend(1.7))));
    }
    let sky = cache.sky;
    if settings.gfx.weather && sky != crate::weather::Sky::Clear {
        let cell = (TILE * crate::scene::PX as u32) as f32;
        let map = Rect::new(0.0, 0.0, here.size().0 as f32 * cell, here.size().1 as f32 * cell);
        let water = cache.water.iter().map(|(x, y)| Rect::new(*x as f32 * cell, *y as f32 * cell, (*x + 1) as f32 * cell, (*y + 1) as f32 * cell)).collect();
        let mut emitter = crate::weather::Emitter::new(sky, map, water);
        emitter.dim = if settings.gfx.night { hour.dark } else { 0.0 };
        commands.spawn((InLocation, emitter, Transform::default(), Visibility::default()));
    }
    for (n, (image, (x, y), _)) in cache.roofs.iter().enumerate() {
        let at = Vec3::new(*x as f32 - CANVAS.x as f32 / 2.0, CANVAS.y as f32 / 2.0 - *y as f32, 1.5);
        commands.spawn((InLocation, RoofOver(n), Sprite::from_image(image.clone()), Anchor::TOP_LEFT, Transform::from_translation(at)));
    }
    if settings.gfx.scan_lines {
        let lines = images.add(crate::post::scan_lines(CANVAS.x, CANVAS.y - (HUD_HEIGHT * crate::screen::UI_PIXEL) as u32).into_image());
        commands.spawn((InLocation, crate::view::ScreenFixed(corner), Sprite::from_image(lines), Anchor::TOP_LEFT, Transform::from_translation(corner.extend(1.72))));
    }
    // Under the dark, so that the grid of a place not yet explored is not given away.
    commands.spawn((InLocation, WalkGrid, Sprite { color: Color::NONE, ..Sprite::from_image(cache.grid.clone()) }, Anchor::TOP_LEFT, Transform::from_translation(corner.extend(1.58))));

    // What lies on the ground, small, with a shadow.
    for lying in &here.items {
        let image = images.add(crate::bag_ui::picture(&lying.item, world).into_image());
        let (x, y) = ((lying.x as f32 + 0.5) * TILE as f32, (lying.y as f32 + 0.8) * TILE as f32);
        let glow = if lying.item.light > 0 { Color::linear_rgb(1.8, 1.5, 1.2) } else { Color::WHITE };
        commands.spawn((InLocation, crate::fx::OnGround((lying.x, lying.y)), Sprite { color: glow, ..Sprite::from_image(image) }, Anchor::BOTTOM_CENTER, Transform::from_translation(on_screen(x, y).extend(0.95)).with_scale(Vec3::splat(1.0))));
    }
    // Every way out is marked: a glowing arrow pointing where it leads, and where to. A way
    // two cells wide is named once.
    let mut named: Vec<String> = Vec::new();
    for exit in &here.exits {
        let dir = if exit.x == 0 {
            (-1, 0)
        } else if exit.x == here.size().0 - 1 {
            (1, 0)
        } else if exit.y == 0 {
            (0, -1)
        } else {
            (0, 1)
        };
        let centre = ((exit.x as f32 + 0.5) * TILE as f32, (exit.y as f32 + 0.5) * TILE as f32);
        let home = on_screen(centre.0, centre.1).extend(1.8);
        let arrow = images.add(crate::scene::exit_arrow(dir.0, dir.1).into_image());
        commands.spawn((InLocation, ExitAt(exit.x, exit.y), ExitMarker { home, dir: Vec2::new(dir.0 as f32, -(dir.1 as f32)) }, Sprite { color: Color::linear_rgb(1.5, 1.4, 1.1), ..Sprite::from_image(arrow) }, Transform::from_translation(home)));
        let name = if exit.label.trim().is_empty() { world.place_name(&exit.to).to_string() } else { exit.label.clone() };
        let name: String = if name.chars().count() > 26 { format!("{}…", name.chars().take(25).collect::<String>()) } else { name };
        // The words sit inside the map, beside the arrow, on the side away from the edge.
        let (anchor, offset) = match dir {
            (-1, _) => (Anchor::CENTER_LEFT, Vec2::new(20.0, 0.0)),
            (1, _) => (Anchor::CENTER_RIGHT, Vec2::new(-20.0, 0.0)),
            // Near a corner the words lean away from the side they would run off.
            (_, -1) if exit.x < 8 => (Anchor::TOP_LEFT, Vec2::new(-16.0, -18.0)),
            (_, -1) if exit.x + 8 > here.size().0 => (Anchor::TOP_RIGHT, Vec2::new(16.0, -18.0)),
            (_, -1) => (Anchor::TOP_CENTER, Vec2::new(0.0, -18.0)),
            _ if exit.x < 8 => (Anchor::BOTTOM_LEFT, Vec2::new(-16.0, 18.0)),
            _ if exit.x + 8 > here.size().0 => (Anchor::BOTTOM_RIGHT, Vec2::new(16.0, 18.0)),
            _ => (Anchor::BOTTOM_CENTER, Vec2::new(0.0, 18.0)),
        };
        if named.contains(&exit.to) {
            continue;
        }
        named.push(exit.to.clone());
        // A dark plate under the words, so that they read over any ground and over each other.
        let plate = Vec2::new(name.chars().count() as f32 * 17.6 + 12.0, 30.0);
        let pad = Vec2::new(if anchor.as_vec().x < 0.0 { -6.0 } else if anchor.as_vec().x > 0.0 { 6.0 } else { 0.0 }, if anchor.as_vec().y < 0.0 { -4.0 } else if anchor.as_vec().y > 0.0 { 4.0 } else { 0.0 });
        commands.spawn((InLocation, ExitAt(exit.x, exit.y), Sprite::from_color(Color::srgba(0.06, 0.04, 0.03, 0.8), plate), anchor, Transform::from_translation((home.truncate() + offset + pad).extend(1.805))));
        for (shift, tint, z) in [(Vec2::new(2.0, -2.0), Color::srgba(0.05, 0.03, 0.02, 0.85), 1.81), (Vec2::ZERO, Color::srgb(1.0, 0.92, 0.7), 1.82)] {
            commands.spawn((InLocation, ExitAt(exit.x, exit.y), Text2d::new(name.clone()), fonts.body(), TextColor(tint), anchor, Transform::from_translation((home.truncate() + offset + shift).extend(z)).with_scale(Vec3::splat(2.0))));
        }
    }
    let mut picture = |id: &str| world.sprites.get(id).or_else(|| world.sprites.get(MARKER_SPRITE)).map(|s| images.add(crate::scene::sprite(s, &world.palette).into_image()));
    // Everything stands on the bottom edge of its cell, centred.
    let foot = |x: usize, y: usize| ((x as f32 + 0.5) * TILE as f32, (y as f32 + 1.0) * TILE as f32);
    for prop in &here.props {
        let (x, y) = foot(prop.x, prop.y);
        if let Some(image) = picture(&prop.sprite) {
            // What burns is drawn brighter than white, so that it glows; a body lies on its side, grey.
            let (glow, turned) = if prop.body.is_some() {
                (Color::srgb(0.55, 0.5, 0.5), Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
            } else if crate::scene::is_lamp(prop) {
                (Color::linear_rgb(1.9, 1.6, 1.25), Quat::IDENTITY)
            } else {
                (Color::WHITE, Quat::IDENTITY)
            };
            let at = on_screen(x, y).extend(if prop.body.is_some() { 0.9 } else { depth(y) });
            commands.spawn((InLocation, Sprite { color: glow, ..Sprite::from_image(image) }, Anchor::BOTTOM_CENTER, Transform::from_translation(at).with_rotation(turned)));
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
            let home = on_screen(x, y).extend(depth(y));
            commands
                .spawn((InLocation, Actor { id: creature.id.clone(), home, tint: Color::WHITE, bob: Some(n as f32 * 0.37) }, Sprite::from_image(image), Anchor::BOTTOM_CENTER, Transform::from_translation(home)))
                .with_children(|me| shadow_under(me, &cache.shadow));
        }
    }
    // The pictures of those who travel with the party, made before the heroes' own.
    let ally_images: Vec<Option<Handle<Image>>> = world.companions.iter().map(|c| picture(&c.creature.sprite)).collect();
    let frames_of = |index: Option<usize>, dolls: &mut Dolls, images: &mut Assets<Image>| index.and_then(|i| world.party.get(i).map(|hero| dolls.of(hero, world.held_light(i).is_some(), images)));
    match world.fight.as_ref().filter(|_| world.fight_here()) {
        // In a fight every hero stands in a cell of its own; the fallen are darkened.
        Some(fight) => {
            for c in fight.combatants.iter().filter(|c| c.side == Side::Party && !c.dead) {
                let (x, y) = foot(c.x.max(0) as usize, c.y.max(0) as usize);
                // A companion is drawn as the creature it is.
                if let Some(ally) = c.id.strip_prefix("ally_").and_then(|n| n.parse::<usize>().ok()).filter(|n| *n < world.companions.len()) {
                    if let Some(image) = ally_images[ally].clone() {
                        let tint = if c.hp > 0 { Color::WHITE } else { Color::srgb(0.35, 0.3, 0.3) };
                        let home = on_screen(x, y).extend(depth(y));
                        commands.spawn((InLocation, Actor { id: c.id.clone(), home, tint, bob: None }, Sprite { color: tint, ..Sprite::from_image(image) }, Anchor::BOTTOM_CENTER, Transform::from_translation(home))).with_children(|me| shadow_under(me, &cache.shadow));
                    }
                    continue;
                }
                let hero = c.id.strip_prefix("hero_").and_then(|n| n.parse::<usize>().ok());
                let Some(frames) = frames_of(hero, &mut dolls, &mut images) else { continue };
                let tint = if c.hp > 0 { Color::WHITE } else { Color::srgb(0.35, 0.3, 0.3) };
                let home = on_screen(x, y).extend(depth(y));
                // Facing the nearest enemy.
                let enemy = fight.combatants.iter().filter(|e| e.side != Side::Party && !e.dead).min_by_key(|e| rules::combat::distance(e.at(), c.at()));
                let facing = enemy.and_then(|e| crate::doll::Facing::of((e.x - c.x) as f32, (e.y - c.y) as f32)).unwrap_or_default();
                let image = frames[crate::doll::frame_index(facing, 0)].clone();
                commands
                    .spawn((InLocation, Actor { id: c.id.clone(), home, tint, bob: None }, Sprite { color: tint, flip_y: c.hp <= 0, ..Sprite::from_image(image) }, Anchor::BOTTOM_CENTER, Transform::from_translation(home)))
                    .with_children(|me| shadow_under(me, &cache.shadow));
            }
        }
        None => {
            // Those who travel with the heroes walk a step behind them.
            // With every hero down, the first companion carries on in the hero's place.
            let lead = (!world.party.is_empty() && world.party.iter().all(|h| h.dead || h.hp <= 0)).then(|| world.companions_here().first().copied()).flatten();
            for n in world.companions_here() {
                if Some(n) == lead {
                    continue;
                }
                let companion = &world.companions[n];
                if let Some(image) = ally_images[n].clone() {
                    commands.spawn((InLocation, Ally { owner: companion.owner.clone(), rank: world.companions.iter().take(n).filter(|c| c.owner == companion.owner).count(), id: companion.creature.id.clone() }, Sprite::from_image(image), Anchor::BOTTOM_CENTER, Transform::default())).with_children(|me| shadow_under(me, &cache.shadow));
                }
            }
            for other in world.tokens_here() {
                let owner = world.owners.iter().position(|o| *o == other.name);
                let frames = frames_of(owner, &mut dolls, &mut images).unwrap_or_default();
                let Some(first) = frames.first().cloned() else { continue };
                commands
                    .spawn((InLocation, Fellow(other.name.clone()), Frames(frames), Gait { last: Vec2::new(other.x, other.y), glide: Glide::at(Vec2::new(other.x, other.y)), ..default() }, Sprite::from_image(first), Anchor::BOTTOM_CENTER, Transform::default()))
                    .with_children(|me| shadow_under(me, &cache.shadow));
            }
            let led = lead.and_then(|n| ally_images[n].clone()).map(|image| vec![image; 4 * crate::doll::STEPS]);
            if let Some(frames) = led.or_else(|| frames_of(my_hero(world, &me, solo), &mut dolls, &mut images)) {
                let first = frames[0].clone();
                commands.spawn((InLocation, Hero, Frames(frames), Sprite::from_image(first), Anchor::BOTTOM_CENTER, Transform::default())).with_children(|me| shadow_under(me, &cache.shadow));
            }
        }
    }
}

/// What the dice said is coming, and the master never drew, is put to the master now.
fn ambush_ask(mut table: ResMut<Table>, game: Res<Game>, session: Res<Session>, settings: Res<Settings>, store: Res<Store>, gm: Res<Gm>, master: Res<Master>) {
    if table.ambush.is_none() {
        return;
    }
    let Some((comer, cell)) = table.ambush.take() else { return };
    if !session.is_client() {
        master.ask(&gm, &store, &settings, Ask::Ambush { world: Box::new(game.world.clone()), comer, cell });
    }
}

/// A companion's figure: whose it is, and which of that player's companions.
#[derive(Component)]
struct Ally {
    owner: String,
    rank: usize,
    id: String,
}

/// Words over someone's head, for a few seconds.
#[derive(Component)]
struct Bubble {
    who: String,
    age: f32,
    size: Vec2,
}

const BUBBLE_SECONDS: f32 = 6.5;

/// In a live world the people of a place talk: a handful of lines is asked for once per
/// place, of the cheap models, and said one by one. They also move about a little.
#[allow(clippy::too_many_arguments)]
fn live_world(
    time: Res<Time>,
    mut game: ResMut<Game>,
    phase: Res<Phase>,
    session: Res<Session>,
    settings: Res<Settings>,
    store: Res<Store>,
    gm: Res<Gm>,
    master: Res<Master>,
    mut table: ResMut<Table>,
    mut redraw: ResMut<Redraw>,
    mut asked: Local<std::collections::HashSet<String>>,
    mut wait: Local<(f32, f32)>,
) {
    if session.is_client() || !game.world.table.live || *phase != Phase::Walking || game.world.party.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    let place = game.world.current.clone();
    // Lines for another place are no use here.
    if table.chatter.0 != place && !table.chatter.1.is_empty() {
        table.chatter.1.clear();
    }
    let speakers = game.world.here().creatures.iter().any(|c| c.attitude != world::Attitude::Hostile) || !game.world.companions_here().is_empty();
    if speakers && table.chatter.1.is_empty() && !asked.contains(&place) && table.busy.is_none() && table.queues.idle() {
        asked.insert(place.clone());
        master.ask(&gm, &store, &settings, Ask::Chatter { world: Box::new(game.world.clone()) });
        wait.0 = 8.0;
    }
    wait.0 -= dt;
    if wait.0 <= 0.0 && table.chatter.0 == place {
        if let Some((who, text)) = table.chatter.1.pop_front() {
            session.broadcast(&Down::Bubble { place: place.clone(), who: who.clone(), text: text.clone() });
            table.bubbles.push((who, text));
            // Twenty to forty seconds to the next, by the clock's own noise.
            wait.0 = 20.0 + (time.elapsed_secs() * 7.3) % 20.0;
        }
    }
    // Now and then someone takes a step or two of their own.
    wait.1 -= dt;
    if wait.1 <= 0.0 {
        wait.1 = 7.0 + (time.elapsed_secs() * 3.1) % 6.0;
        let mut rng = Rng::from_clock();
        let world = &game.world;
        let here = world.here();
        let (px, py) = world.player.cell();
        let idle: Vec<usize> = here.creatures.iter().enumerate().filter(|(_, c)| c.attitude != world::Attitude::Hostile && c.stock.is_empty() && ((c.x as i32 - px).abs() > 1 || (c.y as i32 - py).abs() > 1)).map(|(n, _)| n).collect();
        if !idle.is_empty() {
            let n = idle[(rng.die(idle.len() as u32) - 1) as usize];
            let (cx, cy) = (here.creatures[n].x as i32, here.creatures[n].y as i32);
            let steps = [(1, 0), (-1, 0), (0, 1), (0, -1)];
            let (dx, dy) = steps[(rng.die(4) - 1) as usize];
            let (nx, ny) = (cx + dx, cy + dy);
            let free = (1..here.size().0 as i32 - 1).contains(&nx)
                && (1..here.size().1 as i32 - 1).contains(&ny)
                && !here.blocked(nx, ny, &world.tiles)
                && here.exit_at(nx as usize, ny as usize).is_none()
                && (nx, ny) != (px, py);
            if free {
                let place = world.current.clone();
                if let Some(c) = game.world.locations.get_mut(&place).and_then(|l| l.creatures.get_mut(n)) {
                    (c.x, c.y) = (nx as usize, ny as usize);
                    redraw.0 = true;
                }
            }
        }
    }
}

/// Puts up the bubbles asked for, keeps each over the head of whoever speaks, and takes
/// them down in time. A click on one turns to its speaker.
#[allow(clippy::too_many_arguments)]
fn bubbles(
    mut commands: Commands,
    time: Res<Time>,
    game: Res<Game>,
    fonts: Res<Fonts>,
    mut table: ResMut<Table>,
    mut phase: ResMut<Phase>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    frame: Res<crate::view::View>,
    actors: Query<(&Actor, &Transform), Without<Bubble>>,
    allies: Query<(&Ally, &Transform), Without<Bubble>>,
    mut shown: Query<(Entity, &mut Bubble, &mut Transform)>,
) {
    let head = |who: &str| actors.iter().find(|(a, _)| a.id == who).map(|(_, t)| t.translation).or_else(|| allies.iter().find(|(a, _)| a.id == who).map(|(_, t)| t.translation));
    for (who, text) in std::mem::take(&mut table.bubbles) {
        let Some(at) = head(&who) else { continue };
        // One bubble a speaker.
        for (entity, bubble, _) in &shown {
            if bubble.who == who {
                commands.entity(entity).try_despawn();
            }
        }
        // The cloud is as wide as its longest line, up to a width that reads well.
        // Letters are drawn twice the size of the picture's pixels; words wrap whole, so a
        // line holds a little less than its width in letters.
        let letters = text.chars().count() as f32;
        let width = (letters * 13.0 + 18.0).clamp(60.0, 236.0);
        let per_row = ((width - 18.0) / 13.0).floor().max(4.0);
        let rows = if letters <= per_row { 1.0 } else { (letters / (per_row - 3.0)).ceil() };
        let size = Vec2::new(width, rows * 20.0 + 10.0);
        commands
            .spawn((InLocation, Bubble { who, age: 0.0, size }, Sprite { color: color(PARCHMENT), custom_size: Some(size), ..default() }, Transform::from_translation(Vec3::new(at.x, at.y + 46.0 + size.y / 2.0, 1.88))))
            .with_children(|cloud| {
                // An edge of ink, and a tail towards the speaker.
                cloud.spawn((Sprite { color: color(INK), custom_size: Some(size + Vec2::splat(2.0)), ..default() }, Transform::from_xyz(0.0, 0.0, -0.01)));
                cloud.spawn((Sprite { color: color(PARCHMENT), custom_size: Some(Vec2::new(6.0, 6.0)), ..default() }, Transform::from_xyz(0.0, -size.y / 2.0 - 2.0, 0.0005)));
                cloud.spawn((Text2d::new(text), fonts.body(), TextColor(color(INK)), TextLayout::justify(Justify::Center), bevy::text::TextBounds::new_horizontal((width - 16.0) / 2.0), Transform::from_xyz(0.0, 0.0, 0.001).with_scale(Vec3::splat(2.0))));
            });
    }
    let clicked = mouse.just_pressed(MouseButton::Left).then(|| crate::view::map_under_cursor(&window, &frame)).flatten().map(|p| Vec2::new(p.x - CANVAS.x as f32 / 2.0, CANVAS.y as f32 / 2.0 - p.y));
    for (entity, mut bubble, mut transform) in &mut shown {
        bubble.age += time.delta_secs();
        if bubble.age > BUBBLE_SECONDS {
            commands.entity(entity).try_despawn();
            continue;
        }
        if let Some(at) = head(&bubble.who) {
            let wanted = Vec3::new(at.x.round(), (at.y + 46.0 + bubble.size.y / 2.0).round(), 1.88);
            if transform.translation != wanted {
                transform.translation = wanted;
            }
        }
        // A click on the words turns to whoever said them.
        if let Some(point) = clicked
            && *phase == Phase::Walking
            && (point - transform.translation.truncate()).abs().cmple(bubble.size / 2.0).all()
            && game.world.here().creatures.iter().any(|c| c.id == bubble.who)
        {
            *phase = Phase::Writing { near: Some(bubble.who.clone()) };
        }
    }
}

/// Companions keep a step behind the hero they follow, each in a place of its own.
fn place_allies(time: Res<Time>, game: Res<Game>, settings: Res<Settings>, mut allies: Query<(&Ally, &mut Transform, &mut Sprite)>) {
    let world = &game.world;
    let me = player_name(&settings);
    for (ally, mut transform, mut sprite) in &mut allies {
        // The one they follow: this player's own hero, or another player's token.
        let leader = if ally.owner.is_empty() || ally.owner == me { Some((world.player.x, world.player.y)) } else { world.tokens_here().find(|t| t.name == ally.owner).map(|t| (t.x, t.y)) };
        let Some((lx, ly)) = leader else { continue };
        let back = 14.0 + ally.rank as f32 * 11.0;
        let side = if ally.rank % 2 == 0 { 5.0 } else { -3.0 };
        let feet = ly + FEET.1 + side;
        let goal = on_screen(lx - back, feet);
        let wanted = Vec3::new(goal.x, goal.y, depth(feet));
        let now = transform.translation;
        // Just appeared, or left far behind: beside the leader at once.
        let next = if now == Vec3::ZERO || now.truncate().distance(wanted.truncate()) > 160.0 { wanted } else { now.truncate().lerp(wanted.truncate(), 1.0 - (-time.delta_secs() * 5.0).exp()).extend(wanted.z) };
        if (next.x - now.x).abs() > 0.3 {
            sprite.flip_x = next.x < now.x;
        }
        if transform.translation != next {
            transform.translation = next;
        }
    }
}

/// Eight steps a second.
const STEPS_PER_SECOND: f32 = 8.0;

fn place_hero(game: Res<Game>, pose: Res<HeroPose>, mut hero: Query<(&mut Transform, &mut Sprite, &Frames), With<Hero>>) {
    let player = game.world.player;
    // The sprite's feet sit at the bottom of the patch of ground the party takes up.
    let feet_y = player.y + FEET.1;
    let step = if pose.moving { (pose.clock * STEPS_PER_SECOND) as usize % crate::doll::STEPS } else { 0 };
    for (mut transform, mut sprite, frames) in &mut hero {
        let at = on_screen(player.x, feet_y).round();
        let wanted = Vec3::new(at.x, at.y, depth(feet_y));
        if transform.translation != wanted {
            transform.translation = wanted;
        }
        if let Some(image) = frames.0.get(crate::doll::frame_index(pose.facing, step))
            && sprite.image != *image
        {
            sprite.image = image.clone();
        }
    }
}

/// Other players' tokens follow where their players last said they were, walking as they go.
/// When somebody joins or leaves, the picture is drawn again so that there is a token for each.
fn place_fellows(time: Res<Time>, game: Res<Game>, mut redraw: ResMut<Redraw>, mut fellows: Query<(&Fellow, &Frames, &mut Gait, &mut Transform, &mut Sprite)>, mut was_fighting: Local<bool>) {
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
    for (fellow, frames, mut gait, mut transform, mut sprite) in &mut fellows {
        let Some(token) = others.iter().find(|t| t.name == fellow.0) else { continue };
        // Drawn on the way to where the player was last heard to be, not set down there.
        let now = gait.glide.step(Vec2::new(token.x, token.y), time.delta_secs());
        let moved = now - gait.last;
        if moved.length() > 0.05 {
            if let Some(facing) = crate::doll::Facing::of(moved.x, moved.y) {
                gait.facing = facing;
            }
            gait.clock += time.delta_secs();
            gait.still = 0.0;
        } else {
            gait.still += time.delta_secs();
        }
        gait.last = now;
        let step = if gait.still < 0.15 { (gait.clock * STEPS_PER_SECOND) as usize % crate::doll::STEPS } else { 0 };
        let feet_y = now.y + FEET.1;
        let at = on_screen(now.x, feet_y).round();
        transform.translation = Vec3::new(at.x, at.y, depth(feet_y));
        if let Some(image) = frames.0.get(crate::doll::frame_index(gait.facing, step))
            && sprite.image != *image
        {
            sprite.image = image.clone();
        }
    }
}

/// The grid of where one may walk shows while Alt is held, and fades in a new place.
fn walk_grid(time: Res<Time>, keyboard: Res<ButtonInput<KeyCode>>, phase: Res<Phase>, settings: Res<Settings>, mut cache: ResMut<SceneCache>, mut grids: Query<&mut Sprite, With<WalkGrid>>) {
    cache.age += time.delta_secs();
    let held = keyboard.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]) && matches!(*phase, Phase::Walking | Phase::Fighting);
    let fresh = if *phase == Phase::Walking && settings.gfx.cell_hint { (1.0 - (cache.age - 3.0) / 1.5).clamp(0.0, 1.0) } else { 0.0 };
    // In steps, like every other fade.
    let alpha = ((if held { 1.0 } else { fresh }) * 4.0).round() / 4.0;
    for mut sprite in &mut grids {
        if sprite.color.alpha() != alpha {
            sprite.color = Color::srgba(1.0, 1.0, 1.0, alpha);
        }
    }
}

/// The water moves, a few beats a second.
fn water(time: Res<Time>, cache: Res<SceneCache>, mut layers: Query<&mut Sprite, With<GroundLayer>>, mut since: Local<f32>, mut beat: Local<usize>) {
    if cache.ground.len() < 2 {
        return;
    }
    *since += time.delta_secs();
    if *since < 0.3 {
        return;
    }
    *since = 0.0;
    *beat = (*beat + 1) % cache.ground.len();
    for mut layer in &mut layers {
        layer.image = cache.ground[*beat].clone();
    }
}

/// The dark is redrawn as the lamps waver and the heroes, who carry a light of their own, walk.
fn lighting(time: Res<Time>, mut game: ResMut<Game>, settings: Res<Settings>, cache: Res<SceneCache>, mut images: ResMut<Assets<Image>>) {
    let foggy = cache.sky == crate::weather::Sky::Fog && settings.gfx.weather;
    let Some(hour) = cache.hour.filter(|h| (h.dark > 0.0 && settings.gfx.night) || foggy) else { return };
    // Every frame, so the light moves with the hero smoothly; the flames waver eight times a second.
    let beat = (time.elapsed_secs() * 8.0) as u32;
    // Underground, and in fog, what the party has never come near stays unseen.
    let exploring = hour.dark >= 0.8 || cache.sky == crate::weather::Sky::Fog;
    if exploring {
        let world = &mut game.world;
        let mut eyes: Vec<(f32, f32)> = vec![(world.player.x + world::walk::FEET.0, world.player.y + world::walk::FEET.1)];
        eyes.extend(world.tokens_here().map(|t| (t.x + world::walk::FEET.0, t.y + world::walk::FEET.1)));
        let lamps: Vec<(f32, f32, f32)> = cache.lamps.iter().map(|l| (l.x, l.y, l.reach)).collect();
        let tile = TILE as f32;
        let here = world.current.clone();
        if let Some(place) = world.locations.get_mut(&here) {
            // What burns is seen from afar.
            for (x, y, reach) in lamps {
                place.reveal(x / tile, y / tile, reach * 0.8);
            }
            for (x, y) in eyes {
                place.reveal(x / tile, y / tile, 3.2);
            }
        }
    }
    let game = &*game;
    let mut lights = cache.lamps.clone();
    let carried: f32 = if hour.dark >= 0.5 { 3.4 } else { 2.4 };
    let world = &game.world;
    // A torch in hand lights much farther than eyes used to the dark: one's own, and each
    // fellow's by their token.
    let torch = |player: &str| world.owners.iter().position(|o| o == player).or_else(|| world.owners.is_empty().then_some(0)).and_then(|i| world.held_light(i)).map_or(0.0, |l| l.light as f32 + 1.0);
    let me = player_name(&settings);
    lights.push(crate::scene::Lamp { x: world.player.x, y: world.player.y, reach: carried.max(torch(&me)) });
    for lying in world.here().items.iter().filter(|l| l.item.light > 0) {
        lights.push(crate::scene::Lamp { x: (lying.x as f32 + 0.5) * TILE as f32, y: (lying.y as f32 + 0.5) * TILE as f32, reach: lying.item.light as f32 });
    }
    for token in world.tokens_here() {
        lights.push(crate::scene::Lamp { x: token.x, y: token.y, reach: carried.max(torch(&token.name)) });
    }
    if let Some(fight) = world.fight.as_ref().filter(|_| world.fight_here()) {
        let tile = TILE as f32;
        for c in fight.combatants.iter().filter(|c| c.side == Side::Party && !c.dead) {
            lights.push(crate::scene::Lamp { x: (c.x as f32 + 0.5) * tile, y: (c.y as f32 + 0.5) * tile, reach: carried });
        }
    }
    if let Some(mut image) = images.get_mut(&cache.dark) {
        let seen = exploring.then(|| world.here().seen.as_slice());
        // Fog hides in grey what darkness hides in black.
        let hour = if foggy && hour.dark < 0.8 { crate::scene::Hour { dark: if settings.gfx.night { hour.dark } else { 0.0 }, tint: [150, 152, 162] } } else { hour };
        *image = crate::scene::darkness_seen(world.here().size(), hour, &lights, beat, seen, foggy.then(|| time.elapsed_secs())).into_image();
    }
}

/// Sounds and sights asked for by code that could not ask the effects itself.
fn cues(mut background: ResMut<Background>, mut fx: ResMut<Fx>, game: Res<Game>) {
    if background.cues.is_empty() && background.die.is_none() && !background.level_up {
        return;
    }
    fx.cues.append(&mut background.cues);
    if let Some(value) = background.die.take() {
        fx.throw(value);
        fx.cues.push(Sfx::Dice);
    }
    if std::mem::take(&mut background.level_up) {
        let player = game.world.player;
        let at = on_screen(player.x, player.y);
        fx.burst(at, Color::srgb(1.0, 0.85, 0.35), 18);
        fx.float(at + Vec2::Y * 10.0, "★", Color::srgb(1.0, 0.9, 0.45));
        fx.cues.push(Sfx::LevelUp);
    }
}

#[allow(clippy::too_many_arguments)]
/// A way out the player asked to take with E.
#[derive(Resource, Default)]
struct Leave(Option<String>);

/// Something on the ground the player asked to pick up with E.
#[derive(Resource, Default)]
struct Pick(Option<usize>);

/// Picks up what was asked for, and sends to the master what a use of a thing put into words.
#[allow(clippy::too_many_arguments)]
fn bag_work(
    mut pick: ResMut<Pick>,
    mut said: ResMut<crate::bag_ui::Said>,
    mut game: ResMut<Game>,
    mut phase: ResMut<Phase>,
    mut redraw: ResMut<Redraw>,
    session: Res<Session>,
    settings: Res<Settings>,
    store: Res<Store>,
    gm: Res<Gm>,
    master: Res<Master>,
    table: Res<Table>,
) {
    let me = player_name(&settings);
    let hero = my_hero(&game.world, &me, matches!(*session, Session::Solo)).unwrap_or(0);
    if let Some(n) = pick.0.take() {
        if session.is_client() {
            session.intend(Intent::PickUp { n });
        }
        if game.world.pick_up(hero, n) {
            redraw.0 = true;
        }
    }
    if *phase == Phase::Walking
        && let Some(words) = said.0.take()
    {
        *phase = send(words, None, &game, &gm, &store, &settings, &master, &session, &table);
    }
}

#[allow(clippy::too_many_arguments)]
fn walk(time: Res<Time>, keyboard: Res<ButtonInput<KeyCode>>, gamepads: Query<&Gamepad>, mut game: ResMut<Game>, mut phase: ResMut<Phase>, mut pose: ResMut<HeroPose>, mut warp: ResMut<Warp>, session: Res<Session>, mut table: ResMut<Table>, mut leave: ResMut<Leave>, mut redraw: ResMut<Redraw>) {
    // Orders from a test script, whatever the screen shows.
    for order in std::mem::take(&mut warp.1) {
        redraw.0 = true;
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
            // `fx=[…]`: effects as the master would write them, carried out at once. A `|`
            // stands for a comma, which the script keeps for itself.
            "fx" => {
                let text = order.split_once('=').map_or("", |(_, json)| json).replace('|', ",");
                match serde_json::from_str::<Vec<serde_json::Value>>(&text) {
                    Ok(raw) => {
                        let mut notes = Vec::new();
                        let effects = world::story::read_effects(&raw, &mut notes);
                        notes.extend(world.apply(&effects, None));
                        for note in notes {
                            info!("dev fx: {note}");
                        }
                    }
                    Err(e) => info!("dev fx: not read: {e}"),
                }
            }
            // `danger=N`: the place becomes dangerous, with rats to wander in and a find.
            "danger" => {
                let here = world.current.clone();
                if let Some(place) = world.locations.get_mut(&here) {
                    // Above ten, the wanderer is one the master drew; otherwise it never was.
                    let drawn = amount > 10;
                    place.danger = (amount % 10).clamp(0, 5) as u8;
                    place.wanderers = vec![world::Wanderer { monster: "giant-rat".into(), name: "Camp rat".into(), sprite: if drawn { "cat".into() } else { String::new() }, description: "Lean and bold, used to stealing from travellers.".into() }];
                    place.finds = vec!["A peddler passes, humming.".to_string()];
                }
            }
            // `wander`: hours go by until something wanders in, at most two days of them.
            "wander" => {
                let mut rng = Rng::from_clock();
                for _ in 0..48 {
                    world.pass(60);
                    if let Some(what) = world.wander(&mut rng) {
                        info!("dev wander: {what:?}");
                        match what {
                            world::living::Wandered::Find(text) | world::living::Wandered::Came(text) => table.unread.push_back(Turn { narration: text, options: Vec::new() }),
                            world::living::Wandered::Ambush(comer, cell) => table.ambush = Some((comer, cell)),
                        }
                        break;
                    }
                }
            }
            // `join`: whoever is here first joins the party, so companions can be tested.
            "join" => {
                if let Some(id) = world.here().creatures.first().map(|c| c.id.clone()) {
                    let _ = world.join(&id, "");
                }
            }
            // `fight`: everything alive here turns on the party, so a fight can be tested.
            "fight" => {
                let here = world.current.clone();
                if let Some(place) = world.locations.get_mut(&here) {
                    place.creatures.iter_mut().for_each(|c| c.attitude = world::Attitude::Hostile);
                }
                world.start_fight(Rng::from_clock());
            }
            // `lose`: every hero in the fight falls, so that a defeat can be tested.
            "lose" => {
                if let Some(fight) = world.fight.as_mut() {
                    for c in fight.combatants.iter_mut().filter(|c| c.side == Side::Party) {
                        c.hp = 0;
                        c.stable = true;
                    }
                }
            }
            // `win`: every enemy in the fight falls, so that its end can be tested.
            "win" => {
                if let Some(fight) = world.fight.as_mut() {
                    for c in fight.combatants.iter_mut().filter(|c| c.side != Side::Party) {
                        c.hp = 0;
                        c.dead = true;
                    }
                }
            }
            // `give=Name` puts a thing in the bag, the way the master would.
            "give" => {
                let name = order.split_once('=').map_or("", |(_, n)| n).to_string();
                // A potion given by a test heals, so that drinking can be tested.
                let potion = name.to_lowercase().contains("potion");
                let traits = world::story::ItemTraits { kind: if potion { "potion".into() } else { String::new() }, heal: if potion { "2d4+2".into() } else { String::new() }, ..Default::default() };
                world.apply(&[world::Effect::Give { name, description: "Given by a test.".into(), to: String::new(), traits }], None);
            }
            _ => {}
        }
    }
    // With the bag open the hero still walks, as in the old games; and while a telling is
    // read or the master is answering, nobody has to stand still for it.
    if !matches!(*phase, Phase::Walking | Phase::Bag | Phase::Reading { .. } | Phase::Asking { .. }) {
        pose.moving = false;
        return;
    }
    // The bag looked into in the middle of a fight: nobody walks, and the fight waits.
    if *phase == Phase::Bag && game.world.fight_here() {
        pose.moving = false;
        return;
    }
    // Something for everyone to read comes before walking on, once nothing else is open.
    if matches!(*phase, Phase::Walking | Phase::Asking { .. })
        && let Some(turn) = table.unread.pop_front()
    {
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
    // A way out is taken on purpose: E beside it, or a step off the map where it leads.
    let wished = std::mem::take(&mut leave.0);
    let pushing = {
        let held = |keys: [KeyCode; 2]| keyboard.any_pressed(keys) as i32;
        (held([KeyCode::KeyD, KeyCode::ArrowRight]) - held([KeyCode::KeyA, KeyCode::ArrowLeft]), held([KeyCode::KeyS, KeyCode::ArrowDown]) - held([KeyCode::KeyW, KeyCode::ArrowUp]))
    };
    let stepping_out = game
        .world
        .exit_here()
        .filter(|e| (e.x == 0 && pushing.0 < 0) || (e.x == game.world.here().size().0 - 1 && pushing.0 > 0) || (e.y == 0 && pushing.1 < 0) || (e.y == game.world.here().size().1 - 1 && pushing.1 > 0))
        .map(|e| e.to.clone());
    let taken = stepping_out.or(wished);
    if session.is_client() {
        if let Some(to) = taken.clone() {
            session.intend(Intent::Exit { to: to.clone() });
            // The host moves the party; until its world arrives the guest steps back, so
            // that a refusal does not leave it standing in the doorway asking again.
            game.world.step_back();
            let _ = to;
            return;
        }
    }
    if let Some((x, y)) = warp.0.take() {
        let (w, h) = game.world.here().size();
        game.world.player = world::Player::in_cell(x.min(w - 1), y.min(h - 1));
    }
    if !session.is_client()
        && let Some(to) = taken
    {
        *phase = Phase::Leaving { to, t: 0.0 };
        return;
    }
    // Something hostile close by does not wait to be spoken to. With guests at the table
    // any of their tokens can be the one that is noticed.
    // A pack that has not been counted out yet is, now that the party is here.
    if !session.is_client() && game.world.fight.is_none() && game.world.here().creatures.iter().any(|c| (c.attitude == world::Attitude::Hostile || c.count > 1) && !c.mustered) {
        game.world.muster();
    }
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
    if let Some(facing) = crate::doll::Facing::of(step.x, step.y) {
        pose.facing = facing;
    }
    if pose.moving {
        pose.clock += time.delta_secs();
    }
}

/// Keys that open something while walking: interact, the journal, the pause menu. Escape
/// also closes whatever sheet is open.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn keys(keyboard: Res<ButtonInput<KeyCode>>, gamepads: Query<&Gamepad>, game: Res<Game>, mut phase: ResMut<Phase>, mut was_walking: Local<bool>, mut table: ResMut<Table>, settings: Res<Settings>, session: Res<Session>, store: Res<Store>, mut leave: ResMut<Leave>, mut pick: ResMut<Pick>, mut use_for: ResMut<UseFor>) {
    let pad = |button: GamepadButton| gamepads.iter().any(|pad| pad.just_pressed(button));
    // A thing whose use was being written is forgotten once the field is gone.
    if use_for.0.is_some() && !matches!(*phase, Phase::Writing { .. } | Phase::Bag) {
        use_for.0 = None;
    }
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
            Phase::Paused | Phase::Looking { .. } | Phase::Writing { .. } | Phase::Journal | Phase::Sheet | Phase::Bag | Phase::Judging | Phase::Master => Phase::Walking,
            // A telling with choices stays until one is made or it is closed by its button.
            // Escape closes a telling as "Go on" does: its choices stay where they were offered.
            Phase::Reading { .. } => settled(&game.world),
            _ => return,
        };
        *phase = next;
        return;
    }
    if *phase == Phase::Bag && (keyboard.just_pressed(KeyCode::KeyI) || keyboard.just_pressed(KeyCode::Tab)) {
        *phase = Phase::Walking;
        return;
    }
    // In a fight the bag can still be looked into, and a thing drawn or put away.
    if *phase == Phase::Fighting && (keyboard.just_pressed(KeyCode::KeyI) || keyboard.just_pressed(KeyCode::Tab)) {
        *phase = Phase::Bag;
        return;
    }
    if *phase != Phase::Walking || !was_settled {
        return;
    }
    if keyboard.any_just_pressed([KeyCode::KeyE, KeyCode::Enter, KeyCode::NumpadEnter]) || pad(GamepadButton::South) {
        // Something lying at the party's feet is picked up first.
        if let Some(n) = game.world.lying_near() {
            pick.0 = Some(n);
            return;
        }
        // A thing to hand comes first, then a way out close by, then the party's own words.
        match (game.world.thing_near(THING_REACH), game.world.exit_near(EXIT_HINT_REACH)) {
            (Some(thing), _) => *phase = Phase::Looking { thing: thing.to_string() },
            (None, Some(exit)) => leave.0 = Some(exit.to.clone()),
            (None, None) => *phase = Phase::Writing { near: None },
        }
    } else if keyboard.just_pressed(KeyCode::KeyJ) || pad(GamepadButton::North) {
        *phase = Phase::Journal;
    } else if keyboard.just_pressed(KeyCode::KeyI) || keyboard.just_pressed(KeyCode::Tab) {
        *phase = Phase::Bag;
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
    // It does not take the place of a thing being looked at, a die about to be rolled or
    // words being typed: those wait, and it is read after them.
    match phase {
        Phase::Walking | Phase::Asking { .. } => *phase = Phase::Reading { turn },
        // Read on from the same panel: the new part follows what is being read, and its
        // choices take the place of the old ones.
        Phase::Reading { turn: reading } => {
            reading.narration = format!("{}\n\n{}", reading.narration.trim_end(), turn.narration.trim());
            if !turn.options.is_empty() {
                reading.options = turn.options;
            }
        }
        // Something else is open (the bag, a sheet, the journal, a fight): the telling waits
        // for it to close, and the corner says there is one.
        _ => {
            table.notices.push(format!("✉ {}", gist(&turn.narration)));
            table.unread.push_back(turn);
        }
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

/// The name of a thing here, when the party stands too far from it to deal with it.
fn far_from(world: &World, id: &str) -> Option<String> {
    let here = world.here();
    let (cell, name) = here.props.iter().find(|p| p.id == id).map(|p| ((p.x, p.y), p.name.clone())).or_else(|| here.creatures.iter().find(|c| c.id == id).map(|c| ((c.x, c.y), c.name.clone())))?;
    let centre = ((cell.0 as f32 + 0.5) * TILE as f32, (cell.1 as f32 + 0.5) * TILE as f32);
    let (px, py) = (world.player.x + world::walk::FEET.0, world.player.y + world::walk::FEET.1);
    let distance = ((centre.0 - px).powi(2) + (centre.1 - py).powi(2)).sqrt();
    (distance > THING_REACH * 1.6).then_some(name)
}

/// Which way the hero on this screen looks goes with the world, every frame and whatever
/// sheet is open: the master is told what is in front, and a guest's host is told it too.
fn keep_facing(mut game: ResMut<Game>, pose: Res<HeroPose>) {
    let facing = match pose.facing {
        crate::doll::Facing::Up => (0, -1),
        crate::doll::Facing::Down => (0, 1),
        crate::doll::Facing::Left => (-1, 0),
        crate::doll::Facing::Right => (1, 0),
    };
    if game.world.facing != facing {
        game.bypass_change_detection().world.facing = facing;
    }
}

/// A thing from the bag the player is about to say the use of: "X uses Y", put before what
/// is typed.
#[derive(Resource, Default)]
pub struct UseFor(pub Option<String>);

/// What goes to the master: what was typed, after the thing it is about, if any.
fn with_use(use_for: &UseFor, typed: String) -> String {
    match &use_for.0 {
        Some(thing) if !typed.trim().is_empty() => format!("{thing}: {}", typed.trim()),
        _ => typed,
    }
}

/// A walk the hero is taking to do what a choice needs.
pub struct Walking {
    path: Vec<(usize, usize)>,
    option: world::Action,
    index: usize,
    turn: Turn,
}

/// The walk under way, if any.
#[derive(Resource, Default)]
pub struct Approach(Option<Walking>);

/// The cells to walk to stand on a cell here, from where the hero is.
pub fn way_to_cell(world: &World, goal: (usize, usize)) -> Option<Vec<(usize, usize)>> {
    let here = world.here();
    let (px, py) = world.player.cell();
    let (w, h) = here.size();
    let start = (px.clamp(0, w as i32 - 1) as usize, py.clamp(0, h as i32 - 1) as usize);
    world::path::route(start, |cell| cell == goal, |(x, y)| (x, y) != goal && (here.blocked(x as i32, y as i32, &world.tiles) || here.exit_at(x, y).is_some()))
}

/// The story walks the hero along the cells it was given, as a hero walks to a thing; a key
/// pressed to walk elsewhere lets go of it.
fn led(time: Res<Time>, keyboard: Res<ButtonInput<KeyCode>>, mut table: ResMut<Table>, mut game: ResMut<Game>, mut pose: ResMut<HeroPose>) {
    if table.led.is_empty() {
        return;
    }
    let steering = [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD, KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::ArrowLeft, KeyCode::ArrowRight];
    if keyboard.any_pressed(steering) {
        table.led.clear();
        return;
    }
    let (cx, cy) = table.led[0];
    let goal = world::Player::in_cell(cx, cy);
    let world = &mut game.world;
    let way = Vec2::new(goal.x - world.player.x, goal.y - world.player.y);
    if way.length() < 1.0 {
        world.player = goal;
        table.led.remove(0);
        pose.moving = !table.led.is_empty();
        return;
    }
    let step = way.clamp_length_max(SPEED * time.delta_secs());
    if let Some(facing) = crate::doll::Facing::of(step.x, step.y) {
        pose.facing = facing;
    }
    if !world.walk(step.x, step.y) {
        world.player = goal;
    }
    pose.moving = true;
    pose.clock += time.delta_secs();
}

/// The cells to walk to stand beside a thing here.
fn way_to(world: &World, id: &str) -> Option<Vec<(usize, usize)>> {
    let here = world.here();
    let target = here.props.iter().find(|p| p.id == id).map(|p| (p.x, p.y)).or_else(|| here.creatures.iter().find(|c| c.id == id).map(|c| (c.x, c.y)))?;
    let (px, py) = world.player.cell();
    let (w, h) = here.size();
    let start = (px.clamp(0, w as i32 - 1) as usize, py.clamp(0, h as i32 - 1) as usize);
    let beside = |(x, y): (usize, usize)| x.abs_diff(target.0) + y.abs_diff(target.1) == 1;
    world::path::route(start, beside, |(x, y)| here.blocked(x as i32, y as i32, &world.tiles) || here.exit_at(x, y).is_some())
}

/// Walks the hero along the way to what a choice needs, and takes the choice on arriving. A
/// key pressed to walk elsewhere calls the walk off.
#[allow(clippy::too_many_arguments)]
fn approach_work(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut approach: ResMut<Approach>,
    mut game: ResMut<Game>,
    mut phase: ResMut<Phase>,
    mut pose: ResMut<HeroPose>,
    mut redraw: ResMut<Redraw>,
    mut table: ResMut<Table>,
    session: Res<Session>,
    settings: Res<Settings>,
    store: Res<Store>,
) {
    if approach.0.is_none() || *phase != Phase::Walking {
        return;
    }
    let steering = [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD, KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::ArrowLeft, KeyCode::ArrowRight];
    if keyboard.any_pressed(steering) {
        approach.0 = None;
        return;
    }
    let Some(walking) = approach.0.as_mut() else { return };
    if let Some(&(cx, cy)) = walking.path.first() {
        let goal = world::Player::in_cell(cx, cy);
        let world = &mut game.world;
        let way = Vec2::new(goal.x - world.player.x, goal.y - world.player.y);
        if way.length() < 1.0 {
            world.player = goal;
            walking.path.remove(0);
            return;
        }
        let step = way.clamp_length_max(SPEED * time.delta_secs());
        if let Some(facing) = crate::doll::Facing::of(step.x, step.y) {
            pose.facing = facing;
        }
        // Stuck on a corner, the hero is set down at the next cell.
        if !world.walk(step.x, step.y) {
            world.player = goal;
        }
        pose.moving = true;
        pose.clock += time.delta_secs();
        return;
    }
    // Arrived: the choice is taken as if it had been clicked beside the thing.
    let Some(Walking { option, index, turn, .. }) = approach.0.take() else { return };
    pose.moving = false;
    if session.is_client() {
        session.intend(Intent::Choose { index, label: option.label.clone() });
        *phase = Phase::Asking { said: option.label.clone(), near: None };
        return;
    }
    let _ = turn;
    let outcome = game.world.choose(&option);
    redraw.0 = true;
    autosave(&store, &game);
    if let Some(offered) = table.options.get_mut(&game.world.current) {
        offered.retain(|o| o.label != option.label);
    }
    table.going_on = Some(outcome.clone());
    *phase = follow(&option.label, &outcome, &game, &settings, &session, &table);
}

/// What came of going at a container, in words.
fn chest_words(result: &world::bag::Opened, lang: Lang) -> String {
    match result {
        world::bag::Opened::Took(names) if names.is_empty() => T::ChestEmpty.text(lang).to_string(),
        world::bag::Opened::Took(names) => format!("{} {}", T::ChestTook.text(lang), names.join(", ")),
        world::bag::Opened::Locked => T::ChestLocked.text(lang).to_string(),
        world::bag::Opened::Failed { roll, total, needed } => format!("{} d20 {roll} → {total} / {needed}", T::ChestFailed.text(lang)),
    }
}

/// The first sentence or so of a telling, for players who are somewhere else.
fn gist(narration: &str) -> String {
    let text = narration.trim();
    let end = text.char_indices().find(|(i, c)| matches!(c, '.' | '!' | '?' | '…') && *i > 20).map_or(text.len(), |(i, c)| i + c.len_utf8());
    let first: String = text[..end].chars().take(160).collect();
    if first.chars().count() < text[..end].chars().count() { format!("{first}…") } else { first }
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
        background.level_up = true;
    }
    redraw.0 = true;
    background.quiet = 0.0;
    autosave(store, game);
    // Everyone in the place reads what the master tells, and may take up the choices it
    // offers. A party that has split up is told scene by scene.
    let place = game.world.current.clone();
    // Those elsewhere read a line of it in the chat, so everyone knows what the others are up to.
    let gist = gist(&turn.narration);
    let where_ = game.world.place_name(&place).to_string();
    for player in session.players() {
        if game.world.place_of(&player) == place {
            session.send_to(&player, &Down::Told(turn.clone()));
        } else if !gist.is_empty() {
            session.send_to(&player, &Down::News { place: where_.clone(), text: gist.clone() });
        }
    }
    // News from far off goes to the corner, not to the table's talk.
    if table.elsewhere && !gist.is_empty() {
        table.notices.push(format!("{where_}: {gist}"));
    }
    // The outcome of a chest opened or a die rolled offers nothing, and takes nothing
    // away: the choices of the telling before it stand.
    if !turn.options.is_empty() {
        table.options.insert(place, turn.options.clone());
    }
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
                } else if waited_for && id == game.world.current {
                    // The place the party stands in, drawn afresh: it takes the old one's
                    // place, and the party stands where it would have walked in.
                    match draft.and_then(|d| game.world.add_location(&d, &id, Some(&from)).map_err(Trouble::Unreadable)) {
                        Ok(_) => {
                            let (x, y) = game.world.arrival_cell(&id, &from);
                            game.world.player = world::Player::in_cell(x, y);
                            redraw.0 = true;
                            autosave(&store, &game);
                            *phase = Phase::Walking;
                        }
                        Err(trouble) => {
                            info!("place '{id}' was not drawn again: {trouble:?}");
                            *phase = Phase::Refused { trouble, said: String::new(), near: None };
                        }
                    }
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
            Answer::Turn { id, said, near, draft } if id != 0 => {
                // An answer to a question of the queues. One nobody waits for is thrown away.
                let now = clock_ms();
                let Some(flight) = table.queues.answered(table::RequestId(id), now) else { continue };
                let me = player_name(&settings);
                let solo = matches!(*session, Session::Solo);
                let place_now = |world: &World, player: &str| if player == me || solo { world.current.clone() } else { world.place_of(player).to_string() };
                if solo || flight.acts.iter().any(|a| a.player == me) {
                    table.going_on = None;
                }
                match draft {
                    Err(trouble) => {
                        let why = trouble_text(&trouble, gm.dispatcher.now(), settings.lang);
                        table.master_note = why.clone();
                        for act in &flight.acts {
                            // The host's own, while they still wait on it, keeps its sheet
                            // with "ask again"; everyone else is told in words.
                            if (act.player == me || solo) && matches!(*phase, Phase::Asking { .. }) {
                                *phase = Phase::Refused { said: act.said.clone(), near: act.near.clone(), trouble: trouble.clone() };
                            } else {
                                tell_refusal(&act.player, &why, &me, &session, &mut phase, &mut table);
                            }
                        }
                    }
                    Ok(draft) => {
                        // Those who walked on meanwhile are not in the scene any more.
                        let (here, gone): (Vec<table::Act>, Vec<table::Act>) = flight.acts.iter().cloned().partition(|a| place_now(&game.world, &a.player) == flight.place);
                        for act in &gone {
                            tell_refusal(&act.player, T::ActVoidLeft.text(settings.lang), &me, &session, &mut phase, &mut table);
                        }
                        if here.is_empty() {
                            continue;
                        }
                        // It happens where they are: the host's own place, or a guest's.
                        let scene = if solo || here.iter().any(|a| a.player == me) { None } else { here.first().map(|a| a.player.clone()) };
                        let mut desk = Desk { game: &mut game, table: &mut table, phase: &mut phase, redraw: &mut redraw, background: &mut background, session: &session, settings: &settings, store: &store, gm: &gm, master: &master };
                        if seated(&desk.game.world).is_some() {
                            // A human master reads the model's answer before the players do.
                            desk.table.drafts.push_back(Held { said, near, draft: *draft, scene });
                            if desk.table.drafts.len() == 1 {
                                offer(&mut desk);
                            }
                            continue;
                        }
                        let home = scene.as_deref().map(|player| enter_scene(desk.game, desk.table, player));
                        if desk.game.world.fight_here() {
                            // A fight broke out there while the master wrote: its answer is
                            // not played over the fight. Asked once more when it can be.
                            leave_scene(desk.game, desk.table, home);
                            let why = T::ActVoidFight.text(settings.lang);
                            for out in desk.table.queues.give_back(table::Flight { acts: here, ..flight }, why, now) {
                                if let table::Out::State { player, state: table::ActState::Refused(why), .. } = out {
                                    tell_refusal(&player, &why, &me, &session, desk.phase, desk.table);
                                }
                            }
                            continue;
                        }
                        let lead = here[0].player.clone();
                        desk.game.world.acting = desk.game.world.owners.iter().position(|o| *o == lead).or_else(|| my_hero(&desk.game.world, &lead, solo));
                        // "The party" in the answer is those it answers, not the table.
                        desk.game.world.actors = if solo { Vec::new() } else { here.iter().filter_map(|a| desk.game.world.owners.iter().position(|o| *o == a.player)).collect() };
                        finish(&mut desk, &said, &draft);
                        desk.game.world.actors.clear();
                        leave_scene(desk.game, desk.table, home);
                    }
                }
            }
            // A question asked past the queues: a human master's direction, a party's defeat.
            Answer::Turn { said, near, draft, .. } => {
                let mine = matches!(&*phase, Phase::Asking { said: asked, .. } if *asked == said);
                if mine {
                    table.going_on = None;
                }
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
                        let acting_player = scene.clone().unwrap_or_else(|| player_name(&settings));
                        desk.game.world.acting = desk.game.world.owners.iter().position(|o| *o == acting_player).or_else(|| my_hero(&desk.game.world, &acting_player, matches!(*desk.session, Session::Solo)));
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
            Answer::Ambush(draft) => {
                // Drawn and told by the master: it stands there now, and the telling is read
                // before the fight begins. One that could not be drawn does not come.
                if let Ok(draft) = draft {
                    let mut desk = Desk { game: &mut game, table: &mut table, phase: &mut phase, redraw: &mut redraw, background: &mut background, session: &session, settings: &settings, store: &store, gm: &gm, master: &master };
                    finish(&mut desk, "", &draft);
                }
            }
            Answer::Chatter { place, lines } => {
                table.chatter = (place, lines.into());
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
    // Heroes are drawn by the game itself from what they are and carry.
    if false {
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
    let stirring = event_after(settings.initiative).map(|after| if game.world.table.live { after.min(150.0) } else { after }).or(game.world.table.live.then_some(150.0));
    if let Some(after) = stirring
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
        Phase::Paused | Phase::Journal | Phase::Refused { .. } | Phase::Sheet | Phase::Bag => 0.55,
    };
    // A fade in a few flat steps reads as pixel art; a smooth one does not.
    let stepped = (darkness.clamp(0.0, 1.0) * 5.0).round() / 5.0;
    for mut background in &mut veils {
        if background.0.alpha() != stepped {
            background.0 = Color::srgba(0.0, 0.0, 0.0, stepped);
        }
    }
}

/// Which sheet, if any, lies over the game, with everything it shows: when this value
/// changes the sheet is rebuilt.
#[derive(Clone, PartialEq, Default, Debug)]
enum Sheet {
    #[default]
    None,
    Drawing(String),
    Stuck(String),
    Paused { seat: SeatState, lost: bool },
    Judging(DraftView),
    Master(PanelView),
    /// `chest`: none for a thing that holds nothing; whether it is locked, for one that does.
    /// `shop`: whether it keeps a shop.
    Looking { name: String, description: String, actions: Vec<String>, chest: Option<bool>, shop: bool },
    /// Whether there are other players to talk to.
    Writing { table: bool, about: String },
    Asking(String),
    Reading(Turn),
    /// The telling folded away to a strip, so the map can be seen.
    Folded,
    /// Nothing is being asked or read: the last thing told stays in sight.
    Last(String),
    /// Just walked into a place: its name, and as much of its description as is written out.
    Arrival(String, String),
    Refused(String),
    Journal { quests: Vec<String>, carried: String, lines: Vec<(Speaker, String)> },
    /// What is being rolled, in words.
    Rolling(String),
    /// The sheets of the party, whether there are points to place, and the abilities the
    /// master offers this player to choose from.
    Character { lines: Vec<String>, points: bool, offer: Vec<String>, advancing: String },
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
#[derive(Clone, PartialEq, Default, Debug)]
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
    /// The class this player's hero takes its next levels in; empty with no hero.
    advancing: String,
    folded: bool,
    going_on: Option<String>,
    /// "X uses Y", when the field is open to say what a thing is used for.
    use_for: String,
}

/// Whether the telling being read is folded away.
#[derive(Resource, Default)]
pub struct Folded(pub bool);

/// A new telling opens unfolded; H folds and unfolds it.
fn fold_keys(keyboard: Res<ButtonInput<KeyCode>>, phase: Res<Phase>, mut folded: ResMut<Folded>) {
    if !matches!(*phase, Phase::Reading { .. }) {
        if folded.0 {
            folded.0 = false;
        }
        return;
    }
    if keyboard.just_pressed(KeyCode::KeyH) {
        folded.0 = !folded.0;
    }
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
        Sheet::Looking { name, description, actions, chest, shop } => Sheet::Looking { name: read(&name), description: read(&description), actions: actions.iter().map(|a| read(a)).collect(), chest, shop },
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
        Sheet::Journal { quests, carried, lines } => Sheet::Journal { quests: quests.iter().map(|q| read(q)).collect(), carried, lines: lines.into_iter().map(|(speaker, text)| (speaker, read(&text))).collect() },
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
                Some((name, description, actions)) => {
                    let chest = here.props.iter().find(|p| &p.id == thing).and_then(|p| p.chest.as_ref()).filter(|c| !c.open || !c.items.is_empty()).map(|c| c.lock > 0 && !c.open);
                    Sheet::Looking { name: name.clone(), description: description.clone(), actions: actions.iter().map(|a| a.label.clone()).collect(), chest, shop: world.keeps_shop(thing) }
                }
                None => Sheet::None,
            }
        }
        Phase::Writing { .. } => Sheet::Writing { table: extra.seat != SeatState::Alone, about: extra.use_for.clone() },
        Phase::Asking { said, .. } => Sheet::Asking(extra.going_on.clone().unwrap_or_else(|| said.replace(world::prompt::GO_ON, "").trim().to_string())),
        Phase::Reading { turn } => if extra.folded { Sheet::Folded } else { Sheet::Reading(turn.clone()) },
        Phase::Refused { trouble, .. } => Sheet::Refused(trouble_text(trouble, now, lang)),
        Phase::Journal => {
            let carried = world.story.bags.iter().flat_map(|b| b.inventory.iter()).chain(world.story.inventory.iter()).map(|i| if i.count > 1 { format!("{} ×{}", i.name, i.count) } else { i.name.clone() }).collect::<Vec<_>>().join(", ");
            let skip = world.story.log.len().saturating_sub(JOURNAL_LINES);
            Sheet::Journal { quests: quest_lines(world, lang), carried, lines: world.story.log.iter().skip(skip).map(|l| (l.speaker, l.text.clone())).collect() }
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
            Sheet::Character { lines, points: world.party.iter().any(|h| h.points_to_place > 0), offer: extra.offer.clone(), advancing: extra.advancing.clone() }
        }
        Phase::Walking => world.story.log.iter().rev().find(|l| l.speaker == Speaker::Master && !l.text.starts_with('[')).map_or(Sheet::None, |l| Sheet::Last(l.text.clone())),
        _ => Sheet::None,
    }
}

#[allow(clippy::too_many_arguments)]
fn send(said: String, near: Option<String>, game: &Game, gm: &Gm, store: &Store, settings: &Settings, master: &Master, session: &Session, table: &Table) -> Phase {
    let said = said.trim().to_string();
    if said.is_empty() {
        return Phase::Writing { near };
    }
    if session.is_client() {
        session.intend(Intent::Say { text: said.clone(), near: near.clone() });
        return Phase::Asking { said, near };
    }
    // It is never refused: it waits its turn in the master's queues.
    let _ = (game, gm, store, master);
    table.wants.lock().unwrap().push(Want { player: player_name(settings), said: said.clone(), near: near.clone(), follows: false });
    Phase::Asking { said, near }
}

/// A choice was taken and its outcome told: the master is asked what follows it. The
/// guests in the place read the outcome now, whenever the master's answer comes.
fn follow(label: &str, outcome: &str, game: &Game, settings: &Settings, session: &Session, table: &Table) -> Phase {
    let told = Turn { narration: outcome.to_string(), options: Vec::new() };
    for player in session.players() {
        if game.world.place_of(&player) == game.world.current {
            session.send_to(&player, &Down::Told(told.clone()));
        }
    }
    table.wants.lock().unwrap().push(Want { player: player_name(settings), said: label.to_string(), near: None, follows: true });
    Phase::Asking { said: label.to_string(), near: None }
}

/// The time the master's queues are run by: milliseconds since the game started.
fn clock_ms() -> u64 {
    static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    STARTED.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

/// How a thing done is put to the master: by whom, and whether it goes on from a choice.
fn act_line(world: &World, act: &table::Act, named: bool) -> String {
    let said = if act.follows { format!("{} {}", act.said, world::prompt::GO_ON) } else { act.said.clone() };
    if named { format!("{}: {said}", hero_of(world, &act.player).unwrap_or(&act.player)) } else { said }
}

/// What a question to the master says the party does: one thing as it was said; several as
/// a numbered list the master must answer every line of.
fn asked_words(world: &World, acts: &[table::Act], named: bool) -> String {
    match acts {
        [one] => act_line(world, one, named),
        many => {
            let mut text = String::from("several characters act at the same moment, and it is one scene — tell what comes of each of them, by name:");
            for (n, act) in many.iter().enumerate() {
                let beside = act.near.as_deref().map(|id| format!(" (standing next to \"{id}\")")).unwrap_or_default();
                text.push_str(&format!("\n{}. {}{beside}", n + 1, act_line(world, act, true)));
            }
            text
        }
    }
}

/// Tells a player that something of theirs will not be answered, and why.
fn tell_refusal(player: &str, why: &str, me: &str, session: &Session, phase: &mut Phase, table: &mut Table) {
    if player == me || matches!(session, Session::Solo) {
        if matches!(*phase, Phase::Asking { .. }) {
            *phase = Phase::Walking;
        }
        show(Turn { narration: why.to_string(), options: Vec::new() }, phase, table);
    } else {
        session.send_to(player, &Down::Trouble(why.to_string()));
    }
}

/// The master's queues: what players did is put in them, and what is due is asked. A place
/// is asked one question at a time, of everything that gathered there; places are asked
/// side by side.
#[allow(clippy::too_many_arguments)]
fn master_queue(mut game: ResMut<Game>, mut table: ResMut<Table>, mut phase: ResMut<Phase>, session: Res<Session>, settings: Res<Settings>, master: Res<Master>, gm: Res<Gm>, store: Res<Store>) {
    if session.is_client() {
        return;
    }
    let wants = std::mem::take(&mut *table.wants.lock().unwrap());
    if wants.is_empty() && table.queues.idle() {
        return;
    }
    let now = clock_ms();
    let me = player_name(&settings);
    let solo = matches!(*session, Session::Solo);
    let guests = session.players();
    let mut outs = Vec::new();
    for want in wants {
        let place = if want.player == me || solo { game.world.current.clone() } else { game.world.place_of(&want.player).to_string() };
        let there = usize::from(game.world.current == place) + guests.iter().filter(|g| game.world.place_of(g) == place).count();
        outs.extend(table.queues.submit(&place, &want.player, &want.said, want.near, want.follows, there <= 1, now).1);
    }
    // A human master reads one answer at a time: nothing more is asked while one waits.
    if table.drafts.is_empty() {
        outs.extend(table.queues.tick(now));
    }
    for out in outs {
        match out {
            table::Out::Ask { id, acts, .. } => {
                let said = asked_words(&game.world, &acts, !solo);
                let near = if acts.len() == 1 { acts[0].near.clone() } else { None };
                // The master sees the place through the eyes of the one who acts there.
                let own = solo || acts.iter().any(|a| a.player == me);
                let lead = if own { None } else { acts.first().map(|a| a.player.clone()) };
                let home = lead.as_deref().map(|player| enter_scene(&mut game, &mut table, player));
                master.ask(&gm, &store, &settings, Ask::Turn { world: Box::new(game.world.clone()), said: said.clone(), near, direction: None, id: id.0 });
                leave_scene(&mut game, &mut table, home);
                // Everyone sees what each has set about, a line a hero: the words as they
                // were said, not as the master is asked them.
                if session.is_host() {
                    for act in &acts {
                        let line = format!("{}: {}", hero_of(&game.world, &act.player).unwrap_or(&act.player), act.said);
                        session.broadcast(&Down::Thinking(line.clone()));
                        if act.player != me {
                            table.notices.push(line);
                        }
                    }
                }
            }
            table::Out::State { player, state: table::ActState::Refused(why) | table::ActState::Void(why), .. } => {
                if !why.is_empty() {
                    tell_refusal(&player, &why, &me, &session, &mut phase, &mut table);
                }
            }
            table::Out::State { .. } => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn on_activate(
    used: On<sheets::PlayPress>,
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
    (mut table, mut approach, use_for, mut shopping): (ResMut<Table>, ResMut<Approach>, Res<UseFor>, ResMut<crate::shop_ui::Shopping>),
) {
    // This observer hears every button in the game; outside a game there is nothing to act on.
    let Some(mut game) = game else { return };
    let action = &used.action;
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
        let typed = used.typed.clone();
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
            (PlayAction::Chest(how), Phase::Looking { thing }) => Some(Intent::OpenChest { prop: thing, how }),
            (PlayAction::Choose(index), Phase::Reading { turn }) => turn.options.get(index).map(|option| Intent::Choose { index, label: option.label.clone() }),
            (PlayAction::Roll, Phase::Rolling) => Some(Intent::Roll),
            (PlayAction::Improve(ability), _) => Some(Intent::Improve(ability)),
            (PlayAction::Feat(choice), _) => Some(Intent::Feat(choice)),
            (PlayAction::Advance, _) => {
                // Shown at once here; the host's world confirms it.
                let me = player_name(&settings);
                if let Some(hero) = my_hero(&game.world, &me, false).and_then(|i| game.world.party.get_mut(i)) {
                    hero.advance_next_class();
                }
                session.intend(Intent::Advance);
                return;
            }
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
                // Everyone in the place watches the die fall.
                background.die = Some(done.roll.die);
                for player in session.players() {
                    if game.world.place_of(&player) == game.world.current {
                        session.send_to(&player, &Down::Die(done.roll.die));
                    }
                }
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
            match game.world.can_rest_long() {
                Ok(()) => {
                    // A night in a dangerous place may be broken: two rolls, as for two watches.
                    let mut rng = Rng::from_clock();
                    let was_night = game.world.is_night();
                    let broken = (0..2).any(|_| {
                        // Sleep is at its most dangerous in the dark, whatever the hour it began.
                        let clock = game.world.clock;
                        if !was_night {
                            game.world.clock = clock / 1440 * 1440 + 23 * 60;
                        }
                        let came = match game.world.wander(&mut rng) {
                            Some(world::living::Wandered::Came(told)) => {
                                table.unread.push_back(Turn { narration: told, options: Vec::new() });
                                true
                            }
                            Some(world::living::Wandered::Ambush(comer, cell)) => {
                                table.ambush = Some((comer, cell));
                                true
                            }
                            _ => false,
                        };
                        game.world.clock = clock;
                        came
                    });
                    if broken {
                        game.world.pass(3 * 60);
                        *toast = Toast { text: Some(T::RestBroken), left: 5.0 };
                    } else {
                        game.world.eat_rations();
                        game.world.long_rest();
                        game.world.pass(8 * 60);
                        *toast = Toast { text: Some(T::Rested), left: 3.0 };
                    }
                    redraw.0 = true;
                    autosave(&store, &game);
                }
                Err(world::bag::RestTrouble::NotSafe) => *toast = Toast { text: Some(T::NotSafe), left: 4.0 },
                Err(world::bag::RestTrouble::NoRation) => *toast = Toast { text: Some(T::NeedRation), left: 4.0 },
            }
            *phase = Phase::Walking;
        }
        (PlayAction::ShortRest, _) => {
            game.world.short_rest(&mut Rng::from_clock());
            game.world.pass(60);
            redraw.0 = true;
            *toast = Toast { text: Some(T::ShortRested), left: 3.0 };
            autosave(&store, &game);
            *phase = Phase::Walking;
        }
        (PlayAction::NewHero, _) => {
            autosave(&store, &game);
            *phase = Phase::Walking;
            next.set(Screen::Hero);
        }
        (PlayAction::Advance, _) => {
            let me = player_name(&settings);
            if let Some(hero) = my_hero(&game.world, &me, matches!(*session, Session::Solo)).and_then(|i| game.world.party.get_mut(i)) {
                hero.advance_next_class();
                autosave(&store, &game);
            }
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
            return_to.push(Screen::Play);
            // The keys of whoever is the master now: a provider's, or Gemini's.
            next.set(if settings.active_provider().is_some() { Screen::Provider } else { Screen::Keys });
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
            if let (true, Some(label)) = (session.is_host(), action_label(&game.world, &thing, index)) {
                announce(&session, &mut table, &game.world, &player_name(&settings), &label, true);
            }
            if let Some(outcome) = game.world.perform(&thing, index) {
                *phase = told(outcome_of(outcome), Vec::new(), &mut game, &store, &mut redraw, &mut background, &session, &mut table);
            }
        }
        (PlayAction::Chest(how), Phase::Looking { thing }) => {
            let hero = my_hero(&game.world, &player_name(&settings), matches!(*session, Session::Solo)).unwrap_or(0);
            if let Some(result) = game.world.open_chest(hero, &thing, how, &mut Rng::from_clock()) {
                *phase = told(outcome_of(chest_words(&result, settings.lang)), Vec::new(), &mut game, &store, &mut redraw, &mut background, &session, &mut table);
            }
        }
        (PlayAction::RedrawPlace, _) => {
            // The master is shown the world without this place, as if it were still only a
            // name, and asked to draw it as walked into from a neighbour that exists.
            let id = game.world.current.clone();
            let from = game.world.here().exits.iter().map(|e| e.to.clone()).find(|to| game.world.locations.contains_key(to));
            match from {
                Some(from) => {
                    let mut seen = game.world.clone();
                    if let Some(old) = seen.locations.remove(&id) {
                        seen.stubs.insert(id.clone(), world::Stub { name: old.name, description: old.description });
                    }
                    seen.current = from.clone();
                    master.ask(&gm, &store, &settings, Ask::Location { world: Box::new(seen), id: id.clone(), from, ahead: false });
                    *phase = Phase::Drawing { to: id };
                }
                None => {
                    *toast = Toast { text: Some(T::RedrawNoWay), left: 4.0 };
                    *phase = Phase::Walking;
                }
            }
        }
        (PlayAction::Trade, Phase::Looking { thing }) => {
            // The wares are the host's to lay out; a guest asks, and sees them a moment later.
            if session.is_client() {
                session.intend(Intent::Shop { keeper: thing.clone() });
            }
            game.world.restock(&thing);
            shopping.0 = Some(thing);
            *phase = Phase::Bag;
        }
        (PlayAction::Own, Phase::Looking { thing }) => *phase = Phase::Writing { near: Some(thing) },
        (PlayAction::Own, _) => *phase = Phase::Writing { near: None },
        (PlayAction::Send, Phase::Writing { near }) => {
            *phase = send(with_use(&use_for, used.typed.clone()), near, &game, &gm, &store, &settings, &master, &session, &table);
        }
        (PlayAction::Approach(index), Phase::Reading { turn }) => {
            if let Some(option) = turn.options.get(index) {
                match way_to(&game.world, option.near.as_deref().unwrap_or_default()) {
                    Some(path) => {
                        approach.0 = Some(Walking { path, option: option.clone(), index, turn: turn.clone() });
                        *phase = Phase::Walking;
                    }
                    None => *phase = Phase::Reading { turn: Turn { narration: format!("{}\n\n{}", turn.narration, T::CannotReach.text(settings.lang)), options: turn.options.clone() } },
                }
            }
        }
        (PlayAction::Choose(index), Phase::Reading { turn }) => {
            let me = player_name(&settings);
            let solo = matches!(*session, Session::Solo);
            if let Some(option) = turn.options.get(index).filter(|o| mine_to_take(o, &game.world, &me, solo)) {
                // The outcome happens at once; the master is asked what follows it.
                let outcome = game.world.choose(option);
                redraw.0 = true;
                autosave(&store, &game);
                if let Some(offered) = table.options.get_mut(&game.world.current) {
                    offered.retain(|o| o.label != option.label);
                }
                table.going_on = Some(outcome.clone());
                *phase = follow(&option.label, &outcome, &game, &settings, &session, &table);
            }
        }
        (PlayAction::AskAgain, Phase::Refused { said, near, .. }) => *phase = send(said, near, &game, &gm, &store, &settings, &master, &session, &table),
        _ => {}
    }
}

/// The name of the hero a player plays, if they have one in this world.
/// Whether a player may take a choice: one offered to a character is that character's
/// player's alone; alone at the table, every hero is the player's.
pub fn mine_to_take(option: &world::Action, world: &World, player: &str, solo: bool) -> bool {
    match &option.for_hero {
        None => true,
        Some(_) if solo => true,
        Some(hero) => hero_of(world, player).is_some_and(|mine| mine.trim().eq_ignore_ascii_case(hero.trim())),
    }
}

fn hero_of<'a>(world: &'a World, player: &str) -> Option<&'a str> {
    let index = world.owners.iter().position(|o| o == player)?;
    world.party.get(index).map(|h| h.name.as_str())
}

/// The name of the place the party is in, as this player reads it.
#[derive(Resource, Default)]
struct ShownPlace(String);

/// The world's clock runs a minute to a second of play, on the host's machine. Alone it
/// stops with the game paused or the window out of sight; a fight counts its own rounds.
fn run_clock(time: Res<Time>, mut game: ResMut<Game>, mut phase: ResMut<Phase>, session: Res<Session>, window: Single<&Window, With<bevy::window::PrimaryWindow>>, mut redraw: ResMut<Redraw>, mut since: Local<f32>, mut table: ResMut<Table>, lang: Res<Lang>) {
    if session.is_client() || game.world.party.is_empty() || matches!(*phase, Phase::Fighting | Phase::Drawing { .. } | Phase::Stuck { .. }) {
        return;
    }
    let alone = matches!(*session, Session::Solo);
    if alone && (*phase == Phase::Paused || !window.focused && window.visible) {
        return;
    }
    *since += time.delta_secs();
    if *since < 1.0 {
        return;
    }
    let minutes = since.floor();
    *since -= minutes;
    // The minutes go by unseen; the picture is drawn again only when the light changes.
    let world = &mut game.bypass_change_detection().world;
    let hour_before = world.clock / 60;
    world.clock = world.clock.saturating_add(minutes as u32);
    let new_hour = world.clock / 60 != hour_before;
    if world.sync_light() {
        game.set_changed();
        redraw.0 = true;
    }
    // An hour gone by in a dangerous place: something may wander in.
    if new_hour && *phase == Phase::Walking {
        match game.bypass_change_detection().world.wander(&mut Rng::from_clock()) {
            Some(world::living::Wandered::Find(text)) => {
                game.set_changed();
                game.world.story.say(Speaker::Master, &text);
                show(Turn { narration: text, options: Vec::new() }, &mut phase, &mut table);
            }
            Some(world::living::Wandered::Came(told)) => {
                game.set_changed();
                redraw.0 = true;
                show(Turn { narration: told, options: Vec::new() }, &mut phase, &mut table);
            }
            Some(world::living::Wandered::Ambush(comer, cell)) => {
                table.ambush = Some((comer, cell));
                table.notices.push(T::SomethingComes.text(*lang).to_string());
            }
            None => {}
        }
    }
}

/// The main quest and the open side tasks, a line each, for the journal.
fn quest_lines(world: &World, lang: Lang) -> Vec<String> {
    let quest = &world.quest;
    let mut lines = Vec::new();
    if quest.is_set() {
        let state = match quest.done {
            Some(true) => format!(" — {}", T::QuestWon.text(lang)),
            Some(false) => format!(" — {}", T::QuestLost.text(lang)),
            None => String::new(),
        };
        lines.push(format!("{}{state}", quest.title));
        if quest.done.is_none() {
            lines.push(format!("{} {}", T::Goal.text(lang), quest.goal));
        }
    }
    for task in &quest.tasks {
        lines.push(format!("{} {}{}", if task.done { "✓" } else { "•" }, task.name, if task.done || task.step.is_empty() { String::new() } else { format!(": {}", task.step) }));
    }
    // Those the party has had to do with, and how they regard it.
    let words: Vec<&str> = T::StandingWords.text(lang).split('|').collect();
    for faction in world.factions.iter().filter(|f| f.met) {
        lines.push(format!("⚑ {}: {}", faction.name, words.get((faction.standing + 3).clamp(0, 6) as usize).copied().unwrap_or("")));
    }
    // What is heard of things that ripen, with no hour named.
    for threat in world.threats.iter().filter(|t| !t.rumour.is_empty()) {
        lines.push(format!("{}: {}", T::Rumours.text(lang), threat.rumour));
    }
    lines
}

/// The goal of the main quest, always in sight in the corner of the map; a change of goal or
/// a new task is also called out in the corner feed.
/// What a prepared action of a thing here is called.
fn action_label(world: &World, thing: &str, index: usize) -> Option<String> {
    let here = world.here();
    let (name, actions) = here.props.iter().find(|p| p.id == thing).map(|p| (&p.name, &p.actions)).or_else(|| here.creatures.iter().find(|c| c.id == thing).map(|c| (&c.name, &c.actions)))?;
    actions.get(index).map(|a| format!("{} ({name})", a.label))
}

/// Takes a choice out of those offered in a place: the one with the words the player read.
/// The list may have changed since they read it, so the place in it is only trusted when no
/// words came with it. Taken, a choice is no longer offered: nobody takes it twice.
fn take_offer(offered: &mut Vec<world::Action>, index: usize, label: &str) -> Option<world::Action> {
    let at = if label.is_empty() { Some(index).filter(|i| *i < offered.len()) } else { offered.iter().position(|o| o.label == label) }?;
    Some(offered.remove(at))
}

/// Tells everyone at the table what a player has set about.
fn announce(session: &Session, table: &mut Table, world: &World, player: &str, what: &str, own: bool) {
    let said = format!("{}: {what}", hero_of(world, player).unwrap_or(player));
    session.broadcast(&Down::Thinking(said.clone()));
    // The one who did it has it on their own sheet.
    if !own {
        table.notices.push(said);
    }
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
        let home = matches!(intent, Intent::Do { .. } | Intent::Choose { .. } | Intent::Roll | Intent::Say { .. } | Intent::Gm(_) | Intent::OpenChest { .. }).then(|| enter_scene(&mut game, &mut table, &player));
        let shown = match intent {
            Intent::Do { thing, index } => {
                if let Some(label) = action_label(&game.world, &thing, index) {
                    announce(&session, &mut table, &game.world, &player, &label, false);
                }
                game.world.perform(&thing, index).map(outcome_of)
            }
            Intent::Choose { index, label } => {
                let place = game.world.current.clone();
                // A choice offered to somebody else's hero is not this guest's to take.
                let theirs = table.options.get(&place).and_then(|offered| offered.iter().find(|o| o.label == label)).filter(|o| !mine_to_take(o, &game.world, &player, false)).and_then(|o| o.for_hero.clone());
                if let Some(hero) = theirs {
                    session.send_to(&player, &Down::Trouble(format!("{} {hero}.", T::OfferForOther.text(settings.lang))));
                    continue;
                }
                match table.options.get_mut(&place).and_then(|offered| take_offer(offered, index, &label)) {
                    Some(option) => {
                        let outcome = game.world.choose(&option);
                        // And what follows it is asked of the master: always, in its turn.
                        table.wants.lock().unwrap().push(Want { player: player.clone(), said: option.label.clone(), near: None, follows: true });
                        Some(outcome_of(outcome))
                    }
                    // Told in words: a press that does nothing looks like a game that hangs.
                    None => {
                        session.send_to(&player, &Down::Trouble(T::ChoiceGone.text(settings.lang).to_string()));
                        None
                    }
                }
            }
            Intent::Roll if !game.world.pending_here() => None,
            Intent::Roll => game.world.roll_check(&mut Rng::from_clock()).map(|done| {
                if !table.elsewhere {
                    background.die = Some(done.roll.die);
                }
                for other in session.players() {
                    if game.world.place_of(&other) == game.world.current {
                        session.send_to(&other, &Down::Die(done.roll.die));
                    }
                }
                let verdict = if done.success { T::CheckSuccess } else { T::CheckFailure };
                outcome_of(format!("{}: d20 {}{:+} = {} / {} — {}\n{}", done.hero, done.roll.die, done.roll.bonus, done.roll.total(), done.difficulty, verdict.text(settings.lang), done.narration))
            }),
            Intent::Say { text, near } => {
                // Never refused: it waits its turn in the queue of the place it was done in.
                if !text.trim().is_empty() {
                    table.wants.lock().unwrap().push(Want { player: player.clone(), said: text.trim().to_string(), near, follows: false });
                }
                None
            }
            Intent::Exit { to } => {
                // A guest walks on alone: the party may split up. A place that is only a
                // name is drawn first, and the guest waits at the door.
                let from = game.world.place_of(&player).to_string();
                // What the guest had waiting for the master in the place they leave did not happen.
                for out in table.queues.gone_from(&from, &player, T::ActVoidLeft.text(settings.lang)) {
                    if let table::Out::State { player, state: table::ActState::Void(why), .. } = out {
                        session.send_to(&player, &Down::Trouble(why));
                    }
                }
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
            Intent::Advance => {
                if let Some(hero) = game.world.owners.iter().position(|o| *o == player).and_then(|i| game.world.party.get_mut(i)) {
                    hero.advance_next_class();
                }
                None
            }
            Intent::Stunt { said, spell } => {
                // Only the player whose hero is acting may try something for it.
                if game.world.acting_owner() == Some(player.as_str()) {
                    view.wait_for_master();
                    announce(&session, &mut table, &game.world, &player, &said, false);
                    // The master sees the fight through the eyes of the one who acts.
                    let mut seen = game.world.clone();
                    if let Some(token) = seen.others.iter().find(|t| t.name == player) {
                        seen.facing = token.facing;
                    }
                    master.ask(&gm, &store, &settings, Ask::Stunt { world: Box::new(seen), said, spell });
                }
                None
            }
            // Each player's bag is their own hero's.
            Intent::Arrange { held, to } => {
                if let Some(hero) = game.world.owners.iter().position(|o| *o == player) {
                    game.world.arrange(hero, &held, to);
                }
                None
            }
            Intent::Shop { keeper } => {
                let home = enter_scene(&mut game, &mut table, &player);
                game.world.restock(&keeper);
                leave_scene(&mut game, &mut table, Some(home));
                None
            }
            Intent::Buy { keeper, ware } => {
                if let Some(hero) = game.world.owners.iter().position(|o| *o == player) {
                    let home = enter_scene(&mut game, &mut table, &player);
                    game.world.restock(&keeper);
                    let _ = game.world.buy(hero, &keeper, ware);
                    leave_scene(&mut game, &mut table, Some(home));
                }
                None
            }
            Intent::Sell { keeper, index } => {
                if let Some(hero) = game.world.owners.iter().position(|o| *o == player) {
                    let home = enter_scene(&mut game, &mut table, &player);
                    let _ = game.world.sell(hero, &keeper, index);
                    leave_scene(&mut game, &mut table, Some(home));
                }
                None
            }
            Intent::Haggle { keeper } => {
                if let Some(hero) = game.world.owners.iter().position(|o| *o == player) {
                    let home = enter_scene(&mut game, &mut table, &player);
                    game.world.haggle(hero, &keeper, &mut Rng::from_clock());
                    leave_scene(&mut game, &mut table, Some(home));
                }
                None
            }
            Intent::ArmCompanion { companion, index } => {
                if let Some(hero) = game.world.owners.iter().position(|o| *o == player) {
                    game.world.arm_companion(companion, hero, index);
                }
                None
            }
            Intent::PackCompanion { companion, index } => {
                if let Some(hero) = game.world.owners.iter().position(|o| *o == player) {
                    game.world.pack_companion(companion, hero, index);
                }
                None
            }
            Intent::UnpackCompanion { companion, item } => {
                if let Some(hero) = game.world.owners.iter().position(|o| *o == player) {
                    game.world.unpack_companion(companion, hero, item);
                }
                None
            }
            Intent::DisarmCompanion { companion, weapon } => {
                if let Some(hero) = game.world.owners.iter().position(|o| *o == player) {
                    game.world.disarm_companion(companion, hero, weapon);
                }
                None
            }
            Intent::DropGold { amount, cell } => {
                if let Some(hero) = game.world.owners.iter().position(|o| *o == player) {
                    let home = enter_scene(&mut game, &mut table, &player);
                    game.world.drop_gold(hero, amount, cell);
                    leave_scene(&mut game, &mut table, Some(home));
                    redraw.0 = true;
                }
                None
            }
            Intent::Quick { slot, name } => {
                if let Some(hero) = game.world.owners.iter().position(|o| *o == player) {
                    game.world.set_quick(hero, slot, &name);
                }
                None
            }
            Intent::UseItem { index } => {
                if let Some(hero) = game.world.owners.iter().position(|o| *o == player) {
                    game.world.use_item(hero, index, &mut Rng::from_clock());
                }
                None
            }
            Intent::Drop { index, cell } => {
                if let Some(hero) = game.world.owners.iter().position(|o| *o == player) {
                    let home = enter_scene(&mut game, &mut table, &player);
                    game.world.drop_item(hero, index, cell);
                    leave_scene(&mut game, &mut table, Some(home));
                    redraw.0 = true;
                }
                None
            }
            Intent::OpenChest { prop, how } => {
                let hero = game.world.owners.iter().position(|o| *o == player).unwrap_or(0);
                game.world.open_chest(hero, &prop, how, &mut Rng::from_clock()).map(|result| outcome_of(chest_words(&result, settings.lang)))
            }
            Intent::PickUp { n } => {
                if let Some(hero) = game.world.owners.iter().position(|o| *o == player) {
                    let home = enter_scene(&mut game, &mut table, &player);
                    game.world.pick_up(hero, n);
                    leave_scene(&mut game, &mut table, Some(home));
                    redraw.0 = true;
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
    // The master is told what is in front of the one who acts, not of the host.
    if let Some(token) = game.world.others.iter().find(|t| t.name == player) {
        game.world.facing = token.facing;
    }
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
    // What the master said and did, for the log file: the effects tell whether the picture
    // could follow the story.
    let ops: Vec<String> = draft.effects.iter().map(|e| e.get("op").and_then(|o| o.as_str()).unwrap_or("?").to_string()).collect();
    info!("telling: {} | effects: [{}]", draft.narration.chars().take(160).collect::<String>().replace('\n', " "), ops.join(", "));
    let (turn, notes) = desk.game.world.take_turn(said, draft);
    for note in &notes {
        info!("telling note: {note}");
    }
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
            desk.master.ask(desk.gm, desk.store, desk.settings, Ask::Turn { world: Box::new(desk.game.world.clone()), said: said.clone(), near, direction: Some(text.clone()), id: 0 });
            desk.session.broadcast(&Down::Thinking(text));
            desk.table.busy = Some(said);
        }
        GmAct::Apply(draft) => finish(desk, "", &draft),
        GmAct::Whisper { to, text } => whisper(desk, &[Whisper { to, text }]),
        GmAct::Say { who, text } => {
            let place = desk.game.world.current.clone();
            desk.session.broadcast(&Down::Bubble { place, who: who.clone(), text: text.clone() });
            desk.table.bubbles.push((who, text));
        }
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
    let needs_text = matches!(tool, Tool::ToModel | Tool::Narrate | Tool::Whisper | Tool::Creature | Tool::Tile | Tool::Damage | Tool::Heal | Tool::Give | Tool::Invent | Tool::Gold | Tool::Hours | Tool::Sky | Tool::Say) || (tool == Tool::Brush && panel.brush.is_none());
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
        Tool::Gold => apply(serde_json::json!([{ "op": "gold", "amount": typed.parse::<i64>().unwrap_or(0) }])),
        Tool::Hours => apply(serde_json::json!([{ "op": "time", "hours": typed.replace(',', ".").parse::<f64>().unwrap_or(1.0) }])),
        Tool::Sky => match typed.to_lowercase().as_str() {
            light @ ("day" | "dusk" | "night" | "dark" | "день" | "вечер" | "ночь" | "тьма") => {
                let light = match light {
                    "день" => "day",
                    "вечер" => "dusk",
                    "ночь" => "night",
                    "тьма" => "dark",
                    other => other,
                };
                apply(serde_json::json!([{ "op": "light", "light": light }]))
            }
            sky => apply(serde_json::json!([{ "op": "weather", "weather": sky }])),
        },
        Tool::Say => match world.thing_near(REMOVE_REACH * 3.0) {
            Some(who) => GmAct::Say { who: who.to_string(), text: typed.to_string() },
            None => {
                table.master_note = T::NothingNear.text(lang).to_string();
                return None;
            }
        },
        Tool::Brush => {
            // On with a tile in the field; off with another press.
            panel.brush = if panel.brush.is_some() { None } else { Some(typed.to_lowercase().replace([' ', '-'], "_")) };
            table.master_note = match &panel.brush {
                Some(tile) => format!("{} {tile}", T::BrushOn.text(lang)),
                None => String::new(),
            };
            panel.typed.clear();
            return None;
        }
        Tool::Creature => {
            // A monster of the rules by its index, or anything at all by its name; a number
            // after it is how many: "wolf 3".
            let (typed, count) = match typed.rsplit_once(' ') {
                Some((name, n)) if n.parse::<u8>().is_ok() => (name.trim(), n.parse::<u8>().unwrap_or(1).clamp(1, 8)),
                _ => (typed, 1),
            };
            let with_count = |mut draft: TurnDraft| {
                if let Some(effect) = draft.effects.first_mut().and_then(|e| e.as_object_mut()) {
                    effect.insert("count".to_string(), serde_json::json!(count));
                }
                draft
            };
            let index = typed.to_lowercase().replace([' ', '_'], "-");
            let card = match rules::srd().monsters.get(index.as_str()) {
                Some(monster) => CreatureCard { id: index.clone(), name: tr(lang == Lang::Ru, &monster.name).to_string(), monster: Some(index), ..CreatureCard::default() },
                None => CreatureCard { id: typed.to_string(), name: typed.to_string(), ..CreatureCard::default() },
            };
            GmAct::Apply(Box::new(with_count(card.summon(x + 1, y, panel.hostile))))
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
    used: On<sheets::PlayPress>,
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
    let Some(mut game) = game else { return };
    let action = &used.action;
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
            let narration = used.typed.clone();
            if guest {
                table.judging = None;
                *phase = Phase::Walking;
            }
            GmAct::Verdict { accept, narration }
        }
        (PlayAction::Redo, Phase::Judging) => {
            let direction = used.typed.clone();
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
            let typed = used.typed.clone();
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
            .init_resource::<SceneCache>()
            .init_resource::<Folded>()
            .init_resource::<Leave>()
            .init_resource::<Pick>()
            .init_resource::<Approach>()
            .init_resource::<UseFor>()
            .init_resource::<ShownPlace>()
            .add_systems(Update, ambush_ask.run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>))
            .add_systems(Update, brush_paint.run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>))
            .add_systems(Update, (place_allies, live_world, bubbles).chain().run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>))
            .add_systems(Update, run_clock.run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>))
            .add_systems(Update, keep_facing.run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>))
            .init_resource::<Dolls>()
            .init_resource::<Background>()
            .init_resource::<Panel>()
            .add_systems(OnEnter(Screen::Play), enter)
            .add_systems(OnExit(Screen::Play), leave)
            .add_systems(
                Update,
                (keys, walk, approach_work, led, advance, receive, guests, master_queue, background_work, cues, draw, place_hero, place_fellows, water, lighting, walk_grid, veil).chain().run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>),
            )
            .add_systems(Update, (fold_keys, roofs, exit_markers, exits_seen, bag_work, forget_going_on, notice, relocate, redress).run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>))
            .add_observer(on_activate)
            .add_observer(on_master_activate)
            .init_resource::<sheets::Fields>()
            .init_resource::<hud::Notices>()
            .add_systems(bevy_egui::EguiPrimaryContextPass, hud::hud.in_set(crate::ui::UiSet::Screens).run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>))
            .add_systems(bevy_egui::EguiPrimaryContextPass, sheets::overlay.in_set(crate::ui::UiSet::Screens).run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn players_elsewhere_read_the_first_sentence_of_a_telling() {
        assert_eq!(gist("Бора тянется за кучей, поскальзывается и падает. Гаргулья смеётся."), "Бора тянется за кучей, поскальзывается и падает.");
        assert_eq!(gist("Ok. Then the door opens slowly and a cold wind comes in."), "Ok. Then the door opens slowly and a cold wind comes in.");
        assert!(gist(&"a".repeat(400)).ends_with('…'));
    }

    fn world() -> World {
        let draft: world::WorldDraft = serde_json::from_str(&fake::world("play")).unwrap();
        world::build_world(&draft, "play", "English").unwrap().0
    }

    #[test]
    fn a_telling_arrives_whatever_the_player_has_open() {
        let told = |text: &str| Turn { narration: text.to_string(), options: Vec::new() };
        let mut table = Table::default();
        // Walking: it is read at once.
        let mut phase = Phase::Walking;
        show(told("The door creaks."), &mut phase, &mut table);
        assert!(matches!(&phase, Phase::Reading { turn } if turn.narration == "The door creaks."));
        // Reading: the next one follows in the same panel, nothing waits.
        show(told("A cat runs in."), &mut phase, &mut table);
        assert!(matches!(&phase, Phase::Reading { turn } if turn.narration == "The door creaks.\n\nA cat runs in."));
        assert!(table.unread.is_empty());
        // The bag open: it waits for the bag to close, and the corner says so.
        let mut phase = Phase::Bag;
        show(told("Rain begins."), &mut phase, &mut table);
        assert_eq!(phase, Phase::Bag);
        assert_eq!(table.unread.len(), 1);
        assert!(table.notices.iter().any(|n| n.contains("Rain begins.")));
    }

    #[test]
    fn a_hero_the_story_moves_walks_there_cell_by_cell() {
        let w = world();
        let here = w.here();
        let (px, py) = w.player.cell();
        // A walkable cell a few steps away.
        let goal = (0..here.size().1).flat_map(|y| (0..here.size().0).map(move |x| (x, y))).find(|(x, y)| (x.abs_diff(px as usize) + y.abs_diff(py as usize)) >= 3 && !here.blocked(*x as i32, *y as i32, &w.tiles) && here.exit_at(*x, *y).is_none()).unwrap();
        let path = way_to_cell(&w, goal).expect("a way there");
        assert_eq!(path.last(), Some(&goal));
        // Each step is to a cell beside the one before.
        let mut at = (px as usize, py as usize);
        for cell in &path {
            assert_eq!(cell.0.abs_diff(at.0) + cell.1.abs_diff(at.1), 1, "{at:?} -> {cell:?}");
            at = *cell;
        }
    }

    #[test]
    fn the_locations_corner_is_the_screens_corner_and_y_is_flipped() {
        assert_eq!(on_screen(0.0, 0.0), Vec2::new(-480.0, 270.0));
        assert_eq!(on_screen(480.0, 256.0), Vec2::new(480.0, -242.0));
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
        let Sheet::Journal { carried, lines, .. } = sheet_for(&Phase::Journal, &w, 0, Lang::En, &Extra::default()) else { panic!("expected the journal") };
        assert_eq!(carried, "Cat hair");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], (Speaker::Player, "Shoo it away".to_string()));
    }

    #[test]
    fn the_master_speaks_unprompted_only_when_the_host_allows_it() {
        assert_eq!(event_after(Initiative::Off), None);
        assert!(event_after(Initiative::Often) < event_after(Initiative::Rare));
    }

    /// Feeds a figure news of a walk at `speed` pixels a second, the news coming at the
    /// given moments, and draws it sixty times a second. Gives the largest step drawn in
    /// one frame and where the figure ended.
    fn walked(news_at: &[f32], speed: f32, until: f32) -> (f32, Vec2) {
        let mut glide = Glide::at(Vec2::ZERO);
        let (mut heard, mut next, mut t, mut largest) = (Vec2::ZERO, 0, 0.0f32, 0.0f32);
        let dt = 1.0 / 60.0;
        while t < until {
            t += dt;
            while next < news_at.len() && news_at[next] <= t {
                heard = Vec2::new(news_at[next] * speed, 0.0);
                next += 1;
            }
            let before = glide.shown;
            let shown = glide.step(heard, dt);
            largest = largest.max(shown.distance(before));
        }
        (largest, glide.shown)
    }

    #[test]
    fn another_players_walk_is_drawn_in_even_steps_not_in_jumps() {
        // News ten times a second of a walk at the heroes' own pace: set down where it
        // was heard, the figure would jump over seven pixels at a time.
        let even: Vec<f32> = (1..=20).map(|n| n as f32 * 0.1).collect();
        let (largest, ended) = walked(&even, SPEED, 2.4);
        let a_frame = SPEED / 60.0;
        assert!(largest <= a_frame * 1.6, "a step of {largest} where {a_frame} is a frame's walk");
        // And it comes to rest exactly where the player stopped.
        assert_eq!(ended, Vec2::new(2.0 * SPEED, 0.0));
    }

    #[test]
    fn news_that_comes_unevenly_still_makes_no_jump() {
        // The same walk, the news now early, now late by a good part of its interval.
        let uneven: Vec<f32> = (1..=20).map(|n| n as f32 * 0.1 + if n % 3 == 0 { 0.06 } else if n % 3 == 1 { -0.03 } else { 0.0 }).collect();
        let (largest, ended) = walked(&uneven, SPEED, 2.6);
        // Set down where it was heard, the figure would jump by the walk of the longest gap
        // between two pieces of news, 0.16 s here: over eleven pixels. Gliding, it catches
        // up after late news in steps of about three frames' walk at most.
        let set_down = 0.16 * SPEED;
        assert!(largest <= SPEED / 60.0 * 3.2 && largest < set_down / 3.0, "a step of {largest}, where set down it would be {set_down}");
        assert!((ended.x - uneven[19] * SPEED).abs() < 0.01);
    }

    #[test]
    fn a_hero_the_story_sets_down_elsewhere_is_there_at_once() {
        let mut glide = Glide::at(Vec2::ZERO);
        let far = Vec2::new(GLIDE_JUMP + 20.0, 0.0);
        assert_eq!(glide.step(far, 1.0 / 60.0), far);
        // A figure that has only just appeared starts where it is, not at the corner of the map.
        assert_eq!(Glide::default().step(Vec2::new(300.0, 200.0), 1.0 / 60.0), Vec2::new(300.0, 200.0));
    }

    fn offer(label: &str) -> world::Action {
        serde_json::from_value(serde_json::json!({ "label": label, "outcome": format!("{label}, done"), "effects": [] })).expect("a choice as it is kept")
    }

    #[test]
    fn a_choice_for_one_hero_is_that_heros_players_alone() {
        let mut w = world();
        let skills: Vec<String> = vec!["athletics".into(), "perception".into()];
        w.party.push(rules::Character::create("Bora", "", "human", "fighter", [15, 12, 14, 8, 10, 13], &skills, &[]).unwrap());
        w.party.push(rules::Character::create("Tikhon", "", "human", "fighter", [15, 12, 14, 8, 10, 13], &skills, &[]).unwrap());
        w.owners = vec!["Ann".into(), "Bob".into()];
        let mut for_bora = offer("Turn the key");
        for_bora.for_hero = Some("bora".into());
        assert!(mine_to_take(&for_bora, &w, "Ann", false), "the name is read whatever its case");
        assert!(!mine_to_take(&for_bora, &w, "Bob", false));
        assert!(mine_to_take(&for_bora, &w, "Bob", true), "alone, every hero is the player's");
        assert!(mine_to_take(&offer("Run"), &w, "Bob", false));
        // The master writes it as "for".
        let drafts: Vec<world::story::ActionDraft> = serde_json::from_value(serde_json::json!([{ "label": "Turn the key", "outcome": "It turns.", "for": "Bora" }])).unwrap();
        assert_eq!(world::story::read_actions(&drafts, &mut Vec::new())[0].for_hero.as_deref(), Some("Bora"));
    }

    #[test]
    fn a_choice_is_found_by_its_words_and_taken_only_once() {
        // The guest read "Wait" second in the list; since then the list has changed.
        let mut offered = vec![offer("Run"), offer("Hide"), offer("Wait")];
        assert_eq!(take_offer(&mut offered, 1, "Wait").map(|o| o.label), Some("Wait".to_string()));
        assert_eq!(offered.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), ["Run", "Hide"]);
        // Somebody else pressing the same choice finds it gone, and nothing else is taken
        // in its stead — not even what now stands in its old place.
        assert!(take_offer(&mut offered, 1, "Wait").is_none());
        assert_eq!(offered.len(), 2);
        // A choice that was never offered here is not one.
        assert!(take_offer(&mut offered, 0, "Fly").is_none());
        // Without words the place in the list is all there is to go by.
        assert_eq!(take_offer(&mut offered, 0, "").map(|o| o.label), Some("Run".to_string()));
        assert!(take_offer(&mut offered, 5, "").is_none());
    }
}
