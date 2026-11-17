//! Whole scenes played through the way the game plays them: a player's words, the master's
//! answer as it would come, and what the world is afterwards. Each one is a thing players
//! do at the table that once went wrong or could.

use crate::fake;
use crate::{TurnDraft, World, WorldDraft, build_world};
use rules::Character;

/// The test world with one hero, standing in a place of the given map.
fn world_in(map: &[&str], spawn: (usize, usize), props: serde_json::Value) -> World {
    let draft: WorldDraft = serde_json::from_str(&fake::world("scene")).unwrap();
    let mut w = build_world(&draft, "scene", "English").unwrap().0;
    let skills = vec!["athletics".to_string(), "survival".to_string()];
    w.party.push(Character::create("Brak", "", "human", "fighter", [15, 12, 14, 8, 10, 13], &skills, &[]).unwrap());
    let place = serde_json::json!({
        "id": "lake", "name": "The Lake", "description": "Still water with an island.",
        "legend": { ".": "grass", "T": "tree", "~": "water", "r": "rock" },
        "map": map, "spawn": [spawn.0, spawn.1], "props": props, "creatures": [],
        "exits": [{ "x": 0, "y": spawn.1, "to": w.current.clone(), "label": "Back" }]
    });
    let draft: crate::LocationDraft = serde_json::from_str(&place.to_string()).unwrap();
    let from = w.current.clone();
    w.add_location(&draft, "lake", Some(&from)).unwrap();
    w.travel("lake");
    w.player = crate::Player::in_cell(spawn.0, spawn.1);
    w
}

/// 30 by 16: a lake in the middle with an island, a tree on the shore.
fn lake() -> Vec<&'static str> {
    vec![
        "..............................",
        "..............................",
        "..............................",
        "........~~~~~~~~~~~~~~........",
        "........~~~~~~~~~~~~~~........",
        "........~~~~~~~~~~~~~~........",
        "........~~~~~~~~~~~~~~........",
        "....T...~~~~~~...~~~~~........",
        "........~~~~~~...~~~~~........",
        "........~~~~~~...~~~~~........",
        "........~~~~~~~~~~~~~~........",
        "........~~~~~~~~~~~~~~........",
        "........~~~~~~~~~~~~~~........",
        "..............................",
        "..............................",
        "..............................",
    ]
}

fn sixteen(c: char) -> Vec<String> {
    (0..16).map(|_| c.to_string().repeat(16)).collect()
}

fn reachable(w: &World, cell: (usize, usize)) -> bool {
    let (x, y) = w.player.cell();
    let here = w.here();
    let can = crate::path::reachable((x as usize, y as usize), |(cx, cy)| here.blocked(cx as i32, cy as i32, &w.tiles));
    // A blocking thing is reached by standing beside it.
    [(0i32, -1i32), (1, 0), (0, 1), (-1, 0)].iter().any(|(dx, dy)| {
        let (nx, ny) = (cell.0 as i32 + dx, cell.1 as i32 + dy);
        nx >= 0 && ny >= 0 && can.get(ny as usize).and_then(|row| row.get(nx as usize)).copied().unwrap_or(false)
    })
}

fn answer(w: &mut World, said: &str, json: serde_json::Value) -> (crate::Turn, Vec<String>) {
    let draft: TurnDraft = serde_json::from_value(json).unwrap();
    w.take_turn(said, &draft)
}

