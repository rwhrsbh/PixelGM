//! Playing together. One player hosts and holds the world; the others send what they want
//! to do and are sent what happened. A game played alone is the same game with nobody
//! connected.

use crate::art::Art;
use crate::fight::FightView;
use crate::i18n::T;
use crate::menu::{Nav, Screen, screen_root, spawn_panel};
use crate::play::{self, Game, Phase, Redraw};
use crate::store::{Settings, Store};
use crate::theme::*;
use crate::widgets::{Activate, BUTTON_HEIGHT, backing, button};
use bevy::input_focus::AutoFocus;
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigation;
use net::{Client, ClientEvent, Host, HostEvent, PeerId};
use rules::{Ability, Character, Event};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use world::{Action, FightIntent, FightSummary, Token, Turn, World};

/// Raised whenever the messages change shape; players with another number are turned away.
pub const PROTOCOL: u32 = 1;
pub const DEFAULT_PORT: u16 = 47_800;

/// Something a player wants to do that changes the world. A client sends it; the host does
/// it, exactly as it does for its own player.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Intent {
    /// A prepared action of a thing.
    Do { thing: String, index: usize },
    /// The party member's own words, for the game master.
    Say { text: String, near: Option<String> },
    /// One of the choices offered with the last telling.
    Choose(usize),
    /// Roll the check that is waiting.
    Roll,
    /// The player walked onto an exit.
    Exit { to: String },
    Fight(FightIntent),
    /// A point into an ability of the player's own hero.
    Improve(Ability),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Up {
    Hello { name: String, version: u32 },
    Position { x: f32, y: f32 },
    Intent(Intent),
    Hero(Box<Character>),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Down {
    /// The whole world, sent on joining and whenever it changes.
    World(Box<World>),
    /// Where everybody's token is.
    Positions(Vec<Token>),
    /// The game master told something; everyone reads it.
    Told(Turn),
    /// The game master is working on what somebody did.
    Thinking(String),
    /// The game master could not answer.
    Trouble(String),
    FightEvents(Vec<Event>),
    FightOver(FightSummary),
    /// The host runs another version of the game.
    WrongVersion,
}

pub struct Hosting {
    net: Host<Up, Down>,
    names: HashMap<PeerId, String>,
    tokens: HashMap<PeerId, (f32, f32)>,
    /// The world as last sent, to tell when it has changed.
    sent: String,
    since_world: f32,
    since_positions: f32,
}

pub struct Joined {
    net: Client<Up, Down>,
    since_position: f32,
}

/// How this instance takes part: alone, hosting, or as a guest in somebody's game.
#[derive(Resource, Default)]
pub enum Session {
    #[default]
    Solo,
    Host(Mutex<Hosting>),
    Client(Mutex<Joined>),
}

impl Session {
    pub fn is_client(&self) -> bool {
        matches!(self, Session::Client(_))
    }

    pub fn is_host(&self) -> bool {
        matches!(self, Session::Host(_))
    }

    /// Sends to the host; does nothing unless this is a guest.
    pub fn send(&self, message: Up) {
        if let Session::Client(joined) = self {
            joined.lock().unwrap().net.send(&message);
        }
    }

    pub fn intend(&self, intent: Intent) {
        self.send(Up::Intent(intent));
    }

    /// Sends to every guest; does nothing unless this is the host.
    pub fn broadcast(&self, message: &Down) {
        if let Session::Host(hosting) = self {
            hosting.lock().unwrap().net.broadcast(message);
        }
    }

    pub fn players(&self) -> Vec<String> {
        match self {
            Session::Host(hosting) => {
                let hosting = hosting.lock().unwrap();
                let mut names: Vec<String> = hosting.names.values().cloned().collect();
                names.sort();
                names
            }
            _ => Vec::new(),
        }
    }
}

/// What the whole table shares besides the world: what the master is busy with, the choices
/// last offered, tellings waiting to be read, and what the guests asked for.
#[derive(Resource, Default)]
pub struct Table {
    /// What the master was last asked on a guest's behalf and has not answered yet.
    pub busy: Option<String>,
    pub options: Vec<Action>,
    /// Tellings that arrived while this player was reading something else.
    pub unread: VecDeque<Turn>,
    /// Intents from guests, with the name of who sent each: the play screen carries them out.
    pub inbox: Vec<(String, Intent)>,
    /// A line for the player about the connection, shown on the lobby screen.
    pub status: Option<T>,
}

/// The name this player goes by: what they typed, or the account name.
pub fn player_name(settings: &Settings) -> String {
    if !settings.player_name.trim().is_empty() {
        return settings.player_name.trim().to_string();
    }
    std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "Player".to_string())
}

