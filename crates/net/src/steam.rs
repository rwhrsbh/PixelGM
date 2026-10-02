//! The same wire over Steam. A friends-only lobby lets players find each other, and Steam's
//! relayed messages carry the frames, so nobody has to open a port or know an address.
//!
//! Steam delivers a message whole or not at all but limits its size, so a frame travels as
//! one or more chunks: a first byte that says whether this is the last, then the piece.
//!
//! A session between two players can break and mend itself, so a broken session alone does
//! not end the game. Both ends say "still here" every couple of seconds, and an end that
//! has heard nothing for a while takes the other for gone.

use crate::{Client, ClientEvent, Host, HostEvent, MAX_FRAME, PeerId};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use steamworks::networking_types::{NetworkingIdentity, SendFlags};
use steamworks::{ChatMemberStateChange, FriendFlags, LobbyChatUpdate, LobbyId, LobbyType, SteamId};

/// Spacewar, Valve's test application, which any Steam account may use.
const APP_ID: u32 = 480;
/// Guests write to the host on one channel and the host answers on another, so the two
/// ends can even live in one process.
const TO_HOST: u32 = 0;
const TO_GUEST: u32 = 1;
/// Well under Steam's limit for one reliable message.
const CHUNK: usize = 200 * 1024;
const TICK: Duration = Duration::from_millis(5);
const JOIN_PATIENCE: Duration = Duration::from_secs(20);
const STILL_HERE_EVERY: Duration = Duration::from_secs(2);
/// Silence longer than this means the other end is gone.
const SILENCE_LIMIT: Duration = Duration::from_secs(15);
/// First bytes of a chunk: a piece with more to come, the last piece, or just "still here".
const MORE: u8 = 0;
const LAST: u8 = 1;
const STILL_HERE: u8 = 2;
/// The rich-presence key under which a host tells its friends which lobby to join.
const PRESENCE_KEY: &str = "pixelgm_lobby";
/// The host and up to four guests.
const LOBBY_SEATS: u32 = 5;

type Peers = Arc<Mutex<HashMap<PeerId, Sender<Vec<u8>>>>>;
/// How a Steam host's lobby is doing: not answered yet, open under an id, or refused.
pub type LobbyState = Arc<Mutex<Option<Result<u64, String>>>>;

/// The running Steam client, as far as the game needs it.
#[derive(Clone)]
pub struct Steam {
    client: steamworks::Client,
    /// Steam's callbacks must not be run from two threads at once.
    pump: Arc<Mutex<()>>,
}

impl Steam {
    /// Connects to the Steam client running on this machine.
    pub fn init() -> Result<Self, String> {
        // The library is loaded on first use; without the file that use would crash.
        if !library_present() {
            return Err(format!("{LIBRARY} is missing"));
        }
        steamworks::Client::init_app(APP_ID).map(|client| Self { client, pump: Arc::default() }).map_err(|e| e.to_string())
    }

    /// The player's Steam name.
    pub fn name(&self) -> String {
        self.client.friends().name()
    }

    /// Friends who are hosting a game right now: their names and lobbies.
    pub fn friends_hosting(&self) -> Vec<(String, u64)> {
        self.run_callbacks();
        let mut hosting: Vec<(String, u64)> =
            self.client.friends().get_friends(FriendFlags::IMMEDIATE).into_iter().filter_map(|friend| Some((friend.name(), friend.rich_presence(PRESENCE_KEY)?.parse::<u64>().ok()?))).collect();
        hosting.sort();
        hosting
    }

    fn run_callbacks(&self) {
        let _one_at_a_time = self.pump.lock().unwrap();
        self.client.run_callbacks();
    }
}

const LIBRARY: &str = if cfg!(windows) { "steam_api64.dll" } else if cfg!(target_os = "macos") { "libsteam_api.dylib" } else { "libsteam_api.so" };

/// Whether Steam's library can be found where the system looks for it: beside the game or
/// on the search path.
fn library_present() -> bool {
    let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()));
    let variable = if cfg!(windows) { "PATH" } else { "LD_LIBRARY_PATH" };
    let on_path = std::env::var_os(variable).map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>()).unwrap_or_default();
    // Elsewhere than Windows the loader has more places to look than can be checked here.
    !cfg!(windows) || beside.into_iter().chain(on_path).any(|dir| dir.join(LIBRARY).is_file())
}

