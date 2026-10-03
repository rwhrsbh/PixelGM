//! A stand-in game master that answers without a network: the same JSON a model would send,
//! built by rule. Tests use it, and so does the game when run with the fake-master flag, so
//! that everything after "the answer arrived" can be exercised without spending quota.

use crate::model::{MAP_H, MAP_W};
use serde_json::{Value, json};

fn hash(x: u32, y: u32, seed: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA6B) ^ seed.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 13)
}

fn seed_of(text: &str) -> u32 {
    text.bytes().fold(2_166_136_261, |h, b| (h ^ b as u32).wrapping_mul(16_777_619))
}

const CAMPFIRE: [&str; 16] = [
    "0000000000000000",
    "0000000000000000",
    "0000000e00000000",
    "000000e3e0000000",
    "000000e3ee000000",
    "00000ee33e000000",
    "00000e333ee00000",
    "0000ee3e33e00000",
    "0000e33ee3e00000",
    "0001eee33ee10000",
    "001a1eeeee1a1000",
    "01aba11111aba100",
    "01abbaaaaabba100",
    "001aabbbbbaa1000",
    "0001111111110000",
    "0000000000000000",
];

const CAT: [&str; 16] = [
    "0000000000000000",
    "0000000000000000",
    "0000000000000000",
    "0000000000000000",
    "0010000010000000",
    "0181000181000000",
    "0188111881000000",
    "0188888881000010",
    "0183888381000181",
    "0188818881000181",
    "0018888810001810",
    "0001888881118100",
    "0001888888888100",
    "0001811811818100",
    "0001811811818100",
    "0000100100101000",
];

/// A broken wall the master drew, which nobody can walk through.
const RUIN: [&str; 16] = [
    "4444444444444444",
    "4333433343334333",
    "4333433343334333",
    "1111111111111111",
    "3343334333433343",
    "3343334333433343",
    "1111111111111111",
    "4333433343334333",
    "4333433343334333",
    "1111111111111111",
    "3343334333433343",
    "3343334333433343",
    "1111111111111111",
    "2222222222222222",
    "2222222222222222",
    "1111111111111111",
];

const MOSS: [&str; 16] = [
    "9999989999999899",
    "9899999977999999",
    "9999997767799999",
    "9999977677799989",
    "9989976777999999",
    "9999997799999999",
    "9999999999989999",
    "9799999999999999",
    "7779999899997799",
    "6779999999977679",
    "9799999999976779",
    "9999989999997799",
    "9999999999999999",
    "9989999977999989",
    "9999999767799999",
    "9999999977999999",
];

fn rows(picture: &[&str]) -> Value {
    json!(picture)
}

/// A walled-in clearing with scattered rocks and a pond; `gaps` are the exit cells.
fn clearing(seed: u32, gaps: &[(usize, usize)]) -> Vec<String> {
    let pond = (4 + hash(1, 0, seed) as usize % 16, 3 + hash(2, 0, seed) as usize % 7);
    (0..MAP_H)
        .map(|y| {
            (0..MAP_W)
                .map(|x| {
                    let edge = x == 0 || y == 0 || x == MAP_W - 1 || y == MAP_H - 1;
                    let in_pond = (pond.0..pond.0 + 5).contains(&x) && (pond.1..pond.1 + 3).contains(&y);
                    let roll = hash(x as u32, y as u32, seed);
                    if gaps.contains(&(x, y)) {
                        ','
                    } else if edge {
                        'T'
                    } else if in_pond {
                        '~'
                    } else if roll % 19 == 0 {
                        'r'
                    } else if roll % 7 == 0 {
                        'm'
                    } else {
                        '.'
                    }
                })
                .collect()
        })
        .collect()
}

fn legend() -> Value {
    json!({ ".": "grass", ",": "dirt", "T": "tree", "~": "water", "r": "rock", "m": "moss", "#": "ruin" })
}

