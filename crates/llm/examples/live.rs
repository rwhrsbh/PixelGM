//! One real request through the rotation, to check the wire format against the live API.
//! Usage: cargo run -p llm --example live -- <file with one key per line>

use llm::{Dispatcher, GenRequest, ThinkingSetting, Tier, UreqTransport};
use std::time::Duration;

fn main() {
    let path = std::env::args().nth(1).expect("path to a keys file");
    let dispatcher = Dispatcher::new(UreqTransport::new(Duration::from_secs(120)));
    for key in std::fs::read_to_string(path).expect("readable keys file").lines().filter(|l| !l.trim().is_empty()) {
        dispatcher.add_key(key);
    }
    dispatcher.refresh().expect("model list");
    dispatcher.with_pool(|p| println!("chain: {}", p.chain().iter().map(|m| m.id.as_str()).collect::<Vec<_>>().join(" > ")));

    let mut request = GenRequest::prompt("A rogue picks a DC 15 lock with +5 and rolls 9. Reply as JSON {\"success\": bool, \"line\": string}.");
    request.system = Some("You are the game master of a tabletop RPG. Reply with JSON only.".into());
    request.json = true;
    request.thinking = ThinkingSetting::Low;
    match dispatcher.generate(&request, Tier::Gemma) {
        Ok((reply, model)) => println!("{} answered ({:?}): {}", model.id, reply.usage, reply.text),
        Err(e) => println!("failed: {e:?}"),
    }
}