fn chunks(frame: &[u8]) -> impl Iterator<Item = Vec<u8>> + '_ {
    let count = frame.len().div_ceil(CHUNK).max(1);
    (0..count).map(move |i| {
        let piece = &frame[(i * CHUNK).min(frame.len())..((i + 1) * CHUNK).min(frame.len())];
        let mut chunk = Vec::with_capacity(piece.len() + 1);
        chunk.push(if i + 1 == count { LAST } else { MORE });
        chunk.extend_from_slice(piece);
        chunk
    })
}

/// Puts a frame back together from its chunks.
#[derive(Default)]
struct Assembly(Vec<u8>);

impl Assembly {
    /// Takes one chunk; gives the frame when it was the last. A frame that grows past any
    /// sensible size is thrown away.
    fn take(&mut self, chunk: &[u8]) -> Option<Vec<u8>> {
        let (kind, piece) = chunk.split_first()?;
        if *kind > LAST {
            return None;
        }
        self.0.extend_from_slice(piece);
        if self.0.len() > MAX_FRAME {
            self.0.clear();
            return None;
        }
        (*kind == LAST).then(|| std::mem::take(&mut self.0))
    }
}

type Messages = steamworks::networking_messages::NetworkingMessages;

fn send_chunk(messages: &Messages, to: SteamId, channel: u32, chunk: &[u8]) {
    let _ = messages.send_message_to_user(NetworkingIdentity::new_steam_id(to), SendFlags::RELIABLE | SendFlags::AUTO_RESTART_BROKEN_SESSION, chunk, channel);
}

fn send_frame(messages: &Messages, to: SteamId, channel: u32, frame: &[u8]) {
    for chunk in chunks(frame) {
        send_chunk(messages, to, channel, &chunk);
    }
}

/// Says "still here" when it is time to.
struct Heartbeat(Instant);

impl Heartbeat {
    fn due(&mut self) -> bool {
        let due = self.0.elapsed() >= STILL_HERE_EVERY;
        if due {
            self.0 = Instant::now();
        }
        due
    }
}

struct Guest {
    id: PeerId,
    outgoing: Receiver<Vec<u8>>,
    incoming: Assembly,
    heard: Instant,
}

impl<C: DeserializeOwned + Send + 'static, H: Serialize> Host<C, H> {
    /// Opens a friends-only Steam lobby and hosts in it. The lobby is asked for in the
    /// background: `lobby()` says how that went.
    pub fn listen_steam(steam: &Steam) -> Self {
        let (to_game, events) = channel();
        let peers: Peers = Arc::default();
        let lobby: LobbyState = Arc::default();
        let (steam, known, state) = (steam.clone(), peers.clone(), lobby.clone());
        std::thread::spawn(move || host_thread(steam, to_game, known, state));
        Self { events, peers, port: 0, lobby, _sends: PhantomData }
    }
}