/// The name of the hero's owner as recorded in the world: empty when playing alone, so a
/// solo game's heroes belong to whoever loads the save.
pub fn owner_name(session: &Session, settings: &Settings) -> String {
    match session {
        Session::Solo => String::new(),
        _ => player_name(settings),
    }
}

/// Whether the hero acting in the fight is this player's to command.
pub fn my_turn(world: &World, session: &Session, settings: &Settings) -> bool {
    match (session, world.acting_owner()) {
        (Session::Solo, _) | (_, None) => true,
        (_, Some(owner)) => owner == player_name(settings),
    }
}

pub fn host(port: u16) -> std::io::Result<Session> {
    let net = Host::listen(("0.0.0.0", port))?;
    Ok(Session::Host(Mutex::new(Hosting { net, names: HashMap::new(), tokens: HashMap::new(), sent: String::new(), since_world: 0.0, since_positions: 0.0 })))
}

pub fn join(address: &str, name: &str) -> std::io::Result<Session> {
    let address = if address.contains(':') { address.to_string() } else { format!("{address}:{DEFAULT_PORT}") };
    let net = Client::connect(&address)?;
    net.send(&Up::Hello { name: name.to_string(), version: PROTOCOL });
    Ok(Session::Client(Mutex::new(Joined { net, since_position: 0.0 })))
}

/// The world as a guest should receive it: no tokens of other players in it (they travel
/// separately), and nothing that is only the host's.
fn snapshot(world: &World) -> World {
    let mut copy = world.clone();
    copy.others.clear();
    copy.level_ups.clear();
    copy
}

