//! Playing together. One player hosts and holds the world; the others send what they want
//! to do and are sent what happened. A game played alone is the same game with nobody
//! connected.

use crate::art::Art;
use crate::fight::FightView;
use crate::gm::{Gm, Trouble};
use crate::i18n::{Lang, T};
use crate::master::{Master, Relay, Relayed, Served, serve_for_host};
use crate::menu::{Nav, Screen, screen_root, spawn_panel};
use crate::play::{self, Game, Phase, Redraw};
use crate::store::{Settings, Store};
use crate::theme::*;
use crate::widgets::{Activate, BUTTON_HEIGHT, backing, button, text_button};
use bevy::input_focus::AutoFocus;
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigation;
use net::{Client, ClientEvent, Host, HostEvent, PeerId};
use rules::{Ability, Character, Event};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use world::{Action, CreatureCard, FightIntent, FightSummary, Token, Turn, TurnDraft, World};

/// Raised whenever the messages change shape; players with another number are turned away.
pub const PROTOCOL: u32 = 5;
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
    /// Something only the player in the master's seat may do.
    Gm(GmAct),
    /// One of the abilities offered to the player's hero.
    Feat(usize),
    /// Something tried in a fight that the rules have no button for; `spell` when it is a
    /// spell the rules cannot carry out, with the slot it takes.
    Stunt { said: String, spell: Option<(String, u8)> },
}

/// What the player in the master's seat does. A guest sends it and the host checks who it
/// came from; the host's own player goes through the same door.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum GmAct {
    /// Take the seat when it is empty, or give it back to the model.
    Seat(bool),
    /// The waiting draft is let through, told as the master left it, or thrown away.
    Verdict { accept: bool, narration: String },
    /// A direction for the model. Over a waiting draft it says how that answer is to be
    /// redone; otherwise the model makes a new turn of it. Either way a draft comes back.
    Direct(String),
    /// Changes made by hand: a telling, effects, a creature from the library.
    Apply(Box<TurnDraft>),
    Whisper { to: String, text: String },
    /// Ask the model for a creature for the master's library.
    Invent(String),
}

/// A draft of the model's as the master sees it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DraftView {
    /// What the players did, when the draft answers them.
    pub said: String,
    pub narration: String,
    pub effects: Vec<String>,
    pub options: Vec<String>,
}

/// A model's answer waiting for the human master's verdict.
pub struct Held {
    pub said: String,
    pub near: Option<String>,
    pub draft: TurnDraft,
    /// The guest it answers, when it happens where that guest is.
    pub scene: Option<String>,
}

