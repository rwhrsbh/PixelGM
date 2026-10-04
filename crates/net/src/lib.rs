//! The wire between the host of a game and the players who joined it.
//!
//! The host is the only authority: it holds the world, clients send what they want to do
//! and are sent what happened. This crate only moves messages; what they mean is the game's
//! business. Messages are JSON in length-prefixed frames over TCP, which serves both for a
//! direct connection by address and for several instances on one machine in tests. The
//! same frames travel through Steam for players who are Steam friends (`steam`).

pub mod steam;
pub use steam::bring_overlay;
pub use steam::Steam;

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::marker::PhantomData;
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A frame larger than this is a broken or hostile peer, not a message.
const MAX_FRAME: usize = 32 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

pub type PeerId = u32;

fn write_frame(stream: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    stream.write_all(&(bytes.len() as u32).to_le_bytes())?;
    stream.write_all(bytes)?;
    stream.flush()
}

fn read_frame(stream: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut length = [0u8; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_le_bytes(length) as usize;
    if length > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut bytes = vec![0u8; length];
    stream.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// Reads messages from a stream until it closes or sends something unreadable, handing each
/// to `deliver`.
fn pump<M: DeserializeOwned>(mut stream: TcpStream, mut deliver: impl FnMut(M) -> bool) {
    while let Ok(bytes) = read_frame(&mut stream) {
        match serde_json::from_slice(&bytes) {
            Ok(message) => {
                if !deliver(message) {
                    break;
                }
            }
            // A peer speaking another version of the protocol is dropped, not guessed at.
            Err(_) => break,
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
}

/// Writes queued frames to a stream on its own thread, so a slow peer never stalls the game.
fn writer(mut stream: TcpStream) -> Sender<Vec<u8>> {
    let (queue, frames) = channel::<Vec<u8>>();
    std::thread::spawn(move || {
        for frame in frames {
            if write_frame(&mut stream, &frame).is_err() {
                break;
            }
        }
        let _ = stream.shutdown(Shutdown::Both);
    });
    queue
}

#[derive(Debug, PartialEq)]
pub enum HostEvent<C> {
    Joined(PeerId),
    Left(PeerId),
    Message(PeerId, C),
}

/// The host's end: accepts players and talks to each. `C` is what clients send, `H` what
/// the host sends.
pub struct Host<C, H> {
    events: Receiver<HostEvent<C>>,
    peers: Arc<Mutex<HashMap<PeerId, Sender<Vec<u8>>>>>,
    port: u16,
    /// A Steam host's lobby; stays empty for a host listening on a port.
    lobby: steam::LobbyState,
    _sends: PhantomData<H>,
}

impl<C: DeserializeOwned + Send + 'static, H: Serialize> Host<C, H> {
    /// Starts listening. Port 0 asks the system for any free port; `port()` says which.
    pub fn listen(address: impl ToSocketAddrs) -> io::Result<Self> {
        let listener = TcpListener::bind(address)?;
        let port = listener.local_addr()?.port();
        let (to_game, events) = channel();
        let peers: Arc<Mutex<HashMap<PeerId, Sender<Vec<u8>>>>> = Arc::default();
        let known = peers.clone();
        std::thread::spawn(move || {
            for (id, stream) in (1..).zip(listener.incoming().flatten()) {
                let _ = stream.set_nodelay(true);
                let Ok(reading) = stream.try_clone() else { continue };
                known.lock().unwrap().insert(id, writer(stream));
                if to_game.send(HostEvent::Joined(id)).is_err() {
                    break;
                }
                let (to_game, known) = (to_game.clone(), known.clone());
                std::thread::spawn(move || {
                    pump(reading, |message| to_game.send(HostEvent::Message(id, message)).is_ok());
                    known.lock().unwrap().remove(&id);
                    let _ = to_game.send(HostEvent::Left(id));
                });
            }
        });
        Ok(Self { events, peers, port, lobby: Arc::default(), _sends: PhantomData })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// How a Steam host's lobby is doing: `None` while Steam has not answered (and always
    /// for a host listening on a port), then the lobby's id or why there is none.
    pub fn lobby(&self) -> Option<Result<u64, String>> {
        self.lobby.lock().unwrap().clone()
    }

    /// Everything that happened since the last call, in order.
    pub fn poll(&self) -> Vec<HostEvent<C>> {
        self.events.try_iter().collect()
    }

    pub fn peers(&self) -> Vec<PeerId> {
        let mut ids: Vec<PeerId> = self.peers.lock().unwrap().keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// Queues a message for one player. `false` when that player is gone.
    pub fn send(&self, peer: PeerId, message: &H) -> bool {
        let Ok(frame) = serde_json::to_vec(message) else { return false };
        self.peers.lock().unwrap().get(&peer).is_some_and(|queue| queue.send(frame).is_ok())
    }

    /// Queues a message for every player. The frame is encoded once.
    pub fn broadcast(&self, message: &H) {
        if let Ok(frame) = serde_json::to_vec(message) {
            for queue in self.peers.lock().unwrap().values() {
                let _ = queue.send(frame.clone());
            }
        }
    }

    /// Drops a player.
    pub fn kick(&self, peer: PeerId) {
        self.peers.lock().unwrap().remove(&peer);
    }
}

#[derive(Debug, PartialEq)]
pub enum ClientEvent<H> {
    /// The connection is gone; the game goes on alone or not at all.
    Disconnected,
    Message(H),
}

/// A player's end of the wire.
pub struct Client<C, H> {
    events: Receiver<ClientEvent<H>>,
    queue: Sender<Vec<u8>>,
    _sends: PhantomData<C>,
}

impl<C: Serialize, H: DeserializeOwned + Send + 'static> Client<C, H> {
    pub fn connect(address: &str) -> io::Result<Self> {
        let target = address.to_socket_addrs()?.next().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no such address"))?;
        let stream = TcpStream::connect_timeout(&target, CONNECT_TIMEOUT)?;
        let _ = stream.set_nodelay(true);
        let reading = stream.try_clone()?;
        let (to_game, events) = channel();
        std::thread::spawn(move || {
            pump(reading, |message| to_game.send(ClientEvent::Message(message)).is_ok());
            let _ = to_game.send(ClientEvent::Disconnected);
        });
        Ok(Self { events, queue: writer(stream), _sends: PhantomData })
    }

    pub fn poll(&self) -> Vec<ClientEvent<H>> {
        self.events.try_iter().collect()
    }

    pub fn send(&self, message: &C) -> bool {
        serde_json::to_vec(message).is_ok_and(|frame| self.queue.send(frame).is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::time::Instant;

    #[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
    enum Up {
        Hello(String),
        Big(Vec<u8>),
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
    enum Down {
        Welcome(u32),
        Tick(u32),
    }

    /// Polls until `enough` says the collected events will do, or two seconds pass.
    fn gather<E>(mut poll: impl FnMut() -> Vec<E>, enough: impl Fn(&[E]) -> bool) -> Vec<E> {
        let started = Instant::now();
        let mut all = Vec::new();
        while !enough(&all) && started.elapsed() < Duration::from_secs(2) {
            all.extend(poll());
            std::thread::sleep(Duration::from_millis(5));
        }
        all
    }

    fn pair() -> (Host<Up, Down>, Client<Up, Down>) {
        let host = Host::listen("127.0.0.1:0").unwrap();
        let client = Client::connect(&format!("127.0.0.1:{}", host.port())).unwrap();
        (host, client)
    }

    #[test]
    fn a_player_joins_says_hello_and_is_answered() {
        let (host, client) = pair();
        assert!(client.send(&Up::Hello("Ann".into())));
        let events = gather(|| host.poll(), |e| e.len() >= 2);
        assert_eq!(events, [HostEvent::Joined(1), HostEvent::Message(1, Up::Hello("Ann".into()))]);
        assert!(host.send(1, &Down::Welcome(7)));
        assert_eq!(gather(|| client.poll(), |e| !e.is_empty()), [ClientEvent::Message(Down::Welcome(7))]);
    }

    #[test]
    fn messages_keep_their_order_and_each_player_gets_a_broadcast() {
        let host: Host<Up, Down> = Host::listen("127.0.0.1:0").unwrap();
        let address = format!("127.0.0.1:{}", host.port());
        let a: Client<Up, Down> = Client::connect(&address).unwrap();
        let b: Client<Up, Down> = Client::connect(&address).unwrap();
        gather(|| host.poll(), |e| e.len() >= 2);
        assert_eq!(host.peers(), [1, 2]);
        for n in 0..50 {
            host.broadcast(&Down::Tick(n));
        }
        for client in [&a, &b] {
            let got = gather(|| client.poll(), |e| e.len() >= 50);
            assert_eq!(got, (0..50).map(|n| ClientEvent::Message(Down::Tick(n))).collect::<Vec<_>>());
        }
    }

    #[test]
    fn a_large_message_arrives_whole() {
        let (host, client) = pair();
        let big: Vec<u8> = (0..300_000).map(|n| (n % 251) as u8).collect();
        client.send(&Up::Big(big.clone()));
        let events = gather(|| host.poll(), |e| e.len() >= 2);
        assert_eq!(events[1], HostEvent::Message(1, Up::Big(big)));
    }

    #[test]
    fn a_player_leaving_is_noticed_and_no_longer_reachable() {
        let (host, client) = pair();
        gather(|| host.poll(), |e| !e.is_empty());
        drop(client);
        let events = gather(|| host.poll(), |e| e.contains(&HostEvent::Left(1)));
        assert!(events.contains(&HostEvent::Left(1)));
        assert!(host.peers().is_empty());
        assert!(!host.send(1, &Down::Tick(0)));
    }

    #[test]
    fn the_host_going_away_tells_the_players() {
        let (host, client) = pair();
        gather(|| host.poll(), |e| !e.is_empty());
        host.kick(1);
        assert_eq!(gather(|| client.poll(), |e| !e.is_empty()), [ClientEvent::Disconnected]);
    }

    #[test]
    fn a_peer_that_sends_nonsense_is_dropped() {
        let host: Host<Up, Down> = Host::listen("127.0.0.1:0").unwrap();
        let mut raw = TcpStream::connect(format!("127.0.0.1:{}", host.port())).unwrap();
        write_frame(&mut raw, b"not json").unwrap();
        let events = gather(|| host.poll(), |e| e.len() >= 2);
        assert_eq!(events, [HostEvent::Joined(1), HostEvent::Left(1)]);
    }

    #[test]
    fn nobody_listening_is_an_error_not_a_hang() {
        let port = { TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port() };
        assert!(Client::<Up, Down>::connect(&format!("127.0.0.1:{port}")).is_err());
    }

    #[test]
    fn frames_carry_their_length_and_oversized_ones_are_refused() {
        let mut wire = Vec::new();
        write_frame(&mut wire, b"hello").unwrap();
        assert_eq!(wire, [5, 0, 0, 0, b'h', b'e', b'l', b'l', b'o']);
        assert_eq!(read_frame(&mut wire.as_slice()).unwrap(), b"hello");
        let huge = (MAX_FRAME as u32 + 1).to_le_bytes();
        assert!(read_frame(&mut huge.as_slice()).is_err());
        assert!(read_frame(&mut [5u8, 0, 0, 0, b'h'].as_slice()).is_err(), "a frame cut short");
    }
}