/// The host's side of the wire: who joined, who left, what they sent.
fn host_pump(time: Res<Time>, session: Res<Session>, settings: Res<Settings>, mut table: ResMut<Table>, mut game: Option<ResMut<Game>>, screen: Res<State<Screen>>) {
    let Session::Host(hosting) = &*session else { return };
    let mut hosting = hosting.lock().unwrap();
    for event in hosting.net.poll() {
        match event {
            HostEvent::Joined(_) => {}
            HostEvent::Left(peer) => {
                hosting.names.remove(&peer);
                hosting.tokens.remove(&peer);
            }
            HostEvent::Message(peer, Up::Hello { name, version }) => {
                if version != PROTOCOL {
                    hosting.net.send(peer, &Down::WrongVersion);
                    hosting.net.kick(peer);
                    continue;
                }
                // Two players with one name would share a hero; the later one gets a number.
                let taken: Vec<&String> = hosting.names.values().collect();
                let mut unique = name.trim().to_string();
                let mut n = 2;
                while unique.is_empty() || taken.contains(&&unique) || unique == player_name(&settings) {
                    unique = format!("{} {n}", name.trim());
                    n += 1;
                }
                hosting.names.insert(peer, unique);
                if let Some(game) = &game {
                    hosting.net.send(peer, &Down::World(Box::new(snapshot(&game.world))));
                }
            }
            HostEvent::Message(peer, Up::Position { x, y }) => {
                hosting.tokens.insert(peer, (x, y));
            }
            HostEvent::Message(peer, Up::Hero(hero)) => {
                let (Some(game), Some(name)) = (&mut game, hosting.names.get(&peer)) else { continue };
                let world = &mut game.world;
                let length = world.party.len();
                world.owners.resize(length, String::new());
                if !world.owners.contains(name) {
                    world.party.push(*hero);
                    world.owners.push(name.clone());
                }
            }
            HostEvent::Message(peer, Up::Intent(intent)) => {
                if let Some(name) = hosting.names.get(&peer) {
                    table.inbox.push((name.clone(), intent));
                }
            }
        }
    }
    let Some(game) = &mut game else { return };
    // Guests' tokens, for the host's own picture and for telling the other guests.
    let mut tokens: Vec<Token> = hosting.tokens.iter().filter_map(|(peer, (x, y))| Some(Token { name: hosting.names.get(peer)?.clone(), x: *x, y: *y })).collect();
    tokens.sort_by(|a, b| a.name.cmp(&b.name));
    if game.world.others != tokens {
        game.world.others = tokens.clone();
    }
    if *screen.get() != Screen::Play {
        return;
    }
    hosting.since_positions += time.delta_secs();
    if hosting.since_positions >= 0.1 {
        hosting.since_positions = 0.0;
        tokens.push(Token { name: player_name(&settings), x: game.world.player.x, y: game.world.player.y });
        hosting.net.broadcast(&Down::Positions(tokens));
    }
    hosting.since_world += time.delta_secs();
    if hosting.since_world >= 0.25 {
        hosting.since_world = 0.0;
        // Compared without the host's own token, which moves every frame and travels in
        // the positions message instead.
        let mut copy = snapshot(&game.world);
        let host_token = copy.player;
        copy.player = world::Player { x: 0.0, y: 0.0 };
        let encoded = serde_json::to_string(&copy).unwrap_or_default();
        if encoded != hosting.sent {
            hosting.sent = encoded;
            copy.player = host_token;
            hosting.net.broadcast(&Down::World(Box::new(copy)));
        }
    }
}

/// Takes a world from the host. The guest keeps its own token where it stands unless the
/// party has moved to another place.
fn adopt(world: World, game: &mut Option<ResMut<Game>>, commands: &mut Commands, next: &mut NextState<Screen>, screen: Screen, me: &str, redraw: &mut Redraw, phase: &mut Phase) {
    let mut world = world;
    match game {
        Some(game) => {
            let moved = game.world.current != world.current;
            if !moved {
                world.player = game.world.player;
            }
            world.others = std::mem::take(&mut game.world.others);
            let was_fighting = game.world.fight.is_some();
            game.world = world;
            redraw.0 = true;
            if moved {
                *phase = Phase::Arriving { t: 0.0 };
            } else if was_fighting && game.world.fight.is_none() && *phase == Phase::Fighting {
                // The fight ended on the host; the summary sheet closes it here.
            }
            // A guest who has no hero in this world yet makes one.
            if screen == Screen::Play && !game.world.owners.iter().any(|o| o == me) && game.world.party.len() == game.world.owners.len() {
                next.set(Screen::Hero);
            }
        }
        None => {
            let has_hero = world.owners.iter().any(|o| o == me);
            commands.insert_resource(Phase::Arriving { t: 0.0 });
            commands.insert_resource(Game { world, world_id: "guest".to_string() });
            next.set(if has_hero { Screen::Play } else { Screen::Hero });
        }
    }
}