fn host_thread<C: DeserializeOwned>(steam: Steam, to_game: Sender<HostEvent<C>>, known: Peers, state: LobbyState) {
    let client = steam.client.clone();
    let (matchmaking, messages, friends) = (client.matchmaking(), client.networking_messages(), client.friends());
    {
        let state = state.clone();
        matchmaking.create_lobby(LobbyType::FriendsOnly, LOBBY_SEATS, move |made| {
            *state.lock().unwrap() = Some(made.map(|lobby| lobby.raw()).map_err(|e| e.to_string()));
        });
    }
    messages.session_request_callback(|request| {
        request.accept();
    });
    let gone: Arc<Mutex<Vec<SteamId>>> = Arc::default();
    let _left = {
        let gone = gone.clone();
        client.register_callback(move |update: LobbyChatUpdate| {
            if update.member_state_change != ChatMemberStateChange::Entered {
                gone.lock().unwrap().push(update.user_changed);
            }
        })
    };
    let mut guests: HashMap<SteamId, Guest> = HashMap::new();
    let mut next_id: PeerId = 1;
    let mut announced = false;
    let mut heartbeat = Heartbeat(Instant::now());
    // The game dropping its end leaves this thread the only holder of the peers.
    'serving: while Arc::strong_count(&known) > 1 {
        steam.run_callbacks();
        let lobby = match &*state.lock().unwrap() {
            Some(Ok(lobby)) => Some(LobbyId::from_raw(*lobby)),
            Some(Err(_)) => break,
            None => None,
        };
        if let (Some(lobby), false) = (lobby, announced) {
            friends.set_rich_presence(PRESENCE_KEY, Some(&lobby.raw().to_string()));
            announced = true;
        }
        for message in messages.receive_messages_on_channel(TO_HOST, 64) {
            let Some(from) = message.identity_peer().steam_id() else { continue };
            if !guests.contains_key(&from) {
                // Only somebody sitting in the lobby is a guest.
                if !lobby.is_some_and(|lobby| matchmaking.lobby_members(lobby).contains(&from)) {
                    continue;
                }
                let (queue, outgoing) = channel();
                known.lock().unwrap().insert(next_id, queue);
                guests.insert(from, Guest { id: next_id, outgoing, incoming: Assembly::default(), heard: Instant::now() });
                if to_game.send(HostEvent::Joined(next_id)).is_err() {
                    break 'serving;
                }
                next_id += 1;
            }
            let Some(guest) = guests.get_mut(&from) else { continue };
            guest.heard = Instant::now();
            if let Some(frame) = guest.incoming.take(message.data()) {
                match serde_json::from_slice(&frame) {
                    Ok(read) => {
                        if to_game.send(HostEvent::Message(guest.id, read)).is_err() {
                            break 'serving;
                        }
                    }
                    // A guest speaking another version of the protocol is dropped.
                    Err(_) => gone.lock().unwrap().push(from),
                }
            }
        }
        let mut dropped = std::mem::take(&mut *gone.lock().unwrap());
        let still_here = heartbeat.due();
        for (steam_id, guest) in &guests {
            if guest.heard.elapsed() > SILENCE_LIMIT {
                dropped.push(*steam_id);
                continue;
            }
            if still_here {
                send_chunk(&messages, *steam_id, TO_GUEST, &[STILL_HERE]);
            }
            loop {
                match guest.outgoing.try_recv() {
                    Ok(frame) => send_frame(&messages, *steam_id, TO_GUEST, &frame),
                    Err(TryRecvError::Empty) => break,
                    // The game kicked this guest.
                    Err(TryRecvError::Disconnected) => {
                        dropped.push(*steam_id);
                        break;
                    }
                }
            }
        }
        for steam_id in dropped {
            if let Some(guest) = guests.remove(&steam_id) {
                known.lock().unwrap().remove(&guest.id);
                let _ = to_game.send(HostEvent::Left(guest.id));
            }
        }
        std::thread::sleep(TICK);
    }
    friends.set_rich_presence(PRESENCE_KEY, None);
    if let Some(Ok(lobby)) = &*state.lock().unwrap() {
        matchmaking.leave_lobby(LobbyId::from_raw(*lobby));
    }
}

impl<C: Serialize, H: DeserializeOwned + Send + 'static> Client<C, H> {
    /// Joins a friend's lobby. Joining happens in the background; if it fails the game
    /// hears `Disconnected`.
    pub fn connect_steam(steam: &Steam, lobby: u64) -> Self {
        let (to_game, events) = channel();
        let (queue, outgoing) = channel::<Vec<u8>>();
        let steam = steam.clone();
        std::thread::spawn(move || {
            guest_thread(&steam, LobbyId::from_raw(lobby), &to_game, &outgoing);
            let _ = to_game.send(ClientEvent::Disconnected);
        });
        Self { events, queue, _sends: PhantomData }
    }
}

