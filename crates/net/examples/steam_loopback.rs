//! Checks the Steam wire as far as one account can: opens a lobby, joins it from the same
//! process and passes a small and a large message each way. Needs Steam running.
//!
//!     cargo run -p net --example steam_loopback

use net::{Client, ClientEvent, Host, HostEvent, Steam};
use std::time::{Duration, Instant};

fn wait_for<T>(what: &str, seconds: u64, mut look: impl FnMut() -> Option<T>) -> T {
    let started = Instant::now();
    loop {
        if let Some(found) = look() {
            println!("ok   {what} ({} ms)", started.elapsed().as_millis());
            return found;
        }
        if started.elapsed() > Duration::from_secs(seconds) {
            println!("FAIL {what}: nothing after {seconds} s");
            std::process::exit(1);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn main() {
    let steam = match Steam::init() {
        Ok(steam) => steam,
        Err(e) => {
            println!("FAIL Steam is not available: {e}");
            std::process::exit(1);
        }
    };
    println!("ok   Steam answers; the player is {}", steam.name());

    let host: Host<String, String> = Host::listen_steam(&steam);
    let lobby = wait_for("the lobby opens", 15, || host.lobby()).unwrap_or_else(|e| {
        println!("FAIL Steam refused the lobby: {e}");
        std::process::exit(1);
    });
    println!("     lobby {lobby}; friends hosting right now: {:?}", steam.friends_hosting());

    let guest: Client<String, String> = Client::connect_steam(&steam, lobby);
    assert!(guest.send(&"hello".to_string()));
    let mut heard = Vec::new();
    let peer = wait_for("the host hears the guest", 20, || {
        heard.extend(host.poll());
        heard.iter().find_map(|event| match event {
            HostEvent::Message(peer, text) if text == "hello" => Some(*peer),
            _ => None,
        })
    });
    assert!(heard.contains(&HostEvent::Joined(peer)), "a guest is announced before it speaks: {heard:?}");

    assert!(host.send(peer, &"hi".to_string()));
    wait_for("the guest hears the host", 20, || {
        for event in host.poll() {
            println!("     host event: {event:?}");
        }
        guest.poll().into_iter().find_map(|event| {
            println!("     guest event: {event:?}");
            matches!(&event, ClientEvent::Message(text) if text == "hi").then_some(())
        })
    });

    let big = "world ".repeat(150_000);
    assert!(host.send(peer, &big));
    wait_for("the guest receives 900 KB from the host", 30, || guest.poll().into_iter().find_map(|event| matches!(&event, ClientEvent::Message(text) if *text == big).then_some(())));

    assert!(guest.send(&big));
    wait_for("the host receives 900 KB from the guest", 30, || host.poll().into_iter().find_map(|event| matches!(&event, HostEvent::Message(_, text) if *text == big).then_some(())));

    drop(guest);
    drop(host);
    std::thread::sleep(Duration::from_millis(300));
    println!("ok   both ends closed");
}
