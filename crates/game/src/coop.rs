//! Playing together. One player hosts and holds the world; the others send what they want
//! to do and are sent what happened. A game played alone is the same game with nobody
//! connected.

use crate::fight::FightView;
use crate::gm::{Gm, Trouble};
use crate::i18n::{Lang, T};
use crate::master::{Master, Relay, Relayed, Served, serve_for_host};
use crate::menu::{ReturnTo, Screen};
use crate::play::{self, Game, Phase, Redraw};
use crate::store::{Settings, Store};
use crate::theme::*;
use crate::ui::egui::{self, Key, RichText, vec2};
use crate::ui::kit::{self, Sheet, button, hint};
use crate::ui::theme::c;
use bevy::prelude::*;
use net::{Client, ClientEvent, Host, HostEvent, PeerId};
use rules::{Ability, Character, Event};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use world::{Action, CreatureCard, FightIntent, FightSummary, Token, Turn, TurnDraft, World};

/// Raised whenever the messages change shape; players with another number are turned away.
pub const PROTOCOL: u32 = 10;
pub const DEFAULT_PORT: u16 = 47_800;

/// Something a player wants to do that changes the world. A client sends it; the host does
/// it, exactly as it does for its own player.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Intent {
    /// A prepared action of a thing.
    Do { thing: String, index: usize },
    /// The party member's own words, for the game master.
    Say { text: String, near: Option<String> },
    /// One of the choices offered with a telling: its place in the list as the player saw
    /// it, and its words. The words are what it is found by: the list on the host may have
    /// changed since the player read it.
    Choose { index: usize, label: String },
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
    /// Turn the hero's next levels towards the next class it may take.
    Advance,
    /// Something tried in a fight that the rules have no button for; `spell` when it is a
    /// spell the rules cannot carry out, with the slot it takes.
    Stunt { said: String, spell: Option<(String, u8)> },
    /// A thing moved in the bag, or put on or taken off a hero.
    Arrange { held: world::bag::Held, to: world::bag::Spot },
    /// One of a thing in the bag used.
    UseItem { index: usize },
    /// A shop is being looked into: the host lays out the day's wares.
    Shop { keeper: String },
    /// One of a ware bought from a keeper here.
    Buy { keeper: String, ware: usize },
    /// One of a thing of one's own sold to a keeper here.
    Sell { keeper: String, index: usize },
    /// A try at a better price.
    Haggle { keeper: String },
    /// A weapon or armour handed to a companion out of one's bag, or taken back.
    ArmCompanion { companion: usize, index: usize },
    DisarmCompanion { companion: usize, weapon: bool },
    /// A thing put into a companion's pack, or taken out of it.
    PackCompanion { companion: usize, index: usize },
    UnpackCompanion { companion: usize, item: usize },
    /// Coins laid on the ground of a cell.
    DropGold { amount: u32, cell: (usize, usize) },
    /// A thing put on a quick slot, by name; an empty name clears the slot.
    Quick { slot: usize, name: String },
    /// One of a thing in the bag thrown on the ground of a cell.
    Drop { index: usize, cell: (usize, usize) },
    /// What lies on the ground picked up.
    PickUp { n: usize },
    /// A container opened, picked or forced.
    OpenChest { prop: String, how: world::bag::Opening },
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
    /// Someone in the master's place says something aloud, as a bubble.
    Say { who: String, text: String },
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
    Hello {
        name: String,
        version: u32,
        /// Who this player is, whatever the name: one who comes back gets the same seat.
        #[serde(default)]
        player: u64,
        /// The last thing told to this player that it read, after a drop; 0 when new.
        #[serde(default)]
        last_seq: u64,
    },
    /// Ready to begin, or no longer.
    Ready(bool),
    /// The world as this guest has it went wrong: the whole of it again, please.
    Resync,
    Position {
        x: f32,
        y: f32,
        /// Which way the hero looks, so the master knows what is in front of this player.
        #[serde(default)]
        facing: (i32, i32),
        /// How fast the hero is going, in pixels a second.
        #[serde(default)]
        vx: f32,
        #[serde(default)]
        vy: f32,
    },
    Intent(Intent),
    Hero(Box<Character>),
    /// Take a hero whose player is away.
    Claim(usize),
    /// What this guest's keys answered to a request the host handed over.
    Answer { id: u64, result: Served },
    /// Table talk: for the players, not for the game master.
    Chat(String),
    /// What this guest chose when the keys were spent: to enter a key, or to wait.
    KeysVote { entered: bool },
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Down {
    /// The name the guest goes by at this table.
    Welcome { name: String },
    /// Everyone at the table, the host first, and how the table is reached.
    Roster(Roster),
    /// The host took this player off the table.
    Kicked,
    /// Every seat at the table is taken.
    Full,
    /// The first words of what was told in a place this player is not in.
    News { place: String, text: String },
    /// Something told to this player, numbered in the player's own stream: one that comes
    /// back after a drop names the last it read, and is sent what it missed.
    Feed { seq: u64, message: Box<Down> },
    /// Someone in a place says something aloud: the place, who, the words.
    Bubble { place: String, who: String, text: String },
    /// The parts of the world that changed since the guest last had them: each by its key
    /// ("story", "locations/camp", "sprites/cat"), and the keys gone.
    Patch { id: String, parts: Vec<(String, serde_json::Value)>, removed: Vec<String> },
    /// The whole world, sent on joining and whenever it changes. `id` names its saves.
    World { id: String, world: Box<World> },
    /// Where everybody's token is.
    Positions(Vec<Token>),
    /// The game master told something; everyone reads it.
    Told(Turn),
    /// The story put the guest's hero in this cell of where the guest is.
    MoveTo { x: usize, y: usize },
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
    /// A die was rolled where this guest is, and came up so.
    Die(u32),
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
    /// No key at the table answers: everyone is asked to wait or to enter one. With who has
    /// not decided yet.
    KeysOut { waiting: Vec<String> },
    /// Everyone has decided.
    KeysOver,
    /// The table waits for the quota: seconds until the master asks again, none when it has.
    KeysWait(Option<u64>),
}

/// A seat at the table as the room shows it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Seat {
    pub name: String,
    /// The hero's name and level, empty while there is none.
    pub hero: String,
    pub ready: bool,
    pub host: bool,
}