/// The guest's side of the wire.
#[allow(clippy::too_many_arguments)]
fn client_pump(
    mut commands: Commands,
    time: Res<Time>,
    mut session: ResMut<Session>,
    settings: Res<Settings>,
    store: Res<Store>,
    mut table: ResMut<Table>,
    mut game: Option<ResMut<Game>>,
    mut phase: ResMut<Phase>,
    mut redraw: ResMut<Redraw>,
    mut view: ResMut<FightView>,
    screen: Res<State<Screen>>,
    mut next: ResMut<NextState<Screen>>,
) {
    let events = match &*session {
        Session::Client(joined) => {
            let mut joined = joined.lock().unwrap();
            joined.since_position += time.delta_secs();
            if let (true, Some(game)) = (joined.since_position >= 0.1 && *screen.get() == Screen::Play, &game) {
                joined.since_position = 0.0;
                joined.net.send(&Up::Position { x: game.world.player.x, y: game.world.player.y });
            }
            joined.net.poll()
        }
        _ => return,
    };
    let me = player_name(&settings);
    for event in events {
        match event {
            ClientEvent::Message(Down::World(world)) => adopt(*world, &mut game, &mut commands, &mut next, *screen.get(), &me, &mut redraw, &mut phase),
            ClientEvent::Message(Down::Positions(tokens)) => {
                if let Some(game) = &mut game {
                    game.world.others = tokens.into_iter().filter(|t| t.name != me).collect();
                }
            }
            ClientEvent::Message(Down::Told(turn)) => {
                table.options = turn.options.clone();
                play::show(turn, &mut phase, &mut table);
                redraw.0 = true;
            }
            ClientEvent::Message(Down::Thinking(_)) => {}
            ClientEvent::Message(Down::Trouble(_)) => {
                if matches!(*phase, Phase::Asking { .. }) {
                    *phase = Phase::Walking;
                }
            }
            ClientEvent::Message(Down::FightEvents(events)) => view.extend(events),
            ClientEvent::Message(Down::FightOver(summary)) => view.finish(summary),
            ClientEvent::Message(Down::WrongVersion) => {
                table.status = Some(T::LobbyWrongVersion);
                *session = Session::Solo;
                return;
            }
            ClientEvent::Disconnected => {
                // The host is gone. What this guest last saw is kept as a save of its own,
                // so the game can go on alone from here.
                if let Some(game) = &game {
                    let id = format!("guest-{:x}", crate::store::unix_now());
                    if let Err(e) = store.save_game(&id, &game.world, false) {
                        error!("could not save the game: {e}");
                    }
                }
                table.status = Some(T::LobbyLost);
                *session = Session::Solo;
                *phase = Phase::Walking;
                next.set(Screen::Lobby);
                return;
            }
        }
    }
}

#[derive(Component, Clone, Copy)]
enum LobbyAction {
    Host,
    Join,
    Leave,
}

#[derive(Component)]
struct NameField;

#[derive(Component)]
struct AddressField;

#[derive(Component)]
struct LobbyStatus;

fn field(parent: &mut ChildSpawnerCommands, art: &Art, fonts: &Fonts, initial: &str, marker: impl Bundle) {
    parent.spawn(Node { width: px(180), height: px(BUTTON_HEIGHT), padding: UiRect::axes(px(5), px(0)), align_items: AlignItems::Center, overflow: Overflow::clip(), flex_shrink: 0.0, ..default() }).with_children(|slot| {
        slot.spawn(backing(art.field.clone(), 4.0));
        slot.spawn((
            marker,
            AutoDirectionalNavigation::default(),
            Node { width: percent(100), ..default() },
            EditableText { allow_newlines: false, max_characters: Some(60), ..EditableText::new(initial) },
            TextLayout::no_wrap(),
            fonts.body(),
            TextColor(color(INK)),
            TextCursorStyle { color: color(INK), selection_color: color(BRASS_LIGHT), ..default() },
        ));
    });
}