#[test]
fn a_tree_cut_down_becomes_a_bridge_to_the_chest_on_the_island() {
    let chest = serde_json::json!([{ "id": "chest", "name": "Sunken chest", "description": "On the island.", "x": 15, "y": 8, "blocking": true, "sprite": "chest",
        "contents": [{ "name": "Silver cup", "kind": "junk" }] }]);
    let mut w = world_in(&lake(), (3, 8), chest);
    assert!(!reachable(&w, (15, 8)), "the island is cut off by water");
    let asked = crate::prompt::turn(&w, "I cut down the tree and make a bridge to the island", None);
    assert!(asked.contains("chest (Sunken chest): cannot be reached on foot from here"), "{asked}");
    assert!(asked.contains("The party stands at (3, 8)"));

    // The tree beside the hero falls; the logs go into the bag.
    let (turn, notes) = answer(&mut w, "I cut down the tree", serde_json::json!({
        "narration": "You fell the tree and lop its branches.",
        "effects": [
            { "op": "tile", "x": 4, "y": 7, "tile": "grass" },
            { "op": "give", "name": "Logs", "description": "Rough trunks.", "kind": "junk", "size": [2, 2], "rows": sixteen('3').into_iter().chain(sixteen('3')).collect::<Vec<_>>() }
        ],
        "gained": ["Logs"]
    }));
    assert!(notes.iter().all(|n| !n.starts_with("tile")), "{notes:?}");
    assert!(!turn.narration.is_empty());
    assert_eq!(w.here().tile_id(4, 7), Some("grass"), "the tree is gone from its cell");
    assert!(w.story.bags[0].inventory.iter().any(|i| i.name == "Logs"), "the logs are in the bag");

    // The bridge is a kind of ground the master draws, laid on the water between shore and island.
    let (_, notes) = answer(&mut w, "I lay the logs across the water", serde_json::json!({
        "narration": "The logs bridge the water to the island.",
        "tiles": [{ "id": "log_bridge", "name": "Log bridge", "walkable": true, "rows": sixteen('3') }],
        "effects": [
            { "op": "tile", "x": 8, "y": 8, "tile": "log_bridge", "to": [13, 8] },
            { "op": "take", "name": "Logs" }
        ],
        "lost": ["Logs"]
    }));
    assert!(notes.iter().all(|n| !n.starts_with("tile")), "{notes:?}");
    assert!((8..=13).all(|x| w.here().tile_id(x, 8) == Some("log_bridge")), "{:?}", w.here().map[8]);
    assert!(!w.story.bags[0].inventory.iter().any(|i| i.name == "Logs"), "the logs were used up");
    assert!(reachable(&w, (15, 8)), "the island can be walked to");
    let asked = crate::prompt::turn(&w, "I walk to the chest", None);
    assert!(asked.contains("chest (Sunken chest): ") && !asked.contains("chest (Sunken chest): cannot be reached"), "{asked}");

    // And what is in the chest is the hero's once it is opened.
    let opened = w.open_chest(0, "chest", crate::bag::Opening::Open, &mut rules::Rng::seeded(1));
    assert!(matches!(opened, Some(crate::bag::Opened::Took(ref names)) if names == &vec!["Silver cup".to_string()]), "{opened:?}");

    // Saved and loaded, the bridge is still there.
    let back: World = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
    assert_eq!(back.locations["lake"].tile_id(10, 8), Some("log_bridge"));
    assert!(back.tiles.get("log_bridge").is_some_and(|t| t.walkable));
}

#[test]
fn a_bridge_whose_picture_cannot_be_read_is_sent_back_to_the_master_not_lost() {
    let w = world_in(&lake(), (7, 8), serde_json::json!([]));
    let bad = serde_json::json!({
        "narration": "You lay a bridge.",
        "tiles": [{ "id": "log_bridge", "name": "Log bridge", "walkable": true, "rows": sixteen('0') }],
        "effects": [{ "op": "tile", "x": 8, "y": 8, "tile": "log_bridge", "to": [13, 8] }]
    });
    let draft: TurnDraft = serde_json::from_value(bad).unwrap();
    let faults = w.faults(&draft);
    assert!(faults.iter().any(|f| f.contains("log_bridge")), "{faults:?}");
}

#[test]
fn the_party_steps_onto_a_bridge_laid_in_the_same_answer_whatever_the_order() {
    let mut w = world_in(&lake(), (7, 8), serde_json::json!([]));
    answer(&mut w, "I cross on the logs", serde_json::json!({
        "narration": "You lay the logs and step onto them.",
        "tiles": [{ "id": "log_bridge", "name": "Log bridge", "walkable": true, "rows": sixteen('3') }],
        "effects": [
            { "op": "move", "id": "party", "x": 10, "y": 8 },
            { "op": "tile", "x": 8, "y": 8, "tile": "log_bridge", "to": [13, 8] }
        ]
    }));
    assert_eq!(w.moves, vec![(0, (10, 8))], "on the bridge, not left on the shore");
}

#[test]
fn a_tree_that_is_a_thing_is_cut_down_by_removing_it() {
    let tree = serde_json::json!([{ "id": "old_oak", "name": "Old oak", "description": "Thick.", "x": 6, "y": 8, "blocking": true, "sprite": "chest" }]);
    let mut w = world_in(&lake(), (5, 8), tree);
    let (_, notes) = answer(&mut w, "I chop the oak", serde_json::json!({
        "narration": "The oak falls.",
        "effects": [{ "op": "remove", "id": "old_oak" }]
    }));
    assert!(notes.is_empty(), "{notes:?}");
    assert!(!w.here().props.iter().any(|p| p.id == "old_oak"));
    assert!(!w.here().blocked(6, 8, &w.tiles), "its cell can be walked on");
}