fn guest_thread<H: DeserializeOwned>(steam: &Steam, lobby: LobbyId, to_game: &Sender<ClientEvent<H>>, outgoing: &Receiver<Vec<u8>>) {
    let client = steam.client.clone();
    let (matchmaking, messages) = (client.matchmaking(), client.networking_messages());
    let joined: Arc<Mutex<Option<bool>>> = Arc::default();
    {
        let joined = joined.clone();
        matchmaking.join_lobby(lobby, move |result| *joined.lock().unwrap() = Some(result.is_ok()));
    }
    messages.session_request_callback(|request| {
        request.accept();
    });
    let departed: Arc<Mutex<Vec<SteamId>>> = Arc::default();
    let _left = {
        let departed = departed.clone();
        client.register_callback(move |update: LobbyChatUpdate| {
            if update.member_state_change != ChatMemberStateChange::Entered {
                departed.lock().unwrap().push(update.user_changed);
            }
        })
    };
    let started = Instant::now();
    let mut host: Option<SteamId> = None;
    let mut incoming = Assembly::default();
    let mut heartbeat = Heartbeat(Instant::now());
    let mut heard = Instant::now();
    loop {
        steam.run_callbacks();
        let Some(owner) = host else {
            match *joined.lock().unwrap() {
                Some(true) => {
                    host = Some(matchmaking.lobby_owner(lobby));
                    heard = Instant::now();
                }
                Some(false) => return,
                None if started.elapsed() > JOIN_PATIENCE => return,
                None => std::thread::sleep(TICK),
            }
            continue;
        };
        if heartbeat.due() {
            send_chunk(&messages, owner, TO_HOST, &[STILL_HERE]);
        }
        loop {
            match outgoing.try_recv() {
                Ok(frame) => send_frame(&messages, owner, TO_HOST, &frame),
                Err(TryRecvError::Empty) => break,
                // The game let go of its end: leave quietly.
                Err(TryRecvError::Disconnected) => {
                    matchmaking.leave_lobby(lobby);
                    return;
                }
            }
        }
        for message in messages.receive_messages_on_channel(TO_GUEST, 64) {
            if message.identity_peer().steam_id() != Some(owner) {
                continue;
            }
            heard = Instant::now();
            if let Some(frame) = incoming.take(message.data()) {
                let delivered = serde_json::from_slice(&frame).is_ok_and(|read| to_game.send(ClientEvent::Message(read)).is_ok());
                if !delivered {
                    matchmaking.leave_lobby(lobby);
                    return;
                }
            }
        }
        if heard.elapsed() > SILENCE_LIMIT || departed.lock().unwrap().contains(&owner) {
            matchmaking.leave_lobby(lobby);
            return;
        }
        std::thread::sleep(TICK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_frame_is_one_chunk_marked_last() {
        let sent: Vec<Vec<u8>> = chunks(b"hello").collect();
        assert_eq!(sent, [b"\x01hello".to_vec()]);
        assert_eq!(Assembly::default().take(&sent[0]), Some(b"hello".to_vec()));
    }

    #[test]
    fn a_large_frame_is_cut_up_and_put_back_together() {
        let frame: Vec<u8> = (0..CHUNK * 2 + 17).map(|i| (i % 251) as u8).collect();
        let sent: Vec<Vec<u8>> = chunks(&frame).collect();
        assert_eq!(sent.len(), 3);
        assert_eq!(sent.iter().map(|c| c[0]).collect::<Vec<u8>>(), [0, 0, 1]);
        let mut assembly = Assembly::default();
        assert_eq!(assembly.take(&sent[0]), None);
        assert_eq!(assembly.take(&sent[1]), None);
        assert_eq!(assembly.take(&sent[2]), Some(frame));
        // The next frame starts clean.
        assert_eq!(assembly.take(b"\x01x"), Some(b"x".to_vec()));
    }

    #[test]
    fn a_frame_of_exactly_one_chunk_is_not_followed_by_an_empty_one() {
        let frame = vec![7u8; CHUNK];
        assert_eq!(chunks(&frame).count(), 1);
    }

    #[test]
    fn an_empty_chunk_is_ignored() {
        assert_eq!(Assembly::default().take(&[]), None);
    }

    #[test]
    fn still_here_between_the_chunks_of_a_frame_does_not_spoil_it() {
        let mut assembly = Assembly::default();
        assert_eq!(assembly.take(&[MORE, b'a']), None);
        assert_eq!(assembly.take(&[STILL_HERE]), None);
        assert_eq!(assembly.take(&[LAST, b'b']), Some(b"ab".to_vec()));
    }
}