fn spawn_lobby(mut commands: Commands, art: Res<Art>, fonts: Res<Fonts>, settings: Res<Settings>, session: Res<Session>) {
    let ink = TextColor(color(INK));
    let alone = matches!(*session, Session::Solo);
    commands.spawn(screen_root(Screen::Lobby)).with_children(|root| {
        let sheet = Node { margin: UiRect::top(px(30)), width: px(400), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() };
        spawn_panel(root, &art, sheet, |sheet| {
            sheet.spawn((Text::default(), T::Multiplayer, fonts.title(1), ink));
            sheet.spawn((Text::default(), T::LobbyHelp, fonts.body(), TextColor(color(FADED))));
            sheet.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() }).with_children(|line| {
                line.spawn((Node { width: px(90), flex_shrink: 0.0, ..default() }, Text::default(), T::LobbyName, fonts.body(), ink));
                field(line, &art, &fonts, &player_name(&settings), (NameField, AutoFocus));
            });
            sheet.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() }).with_children(|line| {
                line.spawn((Node { width: px(90), flex_shrink: 0.0, ..default() }, Text::default(), T::LobbyAddress, fonts.body(), ink));
                field(line, &art, &fonts, &format!("127.0.0.1:{DEFAULT_PORT}"), AddressField);
                button(line, &art, &fonts, T::LobbyJoin, alone, LobbyAction::Join);
            });
            sheet.spawn(Node { column_gap: px(6), ..default() }).with_children(|line| {
                button(line, &art, &fonts, T::LobbyHost, alone, LobbyAction::Host);
                button(line, &art, &fonts, T::LobbyLeave, !alone, LobbyAction::Leave);
            });
            sheet.spawn((Text::default(), LobbyStatus, fonts.body(), TextColor(color(WAX_DARK))));
            sheet.spawn(Node::default()).with_children(|slot| button(slot, &art, &fonts, T::Back, true, Nav::Back));
        });
    });
}

/// The address other machines on the same network can reach this one at.
fn local_address() -> String {
    // No packet is sent: connecting a datagram socket only asks the system which of this
    // machine's addresses would be used.
    std::net::UdpSocket::bind("0.0.0.0:0").and_then(|s| s.connect("192.0.2.1:9").and_then(|_| s.local_addr())).map(|a| a.ip().to_string()).unwrap_or_else(|_| "127.0.0.1".to_string())
}