/// Who sits at the table and how it is reached, as the host tells it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Roster {
    pub seats: Vec<Seat>,
    /// Friends reach it through Steam.
    pub steam: bool,
    /// The table's code for one network, and the networks it is on with their codes.
    pub codes: Vec<(String, String)>,
    /// The world played there, empty while none is chosen.
    pub world: String,
    /// Whether the game has begun.
    pub playing: bool,
    /// Whether friends across the internet can come in by the code: none while the router
    /// is being asked.
    pub internet: Option<bool>,
    /// Seconds for a turn in a fight here; 0 for no limit.
    #[serde(default)]
    pub turn_timer: u16,
}

/// The keys whose parts are sent one by one: each place, each picture.
const SPLIT: [&str; 2] = ["locations", "sprites"];

/// A world cut into parts: every field on its own, and each place and picture apart; each
/// with a hash of what it comes to and the part itself.
fn parts_of(world: &serde_json::Value) -> std::collections::BTreeMap<String, (u64, serde_json::Value)> {
    use std::hash::{Hash, Hasher};
    let hash = |v: &serde_json::Value| {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        v.to_string().hash(&mut h);
        h.finish()
    };
    let mut parts = std::collections::BTreeMap::new();
    let Some(fields) = world.as_object() else { return parts };
    for (key, value) in fields {
        match (SPLIT.contains(&key.as_str()), value.as_object()) {
            (true, Some(entries)) => {
                for (id, entry) in entries {
                    parts.insert(format!("{key}/{id}"), (hash(entry), entry.clone()));
                }
            }
            _ => {
                parts.insert(key.clone(), (hash(value), value.clone()));
            }
        }
    }
    parts
}

/// A world with the parts that changed put in and those gone taken out; none when it does
/// not come out a world.
fn patch_world(world: &World, parts: &[(String, serde_json::Value)], removed: &[String]) -> Option<World> {
    let mut value = serde_json::to_value(world).ok()?;
    let fields = value.as_object_mut()?;
    for (key, part) in parts {
        match key.split_once('/') {
            Some((field, id)) => {
                fields.entry(field.to_string()).or_insert_with(|| serde_json::json!({})).as_object_mut()?.insert(id.to_string(), part.clone());
            }
            None => {
                fields.insert(key.clone(), part.clone());
            }
        }
    }
    for key in removed {
        match key.split_once('/') {
            Some((field, id)) => {
                fields.get_mut(field).and_then(|f| f.as_object_mut()).map(|f| f.remove(id));
            }
            None => {
                fields.remove(key);
            }
        }
    }
    serde_json::from_value(value).ok()
}

/// What was told to a player after the last thing they read, in order, to send again.
fn missed_since(hosting: &Hosting, player: u64, last_seq: u64) -> Vec<Down> {
    hosting.feeds.get(&player).map(|(_, kept)| kept.iter().filter(|(seq, _)| *seq > last_seq).filter_map(|(_, json)| serde_json::from_str(json).ok()).collect()).unwrap_or_default()
}

/// What came, with the numbered things taken out of their wrapping: each is read once and
/// in order, and one already read (sent again after a drop) is left out.
fn unwrap_feed(events: Vec<ClientEvent<Down>>, seen: &mut u64) -> Vec<ClientEvent<Down>> {
    events
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Message(Down::Feed { seq, message }) => {
                if seq <= *seen {
                    return None;
                }
                *seen = seq;
                Some(ClientEvent::Message(*message))
            }
            other => Some(other),
        })
        .collect()
}

/// How many things told to a player are kept to send again after a drop.
const FEED_KEPT: usize = 200;

/// How often where everybody is goes over the wire: twenty times a second, so another
/// player's figure walks smoothly between the words of where it is.
const POSITIONS_EVERY: f32 = 0.05;

/// The most players at a table, the host among them; a master of flesh and blood sits apart.
pub const SEATS: usize = 5;

pub struct Hosting {
    net: Host<Up, Down>,
    /// Who each guest is, whatever the name.
    ids: HashMap<PeerId, u64>,
    /// The guests who said they are ready.
    ready: HashMap<PeerId, bool>,
    /// Says where the table is to every network this machine is on; none for Steam alone.
    beacon: Option<net::beacon::Beacon>,
    /// The port held open on the home router for friends across the internet.
    upnp: Option<net::upnp::Upnp>,
    /// What was told to each player, by who they are: the last number given, and the
    /// latest things told, kept to be sent again after a drop.
    feeds: HashMap<u64, (u64, VecDeque<(u64, String)>)>,
    names: HashMap<PeerId, String>,
    tokens: HashMap<PeerId, (f32, f32, (i32, i32), (f32, f32))>,
    /// Where the host's own hero was when last told, to tell how fast it goes.
    host_last: (f32, f32),
    /// The world as last sent to each guest, to tell when it has changed. Each guest is
    /// sent the world as seen from the place that guest is in.
    sent: HashMap<PeerId, String>,
    /// The parts of the world each guest has, by the world and place they were sent for,
    /// with what each part came to.
    parts: HashMap<PeerId, ((String, String), HashMap<String, u64>)>,
    since_world: f32,
    since_positions: f32,
    /// Requests handed to guests, with the guests already tried; the last one is working on it.
    relayed: HashMap<u64, (Relayed, Vec<PeerId>)>,
}

/// How many times a dropped table is tried again, a few seconds apart, before giving up.
pub const REJOIN_TRIES: u32 = 20;
const REJOIN_EVERY: f32 = 3.0;

/// A table being reached again after the line dropped.
pub struct Rejoin {
    pub way: Way,
    pub tries: u32,
    since: f32,
    /// A connection being made by address, on a thread of its own: it may take seconds.
    trying: Option<Arc<Mutex<Option<std::io::Result<Session>>>>>,
}

/// Tries the table again every few seconds after the line dropped, until it answers or the
/// tries run out.
fn rejoin(time: Res<Time>, mut session: ResMut<Session>, mut table: ResMut<Table>, settings: Res<Settings>, link: Res<SteamLink>) {
    if !matches!(*session, Session::Solo) {
        return;
    }
    let Some(rejoin) = &mut table.rejoin else { return };
    if let Some(slot) = &rejoin.trying {
        let Some(result) = slot.lock().unwrap().take() else { return };
        rejoin.trying = None;
        if let Ok(joined) = result {
            *session = joined;
            return;
        }
    }
    rejoin.since += time.delta_secs();
    if rejoin.since < REJOIN_EVERY {
        return;
    }
    rejoin.since = 0.0;
    rejoin.tries += 1;
    if rejoin.tries > REJOIN_TRIES {
        table.rejoin = None;
        table.lost = true;
        table.status = Some(T::LobbyLost);
        return;
    }
    let (name, player, seen) = (player_name(&settings), settings.player_id, table.feed_seen);
    let Some(rejoin) = &mut table.rejoin else { return };
    match rejoin.way.clone() {
        Way::Address(address) => {
            let slot: Arc<Mutex<Option<std::io::Result<Session>>>> = Arc::default();
            rejoin.trying = Some(slot.clone());
            std::thread::spawn(move || {
                let result = join(&address, &name, player, seen);
                *slot.lock().unwrap() = Some(result);
            });
        }
        Way::Steam(lobby) => {
            if let Some(steam) = &link.steam {
                *session = join_steam(steam, lobby, &name, player, seen);
            }
        }
    }
}