/// Ann (the host) and Bob both at the lake; Carl far off in the camp, with logs of his own.
fn three_at_the_lake() -> World {
    let mut w = world_in(&lake(), (3, 8), serde_json::json!([]));
    let skills = vec!["athletics".to_string(), "survival".to_string()];
    for name in ["Bob's hero", "Carl's hero"] {
        w.party.push(Character::create(name, "", "human", "fighter", [15, 12, 14, 8, 10, 13], &skills, &[]).unwrap());
    }
    w.owners = vec!["Ann".into(), "Bob".into(), "Carl".into()];
    w.present = w.owners.clone();
    let camp = w.locations.keys().find(|k| *k != "lake").unwrap().clone();
    w.places.insert("Bob".into(), "lake".into());
    w.places.insert("Carl".into(), camp);
    w.ensure_bags();
    let carls: crate::Item = serde_json::from_value(serde_json::json!({ "name": "Logs", "description": "Carl's own firewood.", "count": 1 })).unwrap();
    w.story.bags[2].receive(carls);
    w
}

#[test]
fn another_players_logs_are_theirs_until_they_drop_them() {
    let mut w = three_at_the_lake();
    // Bob cuts the tree: the logs are his.
    w.acting = Some(1);
    w.actors = vec![1];
    answer(&mut w, "Bob's hero: I cut down the tree", serde_json::json!({
        "narration": "Bob's hero fells the tree.",
        "effects": [{ "op": "tile", "x": 4, "y": 7, "tile": "grass" }, { "op": "give", "name": "Logs", "kind": "junk", "rows": sixteen('3') }]
    }));
    let logs = |w: &World, hero: usize| w.story.bags[hero].inventory.iter().filter(|i| i.name == "Logs").count();
    assert_eq!((logs(&w, 0), logs(&w, 1), logs(&w, 2)), (0, 1, 1), "into the bag of the one who cut");
    // The master is told who carries them and that they are not Ann's to use.
    let asked = crate::prompt::turn(&w, "Ann's hero: I lay the logs across", None);
    assert!(asked.lines().any(|l| l.contains("Bob's hero") && l.contains("Logs")) && !asked.contains("Carl's hero"));
    assert!(crate::prompt::play_prompt(&w).contains("Each hero uses only what is in their OWN bag"));

    // Ann tries to lay Bob's logs: they stay in Bob's bag, whatever the master wrote.
    w.acting = Some(0);
    w.actors = vec![0];
    let bridge = serde_json::json!({
        "narration": "You lay the logs.",
        "tiles": [{ "id": "log_bridge", "name": "Log bridge", "walkable": true, "rows": sixteen('3') }],
        "effects": [{ "op": "take", "name": "Logs", "from": "Bob's hero" }]
    });
    let (_, notes) = answer(&mut w, "Ann's hero: I lay Bob's logs across", bridge.clone());
    assert!(notes.iter().any(|n| n.contains("carried by Bob's hero")), "{notes:?}");
    assert_eq!((logs(&w, 0), logs(&w, 1), logs(&w, 2)), (0, 1, 1), "Bob's logs are not Ann's to use");

    // Bob drops them himself; Ann picks them up and lays them: now they are gone.
    let at = w.story.bags[1].inventory.iter().position(|i| i.name == "Logs").unwrap();
    assert!(w.drop_item(1, at, (4, 8)));
    let n = w.lying_at(0, "Logs", (4, 8)).unwrap();
    assert!(w.pick_up(0, n));
    let (_, notes) = answer(&mut w, "Ann's hero: I lay the logs across", bridge);
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!((logs(&w, 0), logs(&w, 1), logs(&w, 2)), (0, 0, 1));

    // Both acting together may use what either of them carries.
    w.story.bags[1].receive(serde_json::from_value(serde_json::json!({ "name": "Rope", "description": "", "count": 1 })).unwrap());
    w.actors = vec![0, 1];
    let (_, notes) = answer(&mut w, "Ann's hero and Bob's hero: we tie the logs with Bob's rope", serde_json::json!({ "narration": "Tied.", "effects": [{ "op": "take", "name": "Rope" }] }));
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn what_a_hero_far_off_carries_is_not_used_here() {
    let mut w = three_at_the_lake();
    w.acting = Some(0);
    w.actors = vec![0];
    let (_, notes) = answer(&mut w, "Ann's hero: I lay logs across", serde_json::json!({
        "narration": "You have no logs.",
        "effects": [{ "op": "take", "name": "Logs" }]
    }));
    assert!(notes.iter().any(|n| n.contains("take")), "{notes:?}");
    assert_eq!(w.story.bags[2].inventory.iter().filter(|i| i.name == "Logs").count(), 1, "Carl's logs are not taken from the camp");
}

#[test]
fn a_wound_from_the_story_falls_on_the_one_who_did_it_or_the_one_named() {
    let mut w = three_at_the_lake();
    let hp = |w: &World| (w.party[0].hp, w.party[1].hp, w.party[2].hp);
    let full = hp(&w);
    // Bob grabs a coal from the fire: Bob's hero is burnt, not Ann's.
    w.acting = Some(1);
    w.actors = vec![1];
    answer(&mut w, "Bob's hero: I grab the coal", serde_json::json!({ "narration": "It burns.", "effects": [{ "op": "damage", "id": "party", "amount": "3" }] }));
    assert_eq!(hp(&w), (full.0, full.1 - 3, full.2));
    // Named, the named one.
    w.acting = Some(0);
    w.actors = vec![0];
    answer(&mut w, "Ann's hero: I push Bob into the thorns", serde_json::json!({ "narration": "Thorns.", "effects": [{ "op": "damage", "id": "Bob's hero", "amount": "2" }] }));
    assert_eq!(hp(&w), (full.0, full.1 - 5, full.2));
    // Everyone here: Ann and Bob, not Carl far off.
    answer(&mut w, "Ann's hero: I set off the trap", serde_json::json!({ "narration": "Darts fly.", "effects": [{ "op": "damage", "id": "everyone", "amount": "1" }] }));
    assert_eq!(hp(&w), (full.0 - 1, full.1 - 6, full.2));
}

#[test]
fn a_chest_the_story_brings_up_holds_what_it_said_and_opens_to_its_key() {
    let mut w = three_at_the_lake();
    w.acting = Some(1);
    w.actors = vec![1];
    let (_, notes) = answer(&mut w, "Bob's hero: I dive and pull up what glints", serde_json::json!({
        "narration": "You drag a small locked box ashore, and a key on a chain.",
        "effects": [
            { "op": "spawn", "kind": "prop", "id": "box", "name": "Small box", "description": "Locked.", "x": 5, "y": 8, "sprite": "chest", "blocking": true, "lock": 15, "key": "Chain key",
              "contents": [{ "name": "Pearl", "kind": "junk", "rows": sixteen('4') }] },
            { "op": "give", "name": "Chain key", "kind": "key", "rows": sixteen('5') }
        ],
        "gained": ["Chain key"]
    }));
    assert!(notes.is_empty(), "{notes:?}");
    let opened = w.open_chest(0, "box", crate::bag::Opening::Open, &mut rules::Rng::seeded(1));
    assert!(matches!(opened, Some(crate::bag::Opened::Locked)), "Ann has no key: {opened:?}");
    let opened = w.open_chest(1, "box", crate::bag::Opening::Open, &mut rules::Rng::seeded(1));
    assert!(matches!(opened, Some(crate::bag::Opened::Took(ref names)) if names == &vec!["Pearl".to_string()]), "{opened:?}");
    assert!(w.story.bags[1].inventory.iter().any(|i| i.name == "Pearl"), "into the bag of the one who opened it");
}

#[test]
fn logs_dropped_by_one_are_picked_up_by_another_and_a_thing_gone_is_not_taken_twice() {
    let mut w = three_at_the_lake();
    w.acting = Some(1);
    w.actors = vec![1];
    answer(&mut w, "Bob's hero: I cut the tree", serde_json::json!({ "narration": "Logs.", "effects": [{ "op": "give", "name": "Logs", "kind": "junk", "rows": sixteen('3') }, { "op": "give", "name": "Rope", "kind": "tool", "rows": sixteen('4') }] }));
    let at = |w: &World, name: &str| w.story.bags[1].inventory.iter().position(|i| i.name == name).unwrap();
    // Bob throws down both; Ann picks up the logs.
    assert!(w.drop_item(1, at(&w, "Logs"), (4, 8)));
    assert!(w.drop_item(1, at(&w, "Rope"), (4, 9)));
    let seen_logs = w.here().items.iter().position(|l| l.item.name == "Logs").unwrap();
    let seen_rope = w.here().items.iter().position(|l| l.item.name == "Rope").unwrap();
    // Ann takes the logs first; Bob's word for the rope, sent before he knew, still finds the rope.
    let logs = w.lying_at(seen_logs, "Logs", (4, 8)).unwrap();
    assert!(w.pick_up(0, logs));
    let rope = w.lying_at(seen_rope, "Rope", (4, 9)).expect("the rope is still there");
    assert!(w.pick_up(1, rope));
    assert!(w.story.bags[0].inventory.iter().any(|i| i.name == "Logs") && w.story.bags[1].inventory.iter().any(|i| i.name == "Rope"));
    // And the logs, asked for again by someone who did not see them go, are not there.
    assert_eq!(w.lying_at(seen_logs, "Logs", (4, 8)), None);
}

fn token(name: &str, cell: (usize, usize), place: &str) -> crate::Token {
    let at = crate::Player::in_cell(cell.0, cell.1);
    crate::Token { name: name.into(), x: at.x, y: at.y, place: place.into(), facing: (1, 0), vx: 0.0, vy: 0.0 }
}

#[test]
fn logs_are_handed_from_hand_to_hand_only_when_standing_side_by_side_and_build_nothing() {
    let mut w = three_at_the_lake();
    // Bob's logs; Bob stands far from Ann on the other shore.
    w.story.bags[1].receive(serde_json::from_value(serde_json::json!({ "name": "Logs", "description": "", "count": 3 })).unwrap());
    w.others = vec![token("Bob", (25, 8), "lake")];
    let logs = |w: &World, hero: usize| w.story.bags[hero].inventory.iter().find(|i| i.name == "Logs").map_or(0, |i| i.count);

    // Ann wants a bridge: the master offers Bob to hand the logs over, beside Ann.
    w.acting = Some(0);
    w.actors = vec![0];
    let (turn, _) = answer(&mut w, "Ann's hero: I want to build a bridge", serde_json::json!({
        "narration": "You need wood. Bob has logs.",
        "options": [{ "label": "Hand the logs to Ann", "outcome": "Bob hands the logs over.", "for": "Bob's hero", "near": "Brak",
                      "effects": [{ "op": "hand", "name": "Logs", "to": "Brak" }] }]
    }));
    let option = &turn.options[0];
    assert_eq!(option.near.as_deref(), Some("brak"), "Bob walks up to Ann's hero first");
    assert_eq!(option.for_hero.as_deref(), Some("Bob's hero"));

    // Taken from afar: nothing changes hands.
    let home = w.enter("lake", crate::Player::in_cell(25, 8));
    w.acting = Some(1);
    w.actors = vec![1];
    w.choose(option);
    w.leave(home);
    assert_eq!((logs(&w, 0), logs(&w, 1)), (0, 3), "not beside Ann: the logs stay with Bob");

    // Bob walks up beside Ann (3, 8): now they change hands, all three, and no bridge is laid.
    w.others = vec![token("Bob", (4, 8), "lake")];
    let home = w.enter("lake", crate::Player::in_cell(4, 8));
    w.choose(option);
    w.leave(home);
    assert_eq!((logs(&w, 0), logs(&w, 1)), (3, 0), "into Ann's bag");
    assert!((8..=13).all(|x| w.here().tile_id(x, 8) == Some("water")), "handing builds nothing");
    assert_eq!(w.hero_cell(0), (3, 8), "the host's hero is where the host stands");
}

#[test]
fn the_master_is_told_where_the_other_heroes_stand() {
    let mut w = three_at_the_lake();
    w.others = vec![token("Bob", (25, 8), "lake")];
    let asked = crate::prompt::turn(&w, "Brak: I want a bridge", None);
    assert!(asked.contains("- the hero Bob's hero at (25, 8): 31 steps away on foot"), "{asked}");
    assert!(!asked.contains("the hero Carl's hero"), "Carl is somewhere else");
    w.others = vec![token("Bob", (4, 8), "lake")];
    let asked = crate::prompt::turn(&w, "Brak: I want a bridge", None);
    assert!(asked.contains("- the hero Bob's hero at (4, 8): beside the party"), "{asked}");
}