/// A short broken wall north of the camp, so that every test world has a drawn tile that
/// blocks the way.
fn with_ruin(mut map: Vec<String>) -> Vec<String> {
    let row = &mut map[4];
    let mut cells: Vec<char> = row.chars().collect();
    for x in 4..9 {
        cells[x] = '#';
    }
    *row = cells.into_iter().collect();
    map
}

/// The answer to "create a new world".
pub fn world(request: &str) -> String {
    let seed = seed_of(request);
    let (east, south) = ((MAP_W - 1, 8), (14, MAP_H - 1));
    json!({
        "title": "Test Hollow",
        "summary": format!("A quiet hollow made by rule for testing. The host asked for: {request}"),
        "tone": "calm",
        "palette": { "7": "#6fa13a", "e": "#e0703a", "g": "#b040c0" },
        "tiles": [{ "id": "moss", "name": "mossy stone", "walkable": true, "rows": rows(&MOSS) }, { "id": "ruin", "name": "broken wall", "walkable": false, "rows": rows(&RUIN) }],
        "location": {
            "id": "camp",
            "name": "The Camp",
            "description": "A ring of old trees around a fire that someone keeps alive.", "light": "dusk",
            "legend": legend(),
            "map": with_ruin(clearing(seed, &[east, south])),
            "spawn": [8, 12],
            "sprites": [{ "id": "campfire", "rows": rows(&CAMPFIRE) }, { "id": "cat", "rows": rows(&CAT) }],
            "props": [{ "id": "fire", "name": "Campfire", "description": "It crackles.", "x": 10, "y": 12, "blocking": true, "sprite": "campfire",
                "actions": [{ "label": "Warm your hands", "outcome": "The heat seeps into your fingers. Someone fed this fire not long ago.", "effects": [{ "op": "fact", "text": "Someone tends the campfire." }] }] }],
            "creatures": [{ "id": "cat", "name": "Grey cat", "description": "It watches you.", "x": 12, "y": 11, "sprite": "cat", "attitude": "friendly",
                "actions": [
                    { "label": "Pet the cat", "outcome": "The cat leans into your hand and purrs.", "effects": [{ "op": "describe", "id": "self", "description": "It purrs when you look at it." }] },
                    { "label": "Shoo it away", "outcome": "The cat slips away between the trees, leaving a tuft of fur.", "effects": [{ "op": "remove", "id": "self" }, { "op": "give", "name": "Cat hair", "description": "A tuft of grey fur." }] }
                ] }],
            "exits": [
                { "x": east.0, "y": east.1, "to": "east_road", "label": "A road east" },
                { "x": south.0, "y": south.1, "to": "south_marsh", "label": "A path south" }
            ],
            "neighbors": [
                { "id": "east_road", "name": "The East Road", "description": "A rutted road between hedges." },
                { "id": "south_marsh", "name": "The South Marsh", "description": "Wet ground and reeds." }
            ]
        }
    })
    .to_string()
}

/// The answer to "the party does this".
pub fn turn(said: &str) -> String {
    json!({
        "narration": format!("You said: {said}. The hollow listens, and something in it answers."),
        "effects": [{ "op": "fact", "text": "The party made itself heard." }],
        "options": [
            { "label": "Search the ground", "outcome": "Under the leaves you find a coin, worn smooth.", "effects": [{ "op": "give", "name": "Old coin", "description": "Worn smooth." }] },
            { "label": "Wait and listen", "outcome": "Nothing comes. The trees creak.", "effects": [] }
        ]
    })
    .to_string()
}

/// The answer to "translate these": each text marked with the language asked for.
pub fn translation(texts: &[String], language: &str) -> String {
    json!({ "texts": texts.iter().map(|t| format!("[{language}] {t}")).collect::<Vec<_>>() }).to_string()
}