/// How a guest reached the table, to reach it again when the line drops.
#[derive(Clone, PartialEq, Debug)]
pub enum Way {
    Address(String),
    Steam(u64),
}

pub struct Joined {
    net: Client<Up, Down>,
    way: Way,
    /// Where the hero was when last told, to tell how fast it goes.
    last_sent: (f32, f32),
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

    /// Takes a guest off the table, by name; does nothing unless this is the host.
    pub fn kick(&self, player: &str) {
        let Session::Host(hosting) = self else { return };
        let hosting = hosting.lock().unwrap();
        if let Some(peer) = hosting.names.iter().find(|(_, name)| name.as_str() == player).map(|(peer, _)| *peer) {
            hosting.net.send(peer, &Down::Kicked);
            hosting.net.kick(peer);
        }
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

    /// Tells one guest something that must reach them: numbered in their own stream and kept,
    /// so that after a drop it is sent again. `false` unless this is the host and the guest
    /// is there now; kept all the same for when they come back.
    pub fn tell(&self, player: &str, message: Down) -> bool {
        let Session::Host(hosting) = self else { return false };
        let mut hosting = hosting.lock().unwrap();
        let peer = hosting.names.iter().find(|(_, name)| name.as_str() == player).map(|(peer, _)| *peer);
        let Some(id) = peer.and_then(|p| hosting.ids.get(&p).copied()).filter(|id| *id != 0) else {
            return peer.is_some_and(|p| hosting.net.send(p, &message));
        };
        let feed = hosting.feeds.entry(id).or_insert_with(|| (0, VecDeque::new()));
        feed.0 += 1;
        let seq = feed.0;
        let numbered = Down::Feed { seq, message: Box::new(message) };
        feed.1.push_back((seq, serde_json::to_string(&numbered).unwrap_or_default()));
        while feed.1.len() > FEED_KEPT {
            feed.1.pop_front();
        }
        peer.is_some_and(|p| hosting.net.send(p, &numbered))
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
    /// Everyone at the table as the host last told it, and whether it is a Steam table.
    pub roster: Roster,
    /// Things the people of a place will say aloud, in the order they will be said, and the
    /// place they belong to: asked for once, shown one by one.
    pub chatter: (String, std::collections::VecDeque<(String, String)>),
    /// Something hostile the dice said is coming and the master never drew, and the cell it
    /// comes to: the master is asked to draw it and bring it in.
    pub ambush: Option<(world::Wanderer, (usize, usize))>,
    /// What someone says right now, to be put up as a bubble: who, and the words.
    pub bubbles: Vec<(String, String)>,
    /// What other players have just set about, a line each, shown in the corner for a while.
    pub notices: Vec<String>,
    /// The outcome of a choice, shown while the master works out what follows it.
    pub going_on: Option<String>,
    /// The keys are spent and the table is deciding what to do.
    pub keys_vote: Option<KeysVote>,
    /// The table chose to wait: seconds until the master asks again.
    pub keys_wait: Option<u64>,
    /// What players did that needs the master's word, in the order it came, not yet put in
    /// the queues: the play screen puts it there.
    pub wants: Mutex<Vec<Want>>,
    /// Who waits for the master, place by place.
    pub queues: table::Queues,
    /// A direction of the human master's that the model is carrying out.
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
    /// The last numbered thing this guest read from the host.
    pub feed_seen: u64,
    /// The cells the story is walking this guest's hero along, the next one first.
    pub led: Vec<(usize, usize)>,
    /// The line to the host dropped and the table is being tried again.
    pub rejoin: Option<Rejoin>,
    /// The table could not be reached again: the game goes on alone.
    pub lost: bool,
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

/// Something a player did that the master must answer.
pub struct Want {
    pub player: String,
    pub said: String,
    pub near: Option<String>,
    /// A choice whose outcome is already told: the master is asked what follows.
    pub follows: bool,
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

/// Hosts on the first free port of the game's, said to every network by a beacon, and in a
/// Steam lobby as well when Steam is there.
pub fn host(steam: Option<&net::Steam>, name: &str, port: Option<u16>) -> std::io::Result<Session> {
    let listener = match port {
        Some(port) => std::net::TcpListener::bind(("0.0.0.0", port))?,
        None => net::free_listener()?,
    };
    let port = listener.local_addr()?.port();
    let net = match steam {
        Some(steam) => Host::listen_both(steam, listener)?,
        None => Host::serve(listener)?,
    };
    let info = net::beacon::TableInfo { protocol: PROTOCOL, name: name.to_string(), port, taken: 1, seats: SEATS as u8, world: String::new(), playing: false };
    let session = fresh_hosting(net, Some(net::beacon::Beacon::start(info)));
    // The router is asked to let friends across the internet in as well.
    if let (Session::Host(hosting), Some(local)) = (&session, net::beacon::networks().into_iter().find(|n| n.kind() == net::beacon::NetworkKind::Local).map(|n| n.address)) {
        hosting.lock().unwrap().upnp = Some(net::upnp::Upnp::open(local, port));
    }
    Ok(session)
}

/// The codes of a table on this port, one for each network this machine is on, named for
/// the player: the home network, Radmin, ZeroTier.
pub fn table_codes(port: u16, lang: crate::i18n::Lang) -> Vec<(String, String)> {
    use net::beacon::NetworkKind;
    net::beacon::networks()
        .into_iter()
        .map(|network| {
            let name = match network.kind() {
                NetworkKind::Local => T::NetLocal.text(lang),
                NetworkKind::Radmin => "Radmin VPN",
                NetworkKind::ZeroTier => "ZeroTier",
                NetworkKind::Hamachi => "Hamachi",
            };
            (format!("{name} {}", network.address), net::code::encode(std::net::SocketAddrV4::new(network.address, port)))
        })
        .collect()
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
    /// Friends who are online, to be asked to the table one by one, and those asked already.
    online: Vec<(String, u64)>,
    asked: Vec<u64>,
}

/// Opens Steam's overlay to invite friends to the table being hosted in a Steam lobby.
pub fn invite_friends(link: &SteamLink, session: &Session) {
    if let (Some(steam), Session::Host(hosting)) = (&link.steam, session)
        && let Some(Ok(lobby)) = hosting.lock().unwrap().net.lobby()
    {
        steam.invite(lobby);
    }
}

/// How often the list of friends hosting is looked up while the lobby screen is open.
const FRIENDS_LOOK_PERIOD: f32 = 3.0;

fn fresh_hosting(net: Host<Up, Down>, beacon: Option<net::beacon::Beacon>) -> Session {
    Session::Host(Mutex::new(Hosting { net, ids: HashMap::new(), ready: HashMap::new(), beacon, upnp: None, parts: HashMap::new(), feeds: HashMap::new(), host_last: (0.0, 0.0), names: HashMap::new(), tokens: HashMap::new(), sent: HashMap::new(), since_world: 0.0, since_positions: 0.0, relayed: HashMap::new() }))
}

/// Joins a friend's Steam lobby.
pub fn join_steam(steam: &net::Steam, lobby: u64, name: &str, player: u64, last_seq: u64) -> Session {
    let net = Client::connect_steam(steam, lobby);
    net.send(&Up::Hello { name: name.to_string(), version: PROTOCOL, player, last_seq });
    Session::Client(Mutex::new(Joined { net, since_position: 0.0, served: Arc::default(), way: Way::Steam(lobby), last_sent: (0.0, 0.0) }))
}

/// Joins a table by its code or its address, with or without a port.
pub fn join(typed: &str, name: &str, player: u64, last_seq: u64) -> std::io::Result<Session> {
    let typed = typed.trim();
    let address = if net::code::looks_like_code(typed) {
        match net::code::decode(typed) {
            Ok(address) => address.to_string(),
            Err(_) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "the code has a typo")),
        }
    } else if typed.contains(':') {
        typed.to_string()
    } else {
        format!("{typed}:{DEFAULT_PORT}")
    };
    let net = Client::connect(&address)?;
    net.send(&Up::Hello { name: name.to_string(), version: PROTOCOL, player, last_seq });
    Ok(Session::Client(Mutex::new(Joined { net, since_position: 0.0, served: Arc::default(), way: Way::Address(address), last_sent: (0.0, 0.0) })))
}

/// The world as a guest should receive it: no tokens of other players in it (they travel
/// separately), and nothing that is only the host's.
fn snapshot(world: &World) -> World {
    let mut copy = world.clone();
    copy.others.clear();
    copy.level_ups.clear();
    copy
}

/// The table deciding what to do when no key answers.
#[derive(Default, Clone, PartialEq)]
pub struct KeysVote {
    /// What this player chose: a key entered, or to wait.
    pub chosen: Option<bool>,
    /// This player went to enter a key and has not come back yet.
    pub entering: bool,
    /// Who has not decided yet.
    pub waiting: Vec<String>,
    /// On the host: what each player chose.
    votes: HashMap<String, bool>,
}

/// The table deciding what to do about spent keys, on egui, wherever the player is: a sheet
/// in the middle of the screen while they decide, a line at the top while the table waits
/// for the quota. The keys' own screens are not covered by it.
#[allow(clippy::too_many_arguments)]
fn keys_vote(
    mut egui: bevy_egui::EguiContexts,
    lang: Res<Lang>,
    screen: Res<State<Screen>>,
    session: Res<Session>,
    settings: Res<Settings>,
    mut table: ResMut<Table>,
    mut return_to: ResMut<crate::menu::ReturnTo>,
    mut next: ResMut<NextState<Screen>>,
    mut at_the_keys: Local<bool>,
) {
    use crate::ui::egui::vec2;
    use crate::ui::kit::{self, Sheet, button, hint};
    if matches!(*session, Session::Solo) && table.keys_wait.is_some() {
        table.keys_wait = None;
    }
    let keys = matches!(screen.get(), Screen::Keys | Screen::Provider);
    let entering = table.keys_vote.as_ref().is_some_and(|v| v.entering);
    *at_the_keys = entering && (*at_the_keys || keys);
    // Back from the keys: that was this player's choice.
    if let Some(vote) = table.keys_vote.as_mut().filter(|_| entering && *at_the_keys && !keys) {
        vote.entering = false;
        vote.chosen = Some(true);
        session.send(Up::KeysVote { entered: true });
    }
    let Ok(ctx) = egui.ctx_mut() else { return };
    if !kit::ready(ctx) {
        return;
    }
    let t = |text: T| text.text(*lang);
    if let Some(left) = table.keys_wait {
        crate::ui::egui::Area::new(crate::ui::egui::Id::new("keys_wait")).anchor(crate::ui::egui::Align2::CENTER_TOP, vec2(0.0, 30.0)).interactable(false).show(ctx, |ui| {
            crate::ui::egui::Frame::new().fill(crate::ui::egui::Color32::from_rgba_unmultiplied(0x10, 0x0a, 0x08, 0xe8)).inner_margin(crate::ui::egui::Margin::symmetric(6, 3)).show(ui, |ui| {
                ui.label(crate::ui::egui::RichText::new(format!("{} {left} {}", t(T::KeysOutAgain), t(T::KeysOutSeconds))).color(crate::ui::theme::c(BRASS_LIGHT)));
            });
        });
    }
    let Some(vote) = table.keys_vote.clone().filter(|v| !keys && !v.entering) else { return };
    let mut chose = None;
    crate::ui::egui::Area::new(crate::ui::egui::Id::new("keys_vote")).order(crate::ui::egui::Order::Foreground).fixed_pos(ctx.content_rect().min).show(ctx, |ui| {
        ui.set_min_size(ctx.content_rect().size());
        let paper = Sheet { id: "keys_vote", title: t(T::KeysOutTitle), about: "", max: vec2(320.0, 200.0), top: 90.0, fixed: false };
        kit::sheet(
            ui,
            paper,
            |ui| {
                ui.add(crate::ui::egui::Label::new(t(T::KeysOutText)).wrap());
                if vote.chosen.is_some() {
                    hint(ui, t(T::KeysOutChosen));
                }
                if !vote.waiting.is_empty() {
                    hint(ui, format!("{} {}", t(T::KeysOutWaiting), vote.waiting.join(", ")));
                }
            },
            |footer| {
                if vote.chosen.is_none() {
                    if footer.add(button(t(T::KeysOutWait))).clicked() {
                        chose = Some(false);
                    }
                    if kit::right(footer, |r| r.add(button(t(T::KeysOutEnter)).primary())).clicked() {
                        chose = Some(true);
                    }
                }
            },
        );
    });
    let Some(vote) = table.keys_vote.as_mut() else { return };
    match chose {
        Some(false) => {
            vote.chosen = Some(false);
            session.send(Up::KeysVote { entered: false });
        }
        Some(true) => {
            vote.entering = true;
            return_to.push(*screen.get());
            next.set(if settings.active_provider().is_some() { Screen::Provider } else { Screen::Keys });
        }
        None => {}
    }
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
fn host_pump(time: Res<Time>, session: Res<Session>, settings: Res<Settings>, master: Res<Master>, mut table: ResMut<Table>, mut game: Option<ResMut<Game>>, screen: Res<State<Screen>>, mut fx: ResMut<crate::fx::Fx>) {
    let Session::Host(hosting) = &*session else {
        master.relay.set_helpers(0);
        return;
    };
    let mut hosting = hosting.lock().unwrap();
    for event in hosting.net.poll() {
        match event {
            HostEvent::Joined(_) => {}
            HostEvent::Left(peer) => {
                // What the guest who left had waiting for the master did not happen.
                if let (Some(name), Some(game)) = (hosting.names.get(&peer), &game) {
                    let place = game.world.place_of(name).to_string();
                    table.queues.gone_from(&place, name, "");
                }
                hosting.names.remove(&peer);
                hosting.ids.remove(&peer);
                hosting.ready.remove(&peer);
                hosting.tokens.remove(&peer);
                hosting.sent.remove(&peer);
                hosting.parts.remove(&peer);
                // What the guest who left was working on goes to the next one.
                let orphaned: Vec<u64> = hosting.relayed.iter().filter(|(_, (_, tried))| tried.last() == Some(&peer)).map(|(id, _)| *id).collect();
                for id in orphaned {
                    pass_on(&mut hosting, &master.relay, id, Trouble::Network);
                }
            }
            HostEvent::Message(peer, Up::Hello { name, version, player, last_seq }) => {
                if version != PROTOCOL {
                    hosting.net.send(peer, &Down::WrongVersion);
                    hosting.net.kick(peer);
                    continue;
                }
                // One who comes back takes their seat again; the line they had, if it has not
                // been noticed to drop yet, is let go.
                let old = (player != 0).then(|| hosting.ids.iter().find(|(p, id)| **id == player && **p != peer).map(|(p, _)| *p)).flatten();
                let kept = old.and_then(|old| {
                    hosting.ids.remove(&old);
                    hosting.ready.remove(&old);
                    hosting.tokens.remove(&old);
                    hosting.sent.remove(&old);
                    hosting.net.kick(old);
                    hosting.names.remove(&old)
                });
                if kept.is_none() && hosting.names.len() + 1 >= SEATS {
                    hosting.net.send(peer, &Down::Full);
                    hosting.net.kick(peer);
                    continue;
                }
                let unique = kept.unwrap_or_else(|| {
                    // Two players with one name would share a hero; the later one gets a number.
                    let taken: Vec<&String> = hosting.names.values().collect();
                    let mut unique = name.trim().to_string();
                    let mut n = 2;
                    while unique.is_empty() || taken.contains(&&unique) || unique == player_name(&settings) {
                        unique = format!("{} {n}", name.trim());
                        n += 1;
                    }
                    unique
                });
                hosting.net.send(peer, &Down::Welcome { name: unique.clone() });
                hosting.names.insert(peer, unique);
                hosting.ids.insert(peer, player);
                // What was told while this player was away is sent again, in order.
                for message in missed_since(&hosting, player, last_seq) {
                    hosting.net.send(peer, &message);
                }
                // The world itself follows from the pump below, as seen from this guest's place.
            }
            HostEvent::Message(peer, Up::Ready(ready)) => {
                hosting.ready.insert(peer, ready);
            }
            HostEvent::Message(peer, Up::Resync) => {
                hosting.parts.remove(&peer);
                hosting.sent.remove(&peer);
            }
            HostEvent::Message(peer, Up::Position { x, y, facing, vx, vy }) => {
                hosting.tokens.insert(peer, (x, y, facing, (vx, vy)));
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
                    world.outfit(world.party.len() - 1);
                    world.owners.push(name.clone());
                    world.tidy_gear();
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
            HostEvent::Message(peer, Up::KeysVote { entered }) => {
                if let (Some(name), Some(vote)) = (hosting.names.get(&peer).cloned(), table.keys_vote.as_mut()) {
                    vote.votes.insert(name, entered);
                }
            }
            HostEvent::Message(peer, Up::Chat(text)) => {
                if let Some(name) = hosting.names.get(&peer).cloned() {
                    fx.cues.push(crate::audio::Sfx::Chat);
                    table.heard(&name, &text);
                    hosting.net.broadcast(&Down::Chat { from: name, text });
                }
            }
        }
    }
    master.relay.set_helpers(hosting.names.len());
    // No key at the table answers: everyone decides, the host among them, and the master
    // asks again when all have.
    if master.relay.voting() {
        let me = player_name(&settings);
        let vote = table.keys_vote.get_or_insert_with(KeysVote::default);
        if let Some(mine) = vote.chosen {
            vote.votes.insert(me.clone(), mine);
        }
        let mut waiting: Vec<String> = hosting.names.values().cloned().chain([me]).filter(|name| !vote.votes.contains_key(name)).collect();
        waiting.sort();
        if waiting != vote.waiting {
            hosting.net.broadcast(&Down::KeysOut { waiting: waiting.clone() });
            vote.waiting = waiting.clone();
        }
        if waiting.is_empty() {
            master.relay.resolve(vote.votes.values().any(|entered| *entered));
            hosting.net.broadcast(&Down::KeysOver);
            table.keys_vote = None;
        }
    } else if table.keys_vote.take().is_some() {
        hosting.net.broadcast(&Down::KeysOver);
    }
    let left = master.relay.asks_again_in();
    if left != table.keys_wait {
        table.keys_wait = left;
        hosting.net.broadcast(&Down::KeysWait(left));
    }
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
    let mut tokens: Vec<Token> = hosting.tokens.iter().filter_map(|(peer, (x, y, facing, (vx, vy)))| {
        let name = hosting.names.get(peer)?.clone();
        let place = game.world.place_of(&name).to_string();
        Some(Token { name, x: *x, y: *y, place, facing: *facing, vx: *vx, vy: *vy })
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
    if playing && hosting.since_positions >= POSITIONS_EVERY {
        let (x, y) = (game.world.player.x, game.world.player.y);
        let (vx, vy) = ((x - hosting.host_last.0) / hosting.since_positions.max(0.001), (y - hosting.host_last.1) / hosting.since_positions.max(0.001));
        hosting.host_last = (x, y);
        hosting.since_positions = 0.0;
        tokens.push(Token { name: player_name(&settings), x, y, place: game.world.current.clone(), facing: game.world.facing, vx, vy });
        hosting.net.broadcast_fast(&Down::Positions(tokens));
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
            if hosting.sent.get(&peer) == Some(&encoded) {
                continue;
            }
            hosting.sent.insert(peer, encoded);
            let Ok(value) = serde_json::to_value(&copy) else { continue };
            let now = parts_of(&value);
            let key = (game.world_id.clone(), copy.current.clone());
            // Within one world and one place only the parts that changed go: a place, a
            // picture, the story. Anything else, and a guest who asks, gets it whole.
            match hosting.parts.get(&peer).filter(|(was, _)| *was == key) {
                Some((_, before)) => {
                    let changed: Vec<(String, serde_json::Value)> = now.iter().filter(|(k, (hash, _))| before.get(*k) != Some(hash)).map(|(k, (_, v))| (k.clone(), v.clone())).collect();
                    let removed: Vec<String> = before.keys().filter(|k| !now.contains_key(*k)).cloned().collect();
                    if !changed.is_empty() || !removed.is_empty() {
                        hosting.net.send(peer, &Down::Patch { id: game.world_id.clone(), parts: changed, removed });
                    }
                }
                None => {
                    // Where a guest new to its place first stands: by the host, or where the
                    // place lets people in.
                    copy.player = if copy.current == host_place { host_token } else { copy.locations.get(&copy.current).map_or(host_token, |l| world::Player::in_cell(l.spawn.0, l.spawn.1)) };
                    hosting.net.send(peer, &Down::World { id: game.world_id.clone(), world: Box::new(copy.clone()) });
                }
            }
            hosting.parts.insert(peer, (key, now.into_iter().map(|(k, (hash, _))| (k, hash)).collect()));
        }
    }
}

/// Who is at the table, so the world knows whose heroes take part. The host decides; a
/// guest is told with the world. Alone, a save from a shared table is played with one's own
/// hero, and a save in which no hero is one's own with all of them.
/// The host keeps everyone told who sits at the table.
fn roster(time: Res<Time>, session: Res<Session>, settings: Res<Settings>, game: Option<Res<Game>>, mut table: ResMut<Table>, mut since: Local<f32>) {
    let Session::Host(hosting) = &*session else {
        if matches!(*session, Session::Solo) && table.roster != Roster::default() {
            table.roster = Roster::default();
        }
        return;
    };
    *since += time.delta_secs();
    if *since < 0.5 {
        return;
    }
    *since = 0.0;
    let hosting = hosting.lock().unwrap();
    let hero_of = |name: &str| game.as_ref().and_then(|g| g.world.owners.iter().position(|o| o == name).and_then(|i| g.world.party.get(i)).map(|h| format!("{} {}", h.name, h.level))).unwrap_or_default();
    let me = player_name(&settings);
    let mut seats = vec![Seat { name: me.clone(), hero: hero_of(&me), ready: true, host: true }];
    let mut guests: Vec<(&PeerId, &String)> = hosting.names.iter().collect();
    guests.sort_by(|a, b| a.1.cmp(b.1));
    seats.extend(guests.into_iter().map(|(peer, name)| Seat { name: name.clone(), hero: hero_of(name), ready: hosting.ready.get(peer) == Some(&true), host: false }));
    let port = hosting.net.port();
    let world = game.as_ref().map(|g| g.world.title.clone()).unwrap_or_default();
    let mut codes = if port == 0 { Vec::new() } else { table_codes(port, settings.lang) };
    let opened = hosting.upnp.as_ref().and_then(|u| u.opened.lock().unwrap().clone());
    if let Some(Ok(ip)) = &opened {
        codes.push((format!("{} {ip}", T::NetInternet.text(settings.lang)), net::code::encode(std::net::SocketAddrV4::new(*ip, port))));
    }
    let internet = opened.map(|o| o.is_ok());
    let roster = Roster { steam: matches!(hosting.net.lobby(), Some(Ok(_))), codes, world: world.clone(), playing: game.is_some(), internet, turn_timer: settings.turn_timer, seats };
    if let Some(beacon) = &hosting.beacon {
        let taken = roster.seats.len() as u8;
        beacon.update(|info| {
            info.name = me.clone();
            info.taken = taken;
            info.world = world.clone();
            info.playing = roster.playing;
        });
    }
    hosting.net.broadcast(&Down::Roster(roster.clone()));
    if table.roster != roster {
        table.roster = roster;
    }
}

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
            play::use_rules_of(&world);
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
    mut fx: ResMut<crate::fx::Fx>,
) {
    let (events, serving, way) = match &*session {
        Session::Client(joined) => {
            let mut joined = joined.lock().unwrap();
            joined.since_position += time.delta_secs();
            if let (true, Some(game)) = (joined.since_position >= POSITIONS_EVERY && *screen.get() == Screen::Play, &game) {
                let (x, y) = (game.world.player.x, game.world.player.y);
                let (vx, vy) = ((x - joined.last_sent.0) / joined.since_position.max(0.001), (y - joined.last_sent.1) / joined.since_position.max(0.001));
                joined.last_sent = (x, y);
                joined.since_position = 0.0;
                joined.net.send_fast(&Up::Position { x, y, facing: game.world.facing, vx, vy });
            }
            for (id, result) in std::mem::take(&mut *joined.served.lock().unwrap()) {
                joined.net.send(&Up::Answer { id, result });
            }
            (joined.net.poll(), joined.served.clone(), joined.way.clone())
        }
        _ => return,
    };
    let events = unwrap_feed(events, &mut table.feed_seen);
    for event in events {
        let me = player_name(&settings);
        match event {
            ClientEvent::Message(Down::Welcome { name }) => {
                // Back at the table, if the line had dropped.
                table.rejoin = None;
                table.status = None;
                // The host may have numbered the name; the hero is recorded under what the
                // host calls this player.
                if name != me {
                    settings.player_name = name;
                }
            }
            ClientEvent::Message(Down::World { id, world }) => adopt(id, *world, &mut game, &mut commands, &mut next, *screen.get(), &me, &mut redraw, &mut phase, &store),
            ClientEvent::Message(Down::Patch { id, parts, removed }) => {
                // Put together onto the world this guest has; if it does not fit, the whole
                // world is asked for.
                let patched = game.as_ref().filter(|g| g.world_id == id).and_then(|g| patch_world(&g.world, &parts, &removed));
                match patched {
                    Some(world) => adopt(id, world, &mut game, &mut commands, &mut next, *screen.get(), &me, &mut redraw, &mut phase, &store),
                    None => session.send(Up::Resync),
                }
            }
            ClientEvent::Message(Down::Positions(tokens)) => {
                if let Some(game) = &mut game {
                    game.world.others = tokens.into_iter().filter(|t| t.name != me).collect();
                }
            }
            ClientEvent::Message(Down::MoveTo { x, y }) => {
                // The story takes this guest's hero somewhere: it walks there, the way the
                // host's does to a thing, and the player is told; where no way leads there,
                // it is set down at once.
                if let Some(game) = game.as_mut() {
                    match play::way_to_cell(&game.world, (x, y)) {
                        Some(path) if !path.is_empty() => {
                            table.led = path;
                            table.notices.push(T::LedThere.text(settings.lang).to_string());
                        }
                        _ => game.world.player = world::Player::in_cell(x, y),
                    }
                }
            }
            ClientEvent::Message(Down::Told(turn)) => {
                play::show(turn, &mut phase, &mut table);
                redraw.0 = true;
            }
            ClientEvent::Message(Down::Roster(roster)) => table.roster = roster,
            ClientEvent::Message(Down::News { place, text }) => table.notices.push(format!("{place}: {text}")),
            // Unwrapped above: a feed in a feed is nothing.
            ClientEvent::Message(Down::Feed { .. }) => {}
            ClientEvent::Message(Down::Kicked | Down::Full) => {
                // Not a dropped line: there is no coming back by trying again.
                table.status = Some(T::LobbyKicked);
                table.rejoin = None;
                *session = Session::Solo;
                next.set(Screen::Lobby);
                return;
            }
            ClientEvent::Message(Down::Bubble { place, who, text }) => {
                if game.as_ref().is_some_and(|g| g.world.current == place) {
                    table.bubbles.push((who, text));
                }
            }
            ClientEvent::Message(Down::Thinking(said)) => {
                // What a fellow player does is seen by all; one's own words are on the sheet.
                let mine = game.as_ref().and_then(|g| g.world.owners.iter().position(|o| *o == me).and_then(|i| g.world.party.get(i))).map(|h| format!("{}: ", h.name));
                if !mine.is_some_and(|mine| said.starts_with(&mine)) && !said.starts_with(&format!("{me}: ")) {
                    table.notices.push(said);
                }
            }
            ClientEvent::Message(Down::Trouble(why)) => {
                if matches!(*phase, Phase::Asking { .. } | Phase::Drawing { .. }) {
                    *phase = Phase::Walking;
                }
                // Said to the player in words: nothing they did ends without them.
                if !why.trim().is_empty() {
                    play::show(Turn { narration: why, options: Vec::new() }, &mut phase, &mut table);
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
            ClientEvent::Message(Down::KeysOut { waiting }) => table.keys_vote.get_or_insert_with(KeysVote::default).waiting = waiting,
            ClientEvent::Message(Down::KeysOver) => table.keys_vote = None,
            ClientEvent::Message(Down::KeysWait(left)) => table.keys_wait = left,
            ClientEvent::Message(Down::Ask(asked)) => {
                // The model may take minutes: the answer comes back from a thread of its own.
                let (gm, fake, serving) = (gm.clone(), master.fake, serving.clone());
                let (providers, rotation) = (settings.providers(), master.rotation.clone());
                std::thread::spawn(move || {
                    let result = serve_for_host(&gm, &asked, fake, &providers, &rotation);
                    serving.lock().unwrap().push((asked.id, result));
                });
            }
            ClientEvent::Message(Down::Chat { from, text }) => {
                table.heard(&from, &text);
                fx.cues.push(crate::audio::Sfx::Chat);
            }
            ClientEvent::Message(Down::Die(value)) => {
                fx.throw(value);
                fx.cues.push(crate::audio::Sfx::Dice);
            }
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
                table.rejoin = None;
                *session = Session::Solo;
                return;
            }
            ClientEvent::Disconnected => {
                // The line dropped. What this guest last saw is kept as a save of its own, so
                // the game can go on alone from here; meanwhile the table is tried again.
                if let Some(game) = &game
                    && let Err(e) = store.save_game(&game.world_id, &game.world, false)
                {
                    error!("could not save the game: {e}");
                }
                if table.rejoin.is_none() {
                    table.rejoin = Some(Rejoin { way: way.clone(), tries: 0, since: 0.0, trying: None });
                }
                *session = Session::Solo;
                if matches!(*phase, Phase::Asking { .. } | Phase::Drawing { .. }) {
                    *phase = Phase::Walking;
                }
                return;
            }
        }
    }
}

/// At a table joined by address one goes by the name typed, or by this machine's name when
/// nothing of one's own was typed (the field still holding the Steam name counts as nothing).
fn direct_name(settings: &mut Settings, steam: Option<&net::Steam>) {
    let typed = settings.player_name.trim().to_string();
    if typed.is_empty() || steam.is_some_and(|s| s.name() == typed) {
        settings.player_name = computer_name();
    }
}

/// The name of this machine, for a table joined by address.
fn computer_name() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).or_else(|_| std::env::var("USERNAME")).unwrap_or_else(|_| "Player".to_string())
}

/// Whether this table is hosted in a Steam lobby that is open.
pub fn steam_table(session: &Session) -> bool {
    matches!(session, Session::Host(hosting) if matches!(hosting.lock().unwrap().net.lobby(), Some(Ok(_))))
}

/// Steam is asked for when the game starts, so that an invitation accepted in Steam finds
/// the game listening. A test instance with a home of its own leaves Steam alone.
fn steam_start(mut link: ResMut<SteamLink>) {
    if std::env::var_os("PIXELGM_HOME").is_some() {
        return;
    }
    link.tried = true;
    match net::Steam::init() {
        Ok(steam) => {
            link.online = steam.friends_online();
            info!("Steam is here: {} (overlay {})", steam.name(), if steam.overlay() { "on" } else { "off" });
            link.steam = Some(steam);
        }
        Err(e) => info!("Steam is not available: {e}"),
    }
}

/// An invitation accepted in Steam — in the overlay, the friends list, or before the game
/// was started — brings this player to that table.
fn steam_invites(time: Res<Time>, link: Res<SteamLink>, mut session: ResMut<Session>, mut settings: ResMut<Settings>, mut next: ResMut<NextState<Screen>>, mut since: Local<f32>) {
    let Some(steam) = &link.steam else { return };
    *since += time.delta_secs();
    if *since < 0.25 {
        return;
    }
    *since = 0.0;
    // A table's own thread lets Steam speak; in the menus and alone the game does.
    if matches!(*session, Session::Solo) {
        steam.pump();
    }
    // Steam needs a moment after the first frames to hook its overlay: say when it has.
    static OVERLAY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let overlay = steam.overlay();
    if OVERLAY.swap(overlay, std::sync::atomic::Ordering::Relaxed) != overlay {
        info!("Steam overlay is now {}", if overlay { "on" } else { "off" });
    }
    let Some(lobby) = steam.asked_into() else { return };
    info!("invited through Steam into lobby {lobby}");
    settings.player_name = steam.name();
    *session = join_steam(steam, lobby, &steam.name(), settings.player_id, 0);
    next.set(Screen::Lobby);
}

mod lobby;

pub struct CoopPlugin;

impl Plugin for CoopPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Session>()
            .init_resource::<Table>()
            .init_resource::<SteamLink>()
            .add_plugins(lobby::LobbyPlugin)
            .add_systems(PreUpdate, (host_pump, client_pump, rejoin, presence, roster).chain())
            .add_systems(Startup, steam_start)
            .add_systems(Update, steam_invites)
            .add_systems(bevy_egui::EguiPrimaryContextPass, keys_vote.in_set(crate::ui::UiSet::Screens))
;
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
        w.others.push(Token { name: "Bob".into(), x: 1.0, y: 2.0, place: String::new(), facing: (0, 0), vx: 0.0, vy: 0.0 });
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
        let table = host(None, "Ann", Some(0)).unwrap();
        assert!(my_turn(&w, &table, &ann));
        assert!(!my_turn(&w, &table, &bob));
        assert_eq!(owner_name(&Session::Solo, &ann), "");
        assert_eq!(owner_name(&table, &ann), "Ann");
    }

    #[test]
    fn an_address_without_a_port_gets_the_usual_one() {
        // Nothing listens there: what matters is that the address was understood.
        let error = join("127.0.0.1:1", "Ann", 7, 0).err().expect("nobody is listening");
        assert_ne!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn a_table_is_joined_by_its_code_and_a_typo_is_told_without_trying() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let code = net::code::encode(std::net::SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, port));
        assert!(join(&code, "Ann", 7, 0).is_ok());
        let mut typo: Vec<char> = code.chars().collect();
        typo[0] = if typo[0] == 'Z' { 'Y' } else { 'Z' };
        let typo: String = typo.into_iter().collect();
        assert_eq!(join(&typo, "Ann", 7, 0).err().map(|e| e.kind()), Some(std::io::ErrorKind::InvalidInput));
    }

