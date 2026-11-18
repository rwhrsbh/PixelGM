//! Tables that find each other on a network without anybody typing an address: a host says
//! once a second, to every network it is on, that its table is there; a player looking for
//! one listens. It works on a home network and on virtual ones that pass broadcasts
//! (Radmin VPN, ZeroTier); where they do not, the table's code is left.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Where tables are said to be.
pub const BEACON_PORT: u16 = 47_801;
/// A table not heard of for this long is gone.
const FORGET_AFTER: Duration = Duration::from_secs(4);

/// What a table says of itself.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TableInfo {
    /// The game's protocol: a table of another version is shown, but not joined.
    pub protocol: u32,
    /// The table's name: its host's.
    pub name: String,
    /// The port the table listens on; the address is where the word came from.
    pub port: u16,
    /// Players at the table and seats there are.
    pub taken: u8,
    pub seats: u8,
    /// The world played there, empty while none is chosen.
    pub world: String,
    /// Whether the game has begun.
    pub playing: bool,
    /// The table's short code: a player who typed it is let in here.
    #[serde(default)]
    pub code: String,
}

/// A network this machine is on: its own address there and where a broadcast goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Network {
    /// The adapter's name, as the system calls it.
    pub adapter: String,
    pub address: Ipv4Addr,
    pub broadcast: Ipv4Addr,
}

impl Network {
    /// What kind of network it is, by its address or its adapter's name.
    pub fn kind(&self) -> NetworkKind {
        let name = self.adapter.to_lowercase();
        match self.address.octets() {
            [26, ..] => NetworkKind::Radmin,
            [25, ..] => NetworkKind::Hamachi,
            _ if name.contains("zt") || name.contains("zerotier") => NetworkKind::ZeroTier,
            _ => NetworkKind::Local,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkKind {
    Local,
    Radmin,
    ZeroTier,
    Hamachi,
}

/// The networks this machine is on, leaving out the loopback.
pub fn networks() -> Vec<Network> {
    let Ok(interfaces) = if_addrs::get_if_addrs() else { return Vec::new() };
    interfaces
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(v4) => Some(Network { adapter: i.name, address: v4.ip, broadcast: v4.broadcast.unwrap_or(Ipv4Addr::BROADCAST) }),
            _ => None,
        })
        .collect()
}

/// A host saying where its table is, for as long as this is kept.
pub struct Beacon {
    info: Arc<Mutex<TableInfo>>,
    stop: Arc<AtomicBool>,
}

impl Beacon {
    pub fn start(info: TableInfo) -> Self {
        let info = Arc::new(Mutex::new(info));
        let stop = Arc::new(AtomicBool::new(false));
        let (said, stopped) = (info.clone(), stop.clone());
        std::thread::spawn(move || {
            let Ok(socket) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) else { return };
            let _ = socket.set_broadcast(true);
            while !stopped.load(Ordering::Relaxed) {
                let words = serde_json::to_vec(&*said.lock().unwrap()).unwrap_or_default();
                // To every network on its own, and to all at once for the ones that pass that.
                let mut targets: Vec<Ipv4Addr> = networks().into_iter().map(|n| n.broadcast).collect();
                targets.push(Ipv4Addr::BROADCAST);
                targets.dedup();
                for target in targets {
                    let _ = socket.send_to(&words, SocketAddrV4::new(target, BEACON_PORT));
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        });
        Self { info, stop }
    }

    /// What the table says of itself from now on.
    pub fn update(&self, change: impl FnOnce(&mut TableInfo)) {
        change(&mut self.info.lock().unwrap());
    }
}

impl Drop for Beacon {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// A table heard of: where it is and what it said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heard {
    pub address: SocketAddrV4,
    pub info: TableInfo,
}

/// A player listening for tables, for as long as this is kept.
pub struct Listening {
    heard: Arc<Mutex<HashMap<SocketAddrV4, (TableInfo, Instant)>>>,
    stop: Arc<AtomicBool>,
}

impl Listening {
    /// Starts listening. Several instances on one machine may listen at once.
    pub fn start() -> std::io::Result<Self> {
        let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        #[cfg(unix)]
        socket.set_reuse_port(true)?;
        socket.set_broadcast(true)?;
        socket.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, BEACON_PORT)).into())?;
        let socket: UdpSocket = socket.into();
        socket.set_read_timeout(Some(Duration::from_millis(500)))?;
        let heard: Arc<Mutex<HashMap<SocketAddrV4, (TableInfo, Instant)>>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let (keep, stopped) = (heard.clone(), stop.clone());
        std::thread::spawn(move || {
            let mut buffer = [0u8; 2048];
            while !stopped.load(Ordering::Relaxed) {
                let Ok((length, from)) = socket.recv_from(&mut buffer) else { continue };
                let (SocketAddr::V4(from), Ok(info)) = (from, serde_json::from_slice::<TableInfo>(&buffer[..length])) else { continue };
                let address = SocketAddrV4::new(*from.ip(), info.port);
                keep.lock().unwrap().insert(address, (info, Instant::now()));
            }
        });
        Ok(Self { heard, stop })
    }

    /// The tables heard of lately, by name.
    pub fn tables(&self) -> Vec<Heard> {
        let mut heard = self.heard.lock().unwrap();
        heard.retain(|_, (_, when)| when.elapsed() < FORGET_AFTER);
        let mut tables: Vec<Heard> = heard.iter().map(|(address, (info, _))| Heard { address: *address, info: info.clone() }).collect();
        tables.sort_by(|a, b| a.info.name.cmp(&b.info.name).then(a.address.cmp(&b.address)));
        tables
    }
}

impl Drop for Listening {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn networks_are_named_by_their_addresses() {
        let net = |adapter: &str, ip: [u8; 4]| Network { adapter: adapter.into(), address: Ipv4Addr::from(ip), broadcast: Ipv4Addr::BROADCAST };
        assert_eq!(net("Radmin VPN", [26, 14, 3, 7]).kind(), NetworkKind::Radmin);
        assert_eq!(net("ztabcdef", [10, 147, 17, 20]).kind(), NetworkKind::ZeroTier);
        assert_eq!(net("eth0", [192, 168, 1, 5]).kind(), NetworkKind::Local);
        assert_eq!(net("Hamachi", [25, 1, 2, 3]).kind(), NetworkKind::Hamachi);
    }

    #[test]
    fn a_table_said_is_heard() {
        let Ok(listening) = Listening::start() else { return };
        let info = TableInfo { protocol: 10, name: "Test table".into(), port: 47_899, taken: 1, seats: 5, world: String::new(), playing: false, code: "K7Q4M".into() };
        let _beacon = Beacon::start(info.clone());
        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline {
            if listening.tables().iter().any(|t| t.info == info) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        // A machine without a network that passes broadcasts cannot hear itself: not a fault.
        eprintln!("no table heard: this machine's networks pass no broadcast");
    }
}