impl Held {
    pub fn view(&self) -> DraftView {
        const SHOWN: usize = 110;
        let brief = |effect: &serde_json::Value| {
            let text = effect.to_string();
            if text.chars().count() > SHOWN { format!("{}…", text.chars().take(SHOWN).collect::<String>()) } else { text }
        };
        DraftView {
            said: self.said.clone(),
            narration: self.draft.narration.trim().to_string(),
            effects: self.draft.effects.iter().map(brief).collect(),
            options: self.draft.options.iter().map(|o| format!("{} — {}", o.label.trim(), o.outcome.trim())).collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Up {
    Hello { name: String, version: u32 },
    Position { x: f32, y: f32 },
    Intent(Intent),
    Hero(Box<Character>),
    /// Take a hero whose player is away.
    Claim(usize),
    /// What this guest's keys answered to a request the host handed over.
    Answer { id: u64, result: Served },
    /// Table talk: for the players, not for the game master.
    Chat(String),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Down {
    /// The name the guest goes by at this table.
    Welcome { name: String },
    /// The whole world, sent on joining and whenever it changes. `id` names its saves.
    World { id: String, world: Box<World> },
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
    /// A line for the fight's log that is not an event of the rules: how a stunt went.
    FightNote(String),
    /// The place this guest walked into is being drawn; the world follows when it is.
    Drawing(String),
    /// The host's keys are spent; this guest's keys are asked to serve a request.
    Ask(Relayed),
    Chat { from: String, text: String },
    /// For the player in the master's seat: a draft to judge, or word that there is none.
    Draft(Option<DraftView>),
    /// Words for this player alone.
    Whisper(String),
    /// A creature the model made for this player's library, or why it could not.
    Card(Result<CreatureCard, String>),
    /// The host runs another version of the game.
    WrongVersion,
}

pub struct Hosting {
    net: Host<Up, Down>,
    names: HashMap<PeerId, String>,
    tokens: HashMap<PeerId, (f32, f32)>,
    /// The world as last sent to each guest, to tell when it has changed. Each guest is
    /// sent the world as seen from the place that guest is in.
    sent: HashMap<PeerId, String>,
    since_world: f32,
    since_positions: f32,
    /// Requests handed to guests, with the guests already tried; the last one is working on it.
    relayed: HashMap<u64, (Relayed, Vec<PeerId>)>,
}

pub struct Joined {
    net: Client<Up, Down>,
    since_position: f32,
    /// Answers to the host's relayed requests, from the threads that fetched them.
    served: Arc<Mutex<Vec<(u64, Served)>>>,
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

    /// Sends to one guest, by name. `false` unless this is the host and the guest is there.
    pub fn send_to(&self, player: &str, message: &Down) -> bool {
        let Session::Host(hosting) = self else { return false };
        let hosting = hosting.lock().unwrap();
        hosting.names.iter().find(|(_, name)| name.as_str() == player).is_some_and(|(peer, _)| hosting.net.send(*peer, message))
    }

    /// Says something to the other players only; the game master does not hear it.
    pub fn chat(&self, table: &mut Table, me: &str, text: &str) {
        match self {
            Session::Solo => {}
            Session::Host(hosting) => {
                table.heard(me, text);
                hosting.lock().unwrap().net.broadcast(&Down::Chat { from: me.to_string(), text: text.to_string() });
            }
            Session::Client(joined) => {
                joined.lock().unwrap().net.send(&Up::Chat(text.to_string()));
            }
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
    /// The guest that question was asked for: the answer happens where that guest is.
    pub busy_for: Option<String>,
    /// The choices last offered in each place.
    pub options: HashMap<String, Vec<Action>>,
    /// The host is handling something in a place it is not in: what is told is not for
    /// the host's own screen.
    pub elsewhere: bool,
    /// Guests waiting for a place to be drawn: who, and where they are going.
    pub travellers: Vec<(String, String)>,
    /// Tellings that arrived while this player was reading something else.
    pub unread: VecDeque<Turn>,
    /// Intents from guests, with the name of who sent each: the play screen carries them out.
    pub inbox: Vec<(String, Intent)>,
    /// A line for the player about the connection, shown on the lobby screen.
    pub status: Option<T>,
    /// Table talk, oldest first.
    pub chat: Vec<ChatLine>,
    /// On the host: the model's answers a human master has yet to judge, oldest first.
    pub drafts: VecDeque<Held>,
    /// For the player in the master's seat: the draft to judge now.
    pub judging: Option<DraftView>,
    /// For the player in the master's seat: the last thing the panel has to say.
    pub master_note: String,
    /// This player's library of creatures, as last read from disk.
    pub cards: Vec<CreatureCard>,
}

pub struct ChatLine {
    pub from: String,
    pub text: String,
    /// Seconds since it was said.
    pub age: f32,
}

const CHAT_KEPT: usize = 50;

impl Table {
    /// Keeps a creature the model made for this player's library, and says so on the panel.
    pub fn keep_card(&mut self, store: &Store, card: Result<CreatureCard, String>, lang: Lang) {
        self.master_note = match card {
            Ok(card) => match store.save_card(&card) {
                Ok(()) => format!("{} {}", T::CardSaved.text(lang), card.name),
                Err(e) => e.to_string(),
            },
            Err(why) => why,
        };
        self.cards = store.load_cards();
    }

    pub fn heard(&mut self, from: &str, text: &str) {
        self.chat.push(ChatLine { from: from.to_string(), text: text.to_string(), age: 0.0 });
        let extra = self.chat.len().saturating_sub(CHAT_KEPT);
        self.chat.drain(..extra);
    }
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
    Ok(fresh_hosting(Host::listen(("0.0.0.0", port))?))
}

/// Steam, once the multiplayer screen has asked for it. Asking makes Steam show the player
/// as in a game, so it is not done before the player goes there.
#[derive(Resource, Default)]
pub struct SteamLink {
    tried: bool,
    steam: Option<net::Steam>,
    /// Friends hosting right now, as last looked up: their names and lobbies.
    friends: Vec<(String, u64)>,
    since_look: f32,
}

/// How often the list of friends hosting is looked up while the lobby screen is open.
const FRIENDS_LOOK_PERIOD: f32 = 3.0;

fn fresh_hosting(net: Host<Up, Down>) -> Session {
    Session::Host(Mutex::new(Hosting { net, names: HashMap::new(), tokens: HashMap::new(), sent: HashMap::new(), since_world: 0.0, since_positions: 0.0, relayed: HashMap::new() }))
}

/// Hosts in a Steam lobby that the player's friends can see.
pub fn host_steam(steam: &net::Steam) -> Session {
    fresh_hosting(Host::listen_steam(steam))
}

/// Joins a friend's Steam lobby.
pub fn join_steam(steam: &net::Steam, lobby: u64, name: &str) -> Session {
    let net = Client::connect_steam(steam, lobby);
    net.send(&Up::Hello { name: name.to_string(), version: PROTOCOL });
    Session::Client(Mutex::new(Joined { net, since_position: 0.0, served: Arc::default() }))
}

pub fn join(address: &str, name: &str) -> std::io::Result<Session> {
    let address = if address.contains(':') { address.to_string() } else { format!("{address}:{DEFAULT_PORT}") };
    let net = Client::connect(&address)?;
    net.send(&Up::Hello { name: name.to_string(), version: PROTOCOL });
    Ok(Session::Client(Mutex::new(Joined { net, since_position: 0.0, served: Arc::default() })))
}

/// The world as a guest should receive it: no tokens of other players in it (they travel
/// separately), and nothing that is only the host's.
fn snapshot(world: &World) -> World {
    let mut copy = world.clone();
    copy.others.clear();
    copy.level_ups.clear();
    copy
}

/// Hands a request the host's keys could not serve to the next guest not yet tried. With
/// nobody left to try the request fails.
fn pass_on(hosting: &mut Hosting, relay: &Relay, id: u64, last: Trouble) {
    let Hosting { relayed, names, net, .. } = hosting;
    let Some((asked, tried)) = relayed.get_mut(&id) else { return };
    let mut untried: Vec<PeerId> = names.keys().copied().filter(|p| !tried.contains(p)).collect();
    untried.sort();
    match untried.first() {
        Some(peer) => {
            tried.push(*peer);
            net.send(*peer, &Down::Ask(asked.clone()));
        }
        None => {
            relayed.remove(&id);
            relay.serve(id, Err(last));
        }
    }
}

/// The host's side of the wire: who joined, who left, what they sent.
#[allow(clippy::too_many_arguments)]
fn host_pump(time: Res<Time>, session: Res<Session>, settings: Res<Settings>, master: Res<Master>, mut table: ResMut<Table>, mut game: Option<ResMut<Game>>, screen: Res<State<Screen>>) {
    let Session::Host(hosting) = &*session else {
        master.relay.set_helpers(0);
        return;
    };
    let mut hosting = hosting.lock().unwrap();
    for event in hosting.net.poll() {
        match event {
            HostEvent::Joined(_) => {}
            HostEvent::Left(peer) => {
                hosting.names.remove(&peer);
                hosting.tokens.remove(&peer);
                hosting.sent.remove(&peer);
                // What the guest who left was working on goes to the next one.
                let orphaned: Vec<u64> = hosting.relayed.iter().filter(|(_, (_, tried))| tried.last() == Some(&peer)).map(|(id, _)| *id).collect();
                for id in orphaned {
                    pass_on(&mut hosting, &master.relay, id, Trouble::Network);
                }
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
                hosting.net.send(peer, &Down::Welcome { name: unique.clone() });
                hosting.names.insert(peer, unique);
                // The world itself follows from the pump below, as seen from this guest's place.
            }
            HostEvent::Message(peer, Up::Position { x, y }) => {
                hosting.tokens.insert(peer, (x, y));
            }
            HostEvent::Message(peer, Up::Hero(hero)) => {
                let (Some(game), Some(name)) = (&mut game, hosting.names.get(&peer)) else { continue };
                let world = &mut game.world;
                let length = world.party.len();
                world.owners.resize(length, String::new());
                // A player whose hero died lets go of it and brings a new one.
                if let Some(old) = world.owners.iter().position(|o| o == name)
                    && world.party[old].dead
                {
                    world.owners[old] = String::new();
                }
                if !world.owners.contains(name) {
                    world.party.push(*hero);
                    world.owners.push(name.clone());
                }
            }
            HostEvent::Message(peer, Up::Claim(hero)) => {
                if let (Some(game), Some(name)) = (&mut game, hosting.names.get(&peer)) {
                    game.world.claim(hero, name);
                }
            }
            HostEvent::Message(peer, Up::Intent(intent)) => {
                if let Some(name) = hosting.names.get(&peer) {
                    table.inbox.push((name.clone(), intent));
                }
            }
            HostEvent::Message(peer, Up::Answer { id, result }) => {
                // Only the guest the request is with may answer it.
                if hosting.relayed.get(&id).is_some_and(|(_, tried)| tried.last() == Some(&peer)) {
                    match result {
                        Ok(answer) => {
                            hosting.relayed.remove(&id);
                            master.relay.serve(id, Ok(answer));
                        }
                        Err(trouble) => {
                            info!("a guest's keys could not serve request {id}: {trouble:?}");
                            pass_on(&mut hosting, &master.relay, id, trouble);
                        }
                    }
                }
            }
            HostEvent::Message(peer, Up::Chat(text)) => {
                if let Some(name) = hosting.names.get(&peer).cloned() {
                    table.heard(&name, &text);
                    hosting.net.broadcast(&Down::Chat { from: name, text });
                }
            }
        }
    }
    master.relay.set_helpers(hosting.names.len());
    for asked in master.relay.take_asked() {
        let id = asked.id;
        hosting.relayed.insert(id, (asked, Vec::new()));
        pass_on(&mut hosting, &master.relay, id, Trouble::NoKeys);
    }
    let Some(game) = &mut game else { return };
    // Guests' tokens, for the host's own picture and for telling the other guests.
    // A guest is where it was last, or with the host when it has only just sat down: it
    // does not follow when the host walks on.
    for name in hosting.names.values() {
        if !game.world.places.get(name).is_some_and(|place| game.world.locations.contains_key(place)) {
            let here = game.world.current.clone();
            game.world.places.insert(name.clone(), here);
        }
    }
    let mut tokens: Vec<Token> = hosting.tokens.iter().filter_map(|(peer, (x, y))| {
        let name = hosting.names.get(peer)?.clone();
        let place = game.world.place_of(&name).to_string();
        Some(Token { name, x: *x, y: *y, place })
    }).collect();
    tokens.sort_by(|a, b| a.name.cmp(&b.name));
    if game.world.others != tokens {
        game.world.others = tokens.clone();
    }
    // While the host is still making a hero the guests already have the world and can make
    // theirs; tokens are only sent once the host is walking.
    let playing = *screen.get() == Screen::Play;
    if !playing && *screen.get() != Screen::Hero {
        return;
    }
    hosting.since_positions += time.delta_secs();
    if playing && hosting.since_positions >= 0.1 {
        hosting.since_positions = 0.0;
        tokens.push(Token { name: player_name(&settings), x: game.world.player.x, y: game.world.player.y, place: game.world.current.clone() });
        hosting.net.broadcast(&Down::Positions(tokens));
    }
    hosting.since_world += time.delta_secs();
    if hosting.since_world >= 0.25 {
        hosting.since_world = 0.0;
        // Each guest sees the world from the place it is in. Compared without a token,
        // which moves every frame and travels in the positions message instead.
        let guests: Vec<(PeerId, String)> = hosting.names.iter().map(|(peer, name)| (*peer, name.clone())).collect();
        let mut copy = snapshot(&game.world);
        let host_place = copy.current.clone();
        let host_token = copy.player;
        for (peer, name) in guests {
            copy.current = game.world.place_of(&name).to_string();
            copy.player = world::Player { x: 0.0, y: 0.0 };
            let encoded = serde_json::to_string(&copy).unwrap_or_default();
            if hosting.sent.get(&peer) != Some(&encoded) {
                hosting.sent.insert(peer, encoded);
                // Where a guest new to its place first stands: by the host, or where the
                // place lets people in.
                copy.player = if copy.current == host_place { host_token } else { copy.locations.get(&copy.current).map_or(host_token, |l| world::Player::in_cell(l.spawn.0, l.spawn.1)) };
                hosting.net.send(peer, &Down::World { id: game.world_id.clone(), world: Box::new(copy.clone()) });
            }
        }
    }
}

/// Who is at the table, so the world knows whose heroes take part. The host decides; a
/// guest is told with the world. Alone, a save from a shared table is played with one's own
/// hero, and a save in which no hero is one's own with all of them.
fn presence(session: Res<Session>, settings: Res<Settings>, mut game: Option<ResMut<Game>>, screen: Res<State<Screen>>, mut next: ResMut<NextState<Screen>>) {
    let Some(game) = &mut game else { return };
    let me = player_name(&settings);
    let mine = game.world.owners.iter().any(|o| *o == me);
    let mut present = match &*session {
        Session::Client(_) => return,
        Session::Host(_) => {
            let mut names = session.players();
            names.push(me.clone());
            names
        }
        Session::Solo if mine => vec![me.clone()],
        Session::Solo => Vec::new(),
    };
    present.sort();
    if game.world.present != present {
        game.world.present = present;
    }
    // A host who loads a table's save in which every hero is somebody else's needs one.
    let world = &game.world;
    if session.is_host() && *screen.get() == Screen::Play && !world.party.is_empty() && !mine && world.gm.as_deref() != Some(me.as_str()) && world.owners.len() == world.party.len() && world.owners.iter().all(|o| !o.is_empty()) {
        next.set(Screen::Hero);
    }
}

/// Takes a world from the host. The guest keeps its own token where it stands unless the
/// party has moved to another place.
#[allow(clippy::too_many_arguments)]
fn adopt(id: String, world: World, game: &mut Option<ResMut<Game>>, commands: &mut Commands, next: &mut NextState<Screen>, screen: Screen, me: &str, redraw: &mut Redraw, phase: &mut Phase, store: &Store) {
    let mut world = world;
    match game {
        Some(game) => {
            let moved = game.world.current != world.current;
            if moved {
                // Through the way out the guest took, to where it lets out on the other side.
                let (x, y) = world.arrival_cell(&world.current, &game.world.current);
                world.player = world::Player::in_cell(x, y);
            } else {
                world.player = game.world.player;
                // After a fight the guest walks on from where its hero ended it.
                if world.fight.is_none()
                    && let Some((x, y)) = game.world.fight_cell_of(me)
                {
                    world.player = world::Player::in_cell(x, y);
                }
            }
            world.others = std::mem::take(&mut game.world.others);
            game.world = world;
            game.world_id = id;
            redraw.0 = true;
            if moved {
                *phase = Phase::Arriving { t: 0.0 };
                // Every player keeps a copy of the game and can go on alone from it.
                if let Err(e) = store.save_game(&game.world_id, &game.world, false) {
                    error!("could not save the game: {e}");
                }
            }
            // A guest who has no hero in this world yet makes one, unless they run the game.
            if screen == Screen::Play && !game.world.owners.iter().any(|o| o == me) && game.world.gm.as_deref() != Some(me) && game.world.party.len() == game.world.owners.len() {
                next.set(Screen::Hero);
            }
        }
        None => {
            let has_hero = world.owners.iter().any(|o| o == me) || world.gm.as_deref() == Some(me);
            commands.insert_resource(Phase::Arriving { t: 0.0 });
            commands.insert_resource(Game { world, world_id: id });
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
    mut settings: ResMut<Settings>,
    store: Res<Store>,
    gm: Res<Gm>,
    master: Res<Master>,
    mut table: ResMut<Table>,
    mut game: Option<ResMut<Game>>,
    mut phase: ResMut<Phase>,
    mut redraw: ResMut<Redraw>,
    mut view: ResMut<FightView>,
    screen: Res<State<Screen>>,
    mut next: ResMut<NextState<Screen>>,
) {
    let (events, serving) = match &*session {
        Session::Client(joined) => {
            let mut joined = joined.lock().unwrap();
            joined.since_position += time.delta_secs();
            if let (true, Some(game)) = (joined.since_position >= 0.1 && *screen.get() == Screen::Play, &game) {
                joined.since_position = 0.0;
                joined.net.send(&Up::Position { x: game.world.player.x, y: game.world.player.y });
            }
            for (id, result) in std::mem::take(&mut *joined.served.lock().unwrap()) {
                joined.net.send(&Up::Answer { id, result });
            }
            (joined.net.poll(), joined.served.clone())
        }
        _ => return,
    };
    for event in events {
        let me = player_name(&settings);
        match event {
            ClientEvent::Message(Down::Welcome { name }) => {
                // The host may have numbered the name; the hero is recorded under what the
                // host calls this player.
                if name != me {
                    settings.player_name = name;
                }
            }
            ClientEvent::Message(Down::World { id, world }) => adopt(id, *world, &mut game, &mut commands, &mut next, *screen.get(), &me, &mut redraw, &mut phase, &store),
            ClientEvent::Message(Down::Positions(tokens)) => {
                if let Some(game) = &mut game {
                    game.world.others = tokens.into_iter().filter(|t| t.name != me).collect();
                }
            }
            ClientEvent::Message(Down::Told(turn)) => {
                play::show(turn, &mut phase, &mut table);
                redraw.0 = true;
            }
            ClientEvent::Message(Down::Thinking(_)) => {}
            ClientEvent::Message(Down::Trouble(_)) => {
                if matches!(*phase, Phase::Asking { .. } | Phase::Drawing { .. }) {
                    *phase = Phase::Walking;
                }
            }
            ClientEvent::Message(Down::Drawing(to)) => {
                if *phase == Phase::Walking {
                    *phase = Phase::Drawing { to };
                }
            }
            ClientEvent::Message(Down::FightEvents(events)) => view.extend(events),
            ClientEvent::Message(Down::FightOver(summary)) => view.finish(summary),
            ClientEvent::Message(Down::FightNote(line)) => view.note(line),
            ClientEvent::Message(Down::Ask(asked)) => {
                // The model may take minutes: the answer comes back from a thread of its own.
                let (gm, fake, serving) = (gm.clone(), master.fake, serving.clone());
                std::thread::spawn(move || {
                    let result = serve_for_host(&gm, &asked, fake);
                    serving.lock().unwrap().push((asked.id, result));
                });
            }
            ClientEvent::Message(Down::Chat { from, text }) => table.heard(&from, &text),
            ClientEvent::Message(Down::Draft(view)) => {
                match (&view, &*phase) {
                    (Some(_), Phase::Walking | Phase::Master) => *phase = Phase::Judging,
                    (None, Phase::Judging) => *phase = Phase::Walking,
                    _ => {}
                }
                table.judging = view;
            }
            ClientEvent::Message(Down::Whisper(text)) => {
                let turn = Turn { narration: format!("{} {text}", T::WhisperMark.text(settings.lang)), options: Vec::new() };
                play::show(turn, &mut phase, &mut table);
            }
            ClientEvent::Message(Down::Card(card)) => table.keep_card(&store, card, settings.lang),
            ClientEvent::Message(Down::WrongVersion) => {
                table.status = Some(T::LobbyWrongVersion);
                *session = Session::Solo;
                return;
            }
            ClientEvent::Disconnected => {
                // The host is gone. What this guest last saw is kept as a save of its own,
                // so the game can go on alone from here.
                if let Some(game) = &game
                    && let Err(e) = store.save_game(&game.world_id, &game.world, false)
                {
                    error!("could not save the game: {e}");
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
    HostSteam,
    /// Join the Steam lobby of the friend at this place in the list.
    JoinSteam(usize),
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

fn spawn_lobby(mut commands: Commands, art: Res<Art>, fonts: Res<Fonts>, settings: Res<Settings>, session: Res<Session>, mut link: ResMut<SteamLink>) {
    if !link.tried {
        link.tried = true;
        match net::Steam::init() {
            Ok(steam) => {
                link.friends = steam.friends_hosting();
                link.steam = Some(steam);
            }
            Err(e) => info!("Steam is not available: {e}"),
        }
    }
    build_lobby(&mut commands, &art, &fonts, &settings, &session, &link);
}

fn build_lobby(commands: &mut Commands, art: &Art, fonts: &Fonts, settings: &Settings, session: &Session, link: &SteamLink) {
    let ink = TextColor(color(INK));
    let faded = TextColor(color(FADED));
    let alone = matches!(*session, Session::Solo);
    commands.spawn(screen_root(Screen::Lobby)).with_children(|root| {
        let sheet = Node { margin: UiRect::top(px(8)), width: px(420), flex_direction: FlexDirection::Column, row_gap: px(5), ..default() };
        spawn_panel(root, art, sheet, |sheet| {
            sheet.spawn((Text::default(), T::Multiplayer, fonts.title(1), ink));
            sheet.spawn((Text::default(), T::LobbyHelp, fonts.body(), faded));
            sheet.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() }).with_children(|line| {
                line.spawn((Node { width: px(90), flex_shrink: 0.0, ..default() }, Text::default(), T::LobbyName, fonts.body(), ink));
                field(line, art, fonts, &player_name(settings), (NameField, AutoFocus));
            });
            // Steam first: friends find each other without addresses or open ports.
            if link.steam.is_some() {
                sheet.spawn(Node { column_gap: px(6), row_gap: px(2), flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, ..default() }).with_children(|line| {
                    button(line, art, fonts, T::SteamHost, alone, LobbyAction::HostSteam);
                    if link.friends.is_empty() {
                        line.spawn((Text::default(), T::SteamNoFriends, fonts.body(), faded));
                    } else {
                        line.spawn((Text::default(), T::SteamFriends, fonts.body(), ink));
                        for (n, (friend, _)) in link.friends.iter().enumerate() {
                            let joinable = PixelButtonState(alone);
                            text_button(line, art, fonts, friend.clone(), (LobbyAction::JoinSteam(n), joinable));
                        }
                    }
                });
            } else {
                sheet.spawn((Text::default(), T::SteamUnavailable, fonts.body(), faded));
            }
            sheet.spawn((Text::default(), T::DirectTitle, fonts.body(), faded));
            sheet.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() }).with_children(|line| {
                line.spawn((Node { width: px(90), flex_shrink: 0.0, ..default() }, Text::default(), T::LobbyAddress, fonts.body(), ink));
                field(line, art, fonts, &format!("127.0.0.1:{DEFAULT_PORT}"), AddressField);
                button(line, art, fonts, T::LobbyJoin, alone, LobbyAction::Join);
            });
            sheet.spawn(Node { column_gap: px(6), ..default() }).with_children(|line| {
                button(line, art, fonts, T::LobbyHost, alone, LobbyAction::Host);
                button(line, art, fonts, T::LobbyLeave, !alone, LobbyAction::Leave);
            });
            sheet.spawn((Text::default(), LobbyStatus, fonts.body(), TextColor(color(WAX_DARK))));
            sheet.spawn(Node::default()).with_children(|slot| button(slot, art, fonts, T::Back, true, Nav::Back));
        });
    });
}

/// Whether a friend's button may be pressed: only while not at a table already.
#[derive(Component)]
struct PixelButtonState(bool);

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
            let (port, lobby) = {
                let hosting = hosting.lock().unwrap();
                (hosting.net.port(), hosting.net.lobby())
            };
            let players = session.players();
            // A host with no port is in a Steam lobby.
            let how = match (port, lobby) {
                (0, None) => T::SteamOpening.text(lang).to_string(),
                (0, Some(Ok(_))) => T::SteamOpen.text(lang).to_string(),
                (0, Some(Err(why))) => format!("{} {why}", T::SteamRefused.text(lang)),
                _ => format!("{} {}:{port}", T::LobbyHosting.text(lang), local_address()),
            };
            format!("{how}\n{} {}", T::LobbyPlayers.text(lang), if players.is_empty() { "—".to_string() } else { players.join(", ") })
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
    link: Res<SteamLink>,
    friends: Query<&PixelButtonState>,
) {
    let Ok(action) = buttons.get(used.entity) else { return };
    // A friend's button is only live while this player is not at a table.
    if friends.get(used.entity).is_ok_and(|live| !live.0) {
        return;
    }
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
        LobbyAction::HostSteam => {
            if let Some(steam) = &link.steam {
                *session = host_steam(steam);
            }
        }
        LobbyAction::JoinSteam(n) => {
            if let (Some(steam), Some((_, lobby))) = (&link.steam, link.friends.get(*n)) {
                *session = join_steam(steam, *lobby, &player_name(&settings));
            }
        }
    }
    // The buttons depend on the session: draw the screen again.
    next.set(Screen::Lobby);
}

/// Re-entering the same screen does not rebuild it, so the lobby redraws itself when the
/// session changes or another friend starts or stops hosting.
#[allow(clippy::too_many_arguments)]
fn refresh_lobby(
    mut commands: Commands,
    time: Res<Time>,
    session: Res<Session>,
    mut link: ResMut<SteamLink>,
    roots: Query<Entity, With<DespawnOnExit<Screen>>>,
    art: Res<Art>,
    fonts: Res<Fonts>,
    settings: Res<Settings>,
) {
    link.since_look += time.delta_secs();
    let mut friends_changed = false;
    if link.since_look >= FRIENDS_LOOK_PERIOD {
        link.since_look = 0.0;
        if let Some(steam) = &link.steam {
            let now = steam.friends_hosting();
            friends_changed = now != link.friends;
            link.friends = now;
        }
    }
    if !(friends_changed || session.is_changed() && !session.is_added()) {
        return;
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    build_lobby(&mut commands, &art, &fonts, &settings, &session, &link);
}

pub struct CoopPlugin;

impl Plugin for CoopPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Session>()
            .init_resource::<Table>()
            .init_resource::<SteamLink>()
            .add_systems(OnEnter(Screen::Lobby), spawn_lobby)
            .add_systems(PreUpdate, (host_pump, client_pump, presence).chain())
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
        let down = Down::World { id: "w1".into(), world: Box::new(world_with(&["Ann"])) };
        let Down::World { id, world } = serde_json::from_str::<Down>(&serde_json::to_string(&down).unwrap()).unwrap() else { panic!("expected a world") };
        assert_eq!((id.as_str(), &world.owners), ("w1", &vec!["Ann".to_string()]));
        let answer = Up::Answer { id: 7, result: Err(Trouble::Exhausted { retry_at: 5 }) };
        let back: Up = serde_json::from_str(&serde_json::to_string(&answer).unwrap()).unwrap();
        assert!(matches!(back, Up::Answer { id: 7, result: Err(Trouble::Exhausted { retry_at: 5 }) }));
    }

    #[test]
    fn a_snapshot_leaves_out_what_is_only_the_hosts() {
        let mut w = world_with(&[]);
        w.others.push(Token { name: "Bob".into(), x: 1.0, y: 2.0, place: String::new() });
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