    #[test]
    fn what_was_told_while_a_player_was_away_is_sent_again_once() {
        let table = host(None, "Host", Some(0)).unwrap();
        let Session::Host(hosting) = &table else { unreachable!() };
        let port = hosting.lock().unwrap().net.port();
        let guest: Client<Up, Down> = Client::connect(&format!("127.0.0.1:{port}")).unwrap();
        // The guest is seated as the Hello would seat it.
        let peer = loop {
            if let Some(HostEvent::Joined(peer)) = hosting.lock().unwrap().net.poll().into_iter().next() {
                break peer;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        hosting.lock().unwrap().names.insert(peer, "Ann".into());
        hosting.lock().unwrap().ids.insert(peer, 7);
        let turn = |text: &str| Down::Told(Turn { narration: text.into(), options: Vec::new() });
        assert!(table.tell("Ann", turn("one")));
        assert!(table.tell("Ann", turn("two")));
        // Read both as they came.
        let mut seen = 0;
        let mut read = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while read.len() < 2 && std::time::Instant::now() < deadline {
            read.extend(unwrap_feed(guest.poll(), &mut seen));
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(seen, 2);
        // The line drops after the first: the second is sent again, and only the second.
        let again = missed_since(&hosting.lock().unwrap(), 7, 1);
        assert_eq!(again.len(), 1);
        let mut seen_once = 1;
        let resent: Vec<ClientEvent<Down>> = again.into_iter().map(ClientEvent::Message).collect();
        let shown = unwrap_feed(resent, &mut seen_once);
        assert!(matches!(&shown[..], [ClientEvent::Message(Down::Told(t))] if t.narration == "two"));
        // What was read already is not read twice.
        let twice: Vec<ClientEvent<Down>> = missed_since(&hosting.lock().unwrap(), 7, 0).into_iter().map(ClientEvent::Message).collect();
        assert_eq!(unwrap_feed(twice, &mut seen).len(), 0);
    }

    #[test]
    fn a_world_put_together_from_its_changed_parts_is_the_world_the_host_has() {
        let before = world_with(&["Ann"]);
        let mut after = before.clone();
        // A place changed, a picture came, the story grew; nothing else.
        after.here_mut().description = "The fire is out.".into();
        after.sprites.insert("ghost".into(), after.sprites.values().next().unwrap().clone());
        after.story.facts.push("The fire went out.".into());
        let (old, new) = (parts_of(&serde_json::to_value(&before).unwrap()), parts_of(&serde_json::to_value(&after).unwrap()));
        let changed: Vec<(String, serde_json::Value)> = new.iter().filter(|(k, (h, _))| old.get(*k).map(|(o, _)| o) != Some(h)).map(|(k, (_, v))| (k.clone(), v.clone())).collect();
        let keys: Vec<&str> = changed.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"sprites/ghost") && keys.iter().any(|k| k.starts_with("locations/")) && keys.contains(&"story"), "{keys:?}");
        // Only the parts that changed travel, not the whole world.
        assert!(changed.len() <= 4, "{keys:?}");
        let patched = patch_world(&before, &changed, &[]).unwrap();
        assert_eq!(serde_json::to_value(&patched).unwrap(), serde_json::to_value(&after).unwrap());
        // A picture gone is taken out.
        let gone = patch_world(&after, &[], &["sprites/ghost".to_string()]).unwrap();
        assert!(!gone.sprites.contains_key("ghost"));
    }
}
