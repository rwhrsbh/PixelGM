//! A port opened on the home router for a table, so that a friend across the internet can
//! come in by the table's code without Steam. Asked of the router by UPnP, which most home
//! routers answer; one that does not leaves Steam, a virtual network or the router's own
//! settings.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long the router is waited for: the table opens whatever it says.
const ASK_FOR: Duration = Duration::from_secs(3);

/// What the router answered: the address the internet sees, or why there is none.
pub type Opened = Arc<Mutex<Option<Result<Ipv4Addr, String>>>>;

/// A port held open on the router while this is kept.
pub struct Upnp {
    pub opened: Opened,
    port: u16,
}

impl Upnp {
    /// Asks the router, on a thread of its own, to send `port` from the internet to this
    /// machine's `local` address.
    pub fn open(local: Ipv4Addr, port: u16) -> Self {
        let opened: Opened = Arc::default();
        let answer = opened.clone();
        std::thread::spawn(move || {
            let result = map(local, port);
            *answer.lock().unwrap() = Some(result);
        });
        Self { opened, port }
    }
}

fn map(local: Ipv4Addr, port: u16) -> Result<Ipv4Addr, String> {
    let options = igd_next::SearchOptions { timeout: Some(ASK_FOR), ..Default::default() };
    let gateway = igd_next::search_gateway(options).map_err(|e| e.to_string())?;
    gateway.add_port(igd_next::PortMappingProtocol::TCP, port, SocketAddr::V4(SocketAddrV4::new(local, port)), 0, "PixelGM table").map_err(|e| e.to_string())?;
    match gateway.get_external_ip().map_err(|e| e.to_string())? {
        IpAddr::V4(ip) => Ok(ip),
        IpAddr::V6(_) => Err("the router speaks only IPv6".to_string()),
    }
}

impl Drop for Upnp {
    fn drop(&mut self) {
        // Closed again when the table is, if it was opened.
        if matches!(*self.opened.lock().unwrap(), Some(Ok(_))) {
            let port = self.port;
            std::thread::spawn(move || {
                let options = igd_next::SearchOptions { timeout: Some(ASK_FOR), ..Default::default() };
                if let Ok(gateway) = igd_next::search_gateway(options) {
                    let _ = gateway.remove_port(igd_next::PortMappingProtocol::TCP, port);
                }
            });
        }
    }
}