fn lobby_status(session: Res<Session>, table: Res<Table>, settings: Res<Settings>, mut line: Query<&mut Text, With<LobbyStatus>>) {
    let lang = settings.lang;
    let text = match &*session {
        Session::Host(hosting) => {
            let port = hosting.lock().unwrap().net.port();
            let players = session.players();
            format!("{} {}:{port}\n{} {}", T::LobbyHosting.text(lang), local_address(), T::LobbyPlayers.text(lang), if players.is_empty() { "—".to_string() } else { players.join(", ") })
        }
        Session::Client(_) => T::LobbyJoined.text(lang).to_string(),
        Session::Solo => table.status.map_or(String::new(), |t| t.text(lang).to_string()),
    };
    for mut shown in &mut line {
        if shown.0 != text {
            shown.0 = text.clone();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn on_activate(
    used: On<Activate>,
    buttons: Query<&LobbyAction>,
    name: Query<&EditableText, (With<NameField>, Without<AddressField>)>,
    address: Query<&EditableText, (With<AddressField>, Without<NameField>)>,
    mut session: ResMut<Session>,
    mut settings: ResMut<Settings>,
    mut table: ResMut<Table>,
    store: Res<Store>,
    mut next: ResMut<NextState<Screen>>,
) {
    let Ok(action) = buttons.get(used.entity) else { return };
    if let Ok(name) = name.single() {
        let typed = name.value().to_string();
        if !typed.trim().is_empty() && typed.trim() != settings.player_name {
            settings.player_name = typed.trim().to_string();
            if let Err(e) = store.save_settings(&settings) {
                error!("could not save settings: {e}");
            }
        }
    }
    table.status = None;
    match action {
        LobbyAction::Host => match host(DEFAULT_PORT) {
            Ok(hosting) => *session = hosting,
            Err(e) => {
                error!("could not host: {e}");
                table.status = Some(T::LobbyHostFailed);
            }
        },
        LobbyAction::Join => {
            let typed = address.single().map(|a| a.value().to_string()).unwrap_or_default();
            match join(typed.trim(), &player_name(&settings)) {
                Ok(joined) => *session = joined,
                Err(e) => {
                    error!("could not join {typed}: {e}");
                    table.status = Some(T::LobbyJoinFailed);
                }
            }
        }
        LobbyAction::Leave => *session = Session::Solo,
    }
    // The buttons depend on the session: draw the screen again.
    next.set(Screen::Lobby);
}

/// Re-entering the same screen does not rebuild it, so the lobby redraws itself when the
/// session changes.
fn refresh_lobby(mut commands: Commands, session: Res<Session>, roots: Query<Entity, With<DespawnOnExit<Screen>>>, art: Res<Art>, fonts: Res<Fonts>, settings: Res<Settings>, screen: Res<State<Screen>>) {
    if !session.is_changed() || session.is_added() || *screen.get() != Screen::Lobby {
        return;
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    spawn_lobby(commands, art, fonts, settings, session);
}

pub struct CoopPlugin;

impl Plugin for CoopPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Session>()
            .init_resource::<Table>()
            .add_systems(OnEnter(Screen::Lobby), spawn_lobby)
            .add_systems(PreUpdate, (host_pump, client_pump))
            .add_systems(Update, (lobby_status, refresh_lobby).run_if(in_state(Screen::Lobby)))
            .add_observer(on_activate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with(owners: &[&str]) -> World {
        let draft: world::WorldDraft = serde_json::from_str(&world::fake::world("coop")).unwrap();
        let mut w = world::build_world(&draft, "coop", "English").unwrap().0;
        w.owners = owners.iter().map(|o| o.to_string()).collect();
        w
    }

    #[test]
    fn messages_survive_the_wire() {
        let up = Up::Intent(Intent::Fight(FightIntent::Attack { target: 2, weapon: 0 }));
        let back: Up = serde_json::from_str(&serde_json::to_string(&up).unwrap()).unwrap();
        assert!(matches!(back, Up::Intent(Intent::Fight(FightIntent::Attack { target: 2, weapon: 0 }))));
        let down = Down::World(Box::new(world_with(&["Ann"])));
        let Down::World(world) = serde_json::from_str::<Down>(&serde_json::to_string(&down).unwrap()).unwrap() else { panic!("expected a world") };
        assert_eq!(world.owners, ["Ann"]);
    }

    #[test]
    fn a_snapshot_leaves_out_what_is_only_the_hosts() {
        let mut w = world_with(&[]);
        w.others.push(Token { name: "Bob".into(), x: 1.0, y: 2.0 });
        assert!(snapshot(&w).others.is_empty());
    }

    #[test]
    fn alone_every_hero_is_mine_and_at_a_table_only_my_own() {
        let mut w = world_with(&["Ann"]);
        let skills = vec!["acrobatics".to_string(), "athletics".to_string()];
        w.party.push(Character::create("Brak", "", "human", "fighter", [15, 12, 14, 8, 10, 13], &skills, &[]).unwrap());
        w.here_mut().creatures[0].attitude = world::Attitude::Hostile;
        assert!(w.start_fight(rules::Rng::seeded(1)));
        let fight = w.fight.as_mut().unwrap();
        fight.turn = fight.order.iter().position(|i| *i == 0).unwrap();
        let ann = Settings { player_name: "Ann".into(), ..Settings::default() };
        let bob = Settings { player_name: "Bob".into(), ..Settings::default() };
        assert!(my_turn(&w, &Session::Solo, &bob));
        let table = host(0).unwrap();
        assert!(my_turn(&w, &table, &ann));
        assert!(!my_turn(&w, &table, &bob));
        assert_eq!(owner_name(&Session::Solo, &ann), "");
        assert_eq!(owner_name(&table, &ann), "Ann");
    }

    #[test]
    fn an_address_without_a_port_gets_the_usual_one() {
        // Nothing listens there: what matters is that the address was understood.
        let error = join("127.0.0.1", "Ann").err().expect("nobody is listening");
        assert_ne!(error.kind(), std::io::ErrorKind::InvalidInput);
    }
}