/// The answer to "offer this hero abilities".
pub fn feats(secret: bool) -> String {
    let all = json!([
        { "name": "Ember hands", "description": "Fire leaps from your palms.", "dice": "2d6", "range": 30, "save": "dex", "uses": 2 },
        { "name": "Camp medic", "description": "You know where to press.", "dice": "1d8", "heals": true, "range": 5, "uses": 2 },
        { "name": "Thick skin", "description": "Scars on scars.", "ac": 1, "hp": 2 }
    ]);
    let feats = if secret { json!([all[0]]) } else { all };
    json!({ "feats": feats }).to_string()
}

/// The answer to an improvised action in a fight.
pub fn stunt(said: &str) -> String {
    json!({
        "narration": format!("You try it: {said}."),
        "ability": "dex", "skill": "acrobatics", "dc": 8, "target": "wolf", "damage": "1d6", "heal": "",
        "success": "It works better than it had any right to.", "failure": "It does not come off."
    })
    .to_string()
}

/// The answer to "draw this hero": a figure unlike the stock one.
pub fn portrait() -> String {
    json!({ "rows": [
        "0000011111000000", "0000199999100000", "0000199999100000", "0000122222100000",
        "0000121212100000", "0000122222100000", "0000011111000000", "0001166666110000",
        "0016666666661000", "0016166666161000", "0012166666121000", "0001166666110000",
        "0000166166100000", "0000166166100000", "0000133133100000", "0000111011100000"
    ] })
    .to_string()
}

/// The answer to a human master's "make me a creature".
pub fn creature(request: &str) -> String {
    let name = request.trim().chars().take(24).collect::<String>();
    json!({
        "id": request, "name": if name.is_empty() { "Thing".to_string() } else { name },
        "description": "Made to order.",
        "rows": ["0330", "3333", "3003"],
        "stats": { "ac": 12, "hp": 9, "speed": 30, "attacks": [{ "name": "Bite", "bonus": 3, "damage": "1d6+1", "range": 5 }], "xp": 25 }
    })
    .to_string()
}

/// The answer to "write the older story up as a chapter".
pub fn chapter(lines: usize) -> String {
    json!({ "chapter": format!("A chapter covering {lines} lines of the story."), "facts": ["A chapter was written."] }).to_string()
}

/// The answer to "draw the place `id`, the party walks in from `from`".
pub fn location(id: &str, name: &str, from: &str) -> String {
    let seed = seed_of(id);
    let back = (0, 8);
    let onward = (MAP_W - 1, 5);
    let further = format!("{id}_beyond");
    json!({
        "id": id,
        "name": name,
        "description": format!("{name}, drawn by rule. The way back is to the west."),
        "legend": legend(),
        "map": clearing(seed, &[back, onward]),
        "spawn": [2, 8],
        "props": [{ "id": "fire", "name": "Cold fire", "description": "Ashes.", "x": 15, "y": 8, "blocking": true, "sprite": "campfire",
            "actions": [{ "label": "Dig through the ashes", "outcome": "You sift the ashes.", "effects": [
                { "op": "check", "ability": "wis", "skill": "perception", "dc": 12,
                  "success": { "narration": "Something glints: a ring.", "effects": [{ "op": "give", "name": "Ash ring", "description": "Warm to the touch." }, { "op": "xp", "amount": 25 }] },
                  "failure": { "narration": "Only soot, and it stings your eyes.", "effects": [{ "op": "damage", "amount": "1" }] } }
            ] }] }],
        "creatures": [{ "id": "wolf", "name": "Wolf", "description": "Lean and hungry.", "x": 20, "y": 8, "sprite": "cat", "attitude": "hostile", "monster": "wolf" }],
        "exits": [
            { "x": back.0, "y": back.1, "to": from, "label": "Back" },
            { "x": onward.0, "y": onward.1, "to": further, "label": "Onward" }
        ],
        "neighbors": [{ "id": further, "name": format!("Beyond {name}"), "description": "Further on." }]
    })
    .to_string()
}
