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

#[test]
fn a_long_thing_is_laid_on_its_side_where_it_fits_and_stood_up_again() {
    let mut w = three_at_the_lake();
    let spear: crate::Item = serde_json::from_value(serde_json::json!({ "name": "Spear", "description": "", "count": 1, "size_cells": [1, 4] })).unwrap();
    w.story.bags[0].receive(spear);
    let at = |w: &World| w.story.bags[0].inventory.iter().position(|i| i.name == "Spear").unwrap();
    let i = at(&w);
    assert_eq!(w.story.bags[0].inventory[i].size(), (1, 4));
    assert!(w.turn_item(0, i));
    let spear = &w.story.bags[0].inventory[i];
    assert_eq!((spear.size(), spear.upright_size()), ((4, 1), (1, 4)), "on its side it takes four across");
    let at_cell = spear.at.expect("it still has a place");
    assert!(w.story.bags[0].fits((4, 1), at_cell, Some(i)), "and it fits where it lies");
    assert!(w.turn_item(0, i));
    assert_eq!(w.story.bags[0].inventory[i].size(), (1, 4), "stood up again");
    // A square thing is the same either way.
    w.story.bags[0].receive(serde_json::from_value(serde_json::json!({ "name": "Box", "description": "", "count": 1, "size_cells": [2, 2] })).unwrap());
    let b = w.story.bags[0].inventory.iter().position(|i| i.name == "Box").unwrap();
    assert!(!w.turn_item(0, b));
    // Saved and loaded, it lies as it was laid.
    assert!(w.turn_item(0, i));
    let back: World = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
    assert!(back.story.bags[0].inventory.iter().find(|i| i.name == "Spear").unwrap().turned);
}

#[test]
fn what_the_master_made_without_a_picture_is_drawn_by_it_and_what_the_game_gives_is_not() {
    let mut w = three_at_the_lake();
    w.outfit(1);
    w.story.bags[1].receive(serde_json::from_value(serde_json::json!({ "name": "Old Map", "description": "Torn.", "count": 1 })).unwrap());
    w.story.bags[1].stow("longsword");
    let names: Vec<String> = w.undrawn_things(&[], 6).into_iter().map(|t| t.name).collect();
    assert!(names.contains(&"Old Map".to_string()) && names.contains(&"Logs".to_string()), "{names:?}");
    assert!(!names.iter().any(|n| crate::bag::is_ration(n) || n.to_lowercase().contains("sword")), "the game's own are not asked for: {names:?}");
    let drawn: crate::bag::ThingsDraft = serde_json::from_str(&fake::things(&[("Old Map".into(), (2, 2))])).unwrap();
    assert_eq!(w.draw_things(&drawn.things), 1);
    let map = w.story.bags[1].inventory.iter().find(|i| i.name == "Old Map").unwrap();
    assert!(!map.rows.is_empty() && map.size() == (2, 2) && map.at.is_some(), "drawn, at the master's size, with a place in the bag");
    assert!(!w.undrawn_things(&[], 6).iter().any(|t| t.name == "Old Map"));
}

/// Plays the master writing up the story the way `compaction` asks, until a turn fits.
fn write_up(w: &mut World, room: usize) -> usize {
    let mut steps = 0;
    while let Some((what, _)) = crate::prompt::compaction(w, room) {
        steps += 1;
        assert!(steps < 200, "the story never came to fit");
        let short = "Chapter: the party did many things; nothing is forgotten.";
        match what {
            crate::prompt::Compaction::Chapter { upto } => {
                let asked = crate::prompt::chapter_upto(w, upto);
                assert!(crate::prompt::tokens(&crate::prompt::play_prompt(w)) + crate::prompt::tokens(&asked) <= room, "the request to write up fits what the model reads");
                w.story.close_chapter_upto(upto, short, &[]);
            }
            crate::prompt::Compaction::Merge { count } => {
                let asked = crate::prompt::merge_chapters(w, count);
                assert!(crate::prompt::tokens(&asked) <= room);
                w.story.merge_chapters(count, short, &[]);
            }
        }
    }
    steps
}

#[test]
fn a_long_story_is_written_up_in_pieces_that_fit_until_a_turn_fits_again() {
    let mut w = three_at_the_lake();
    let base = crate::prompt::tokens(&crate::prompt::play_prompt(&w)) + crate::prompt::tokens(&crate::prompt::turn(&w, "", None));
    // A room the empty story uses less than half of, and a story many times that.
    let room = base * 2 + 2_000;
    assert_eq!(crate::prompt::compaction(&w, room), None, "a short story is left as it is");
    for n in 0..600 {
        w.story.say(crate::Speaker::Player, &format!("Line {n}: I look around the lake once more, slowly, and say what I see."));
        w.story.say(crate::Speaker::Master, &format!("Answer {n}: the water is still, the reeds whisper, and nothing changes but the light."));
    }
    let (_, urgent) = crate::prompt::compaction(&w, room).expect("a long story is written up");
    assert!(urgent, "a turn would not fit at all");
    let steps = write_up(&mut w, room);
    assert!(steps > 1, "in more than one piece");
    let turn = crate::prompt::tokens(&crate::prompt::play_prompt(&w)) + crate::prompt::tokens(&crate::prompt::turn(&w, "", None));
    assert!((turn as f32) < room as f32 * crate::prompt::COMPACT_AT, "a turn fits again");
    assert!(w.story.log.len() >= crate::story::KEPT_LINES, "the newest lines are kept word for word");
}

#[test]
fn chapters_that_pile_up_are_written_up_into_one() {
    let mut w = three_at_the_lake();
    let base = crate::prompt::tokens(&crate::prompt::play_prompt(&w)) + crate::prompt::tokens(&crate::prompt::turn(&w, "", None));
    let room = base * 2 + 2_000;
    // Enough chapters to fill the room whatever the length of the rules.
    for n in 0..base / 30 {
        w.story.chapters.push(format!("Chapter {n}: {}", "a long road walked and many people met on it. ".repeat(6)));
    }
    let many = w.story.chapters.len();
    let (what, _) = crate::prompt::compaction(&w, room).expect("too many chapters");
    assert!(matches!(what, crate::prompt::Compaction::Merge { .. }), "{what:?}");
    write_up(&mut w, room);
    assert!(w.story.chapters.len() < many);
}

#[test]
fn after_the_story_is_written_up_the_master_still_sees_where_everyone_stands_and_who_spoke() {
    let mut w = three_at_the_lake();
    w.others = vec![token("Bob", (4, 8), "lake")];
    w.facing = (1, 0);
    w.story.remember("The ferryman owes Brak a favour.");
    for n in 0..400 {
        w.story.say(crate::Speaker::Player, &format!("Bob's hero: I skim stone number {n} across the lake."));
        w.story.say(crate::Speaker::Master, &format!("Stone {n} skips and sinks."));
    }
    w.story.say(crate::Speaker::Player, "Brak: I ask Bob where he learned that.");
    // A room the rules fit in with some to spare, but not a third of the long story.
    let rules = crate::prompt::tokens(&crate::prompt::play_prompt(&w));
    let story = crate::prompt::tokens(&crate::prompt::turn(&w, "", None));
    write_up(&mut w, rules + story / 3 + 3_000);
    let asked = crate::prompt::turn(&w, "Brak: I look at the water in front of me", None);
    for wanted in [
        "The party stands at (3, 8)",
        "The hero faces east. IN FRONT of the hero: (4, 8)",
        "AROUND the party:",
        "- the hero Bob's hero at (4, 8): beside the party",
        "Brak: I ask Bob where he learned that.",
        "The ferryman owes Brak a favour.",
        "Bob's hero, human fighter",
        "«Brak: I look at the water in front of me»",
    ] {
        assert!(asked.contains(wanted), "{wanted:?} is gone from the request after the story was written up");
    }
    assert!(w.story.chapters.len() >= 1);
}

#[test]
fn a_new_place_is_asked_with_the_look_of_the_world_the_side_the_way_leaves_and_the_story() {
    let mut w = three_at_the_lake();
    let camp = w.locations.keys().find(|k| *k != "lake").unwrap().clone();
    w.story.say(crate::Speaker::Player, "Brak: we follow the thief's tracks east.");
    let stub = w.locations["lake"].exits.iter().find(|e| e.to != camp).map(|e| e.to.clone());
    // The lake's exits lead back to the camp; ask for the camp's onward place instead.
    w.travel(&camp);
    let onward = w.here().exits.iter().find(|e| w.stubs.contains_key(&e.to)).map(|e| e.to.clone()).or(stub).expect("a way to a place not drawn yet");
    let asked = crate::prompt::new_location(&w, &onward, &camp);
    assert!(asked.contains("THE LOOK OF THIS WORLD"), "pictures to draw alike");
    assert!(asked.contains("The way here leaves") && asked.contains("side"), "{}", &asked[asked.find("The way here").unwrap_or(0)..].chars().take(200).collect::<String>());
    assert!(asked.contains("Brak: we follow the thief's tracks east."));
    assert!(asked.contains("PALETTE"));
}

#[test]
fn things_without_a_picture_are_drawn_with_the_next_turn_with_the_palette_at_hand() {
    let mut w = three_at_the_lake();
    w.story.bags[0].receive(serde_json::from_value(serde_json::json!({ "name": "Old Map", "description": "Torn.", "count": 1 })).unwrap());
    let asked = crate::prompt::turn(&w, "Brak: I unfold the map", None);
    assert!(asked.contains("PALETTE (0 is transparent):"), "a turn draws with the world's colours");
    assert!(asked.contains("THINGS WITHOUT A PICTURE YET") && asked.contains("\"Old Map\""), "the thing is asked for with the turn");
    let picture: crate::bag::ThingsDraft = serde_json::from_str(&fake::things(&[("Old Map".into(), (2, 2))])).unwrap();
    let draft: TurnDraft = serde_json::from_value(serde_json::json!({ "narration": "The map shows the lake.", "pictures": picture.things })).unwrap();
    w.take_turn("Brak: I unfold the map", &draft);
    let map = w.story.bags[0].inventory.iter().find(|i| i.name == "Old Map").unwrap();
    assert!(!map.rows.is_empty() && map.size() == (2, 2));
    assert!(!crate::prompt::turn(&w, "x", None).contains("\"Old Map\""), "asked for no more once drawn");
}

#[test]
fn each_hero_reads_the_part_written_for_it_and_only_its_own_choices() {
    let mut w = three_at_the_lake();
    w.places.insert("Carl".into(), "lake".into());
    w.acting = Some(1);
    w.actors = vec![1];
    let (turn, notes) = answer(&mut w, "Bob's hero: I tip the bin over and leave a mess in it", serde_json::json!({
        "narration": "You tip the bin; the stink rises.",
        "options": [{ "label": "Run off", "outcome": "You run.", "effects": [] }],
        "parts": [
            { "to": ["Brak"], "narration": "Bob's hero tips the bin over beside you and fouls it; the stink hits you.", "options": [{ "label": "Shout at Bob", "outcome": "You shout.", "effects": [] }] },
            { "to": ["Carl's hero"], "narration": "From the reeds you see Bob's hero tip a bin and foul it.", "options": [] }
        ]
    }));
    assert!(notes.iter().all(|n| !n.contains("could not")), "{notes:?}");
    // The one who acted: the telling and the choices that are not another's.
    let bob = turn.view_for(Some("Bob's hero"));
    assert_eq!(bob.narration, "You tip the bin; the stink rises.");
    assert_eq!(bob.options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), ["Run off"]);
    // Brak: what happened as it looks to him, and his own choice, which only he may take.
    let brak = turn.view_for(Some("Brak"));
    assert!(brak.narration.contains("Bob's hero tips the bin"));
    assert!(brak.options.iter().any(|o| o.label == "Shout at Bob" && o.for_hero.as_deref() == Some("Brak")));
    // Carl: his part, no choices of anyone else's.
    let carl = turn.view_for(Some("Carl's hero"));
    assert!(carl.narration.contains("From the reeds") && !carl.options.iter().any(|o| o.label == "Shout at Bob"));
    // Alone, a player reads it all; the parts are in the story for the master to remember.
    let all = turn.view_for(None);
    assert!(all.narration.contains("the stink rises") && all.narration.contains("From the reeds"));
    assert!(w.story.log.iter().any(|l| l.text.starts_with("(Brak) ")));
    assert!(turn.every_option().len() == 2, "every choice can be found to be taken");
    assert!(crate::prompt::turn(&w, "x", None).contains("\"parts\""), "the master is told how to write parts");
}

#[test]
fn the_master_has_the_place_drawn_again_as_wanted_and_it_takes_the_old_ones_place() {
    let mut w = three_at_the_lake();
    let camp = w.locations.keys().find(|k| *k != "lake").unwrap().clone();
    let mut seen = w.clone();
    seen.stubs.insert("lake".into(), crate::Stub { name: "The Lake".into(), description: "Still water.".into() });
    seen.redraw_wish = "a frozen lake with a hut on the ice".into();
    let asked = crate::prompt::new_location(&seen, "lake", &camp);
    assert!(asked.contains("THE GAME MASTER HAS THIS PLACE DRAWN AGAIN, AS THEY WANT IT: a frozen lake with a hut on the ice"), "{asked}");
    assert!(asked.contains(&format!("{camp} (")), "its ways out are kept");
    let drawn: crate::LocationDraft = serde_json::from_str(&fake::location("lake", "The Frozen Lake", &camp)).unwrap();
    w.add_location(&drawn, "lake", None).unwrap();
    assert_eq!(w.locations["lake"].name, "The Frozen Lake");
    assert!(w.locations["lake"].exits.iter().any(|e| e.to == camp), "the way back is still there");
    let (x, y) = w.locations["lake"].spawn;
    assert!(!w.locations["lake"].blocked(x as i32, y as i32, &w.tiles), "everyone stands on ground that can be walked");
}

/// Effects as the master writes them, read the way the game reads them.
fn steps(raw: serde_json::Value) -> Vec<crate::story::Effect> {
    crate::story::read_effects(raw.as_array().unwrap(), &mut Vec::new())
}

/// "A lever: a blast, three minutes of quiet, another blast": the second blast waits for its
/// second of play, and happens in the place where the lever is even when the party is away.
#[test]
fn a_lever_blasts_waits_three_minutes_and_blasts_again() {
    let mut w = world_in(&lake(), (2, 2), serde_json::json!([]));
    let blast = |x: i64| serde_json::json!({ "op": "tile", "x": x, "y": 0, "tile": "rock" });
    w.apply(&steps(serde_json::json!([blast(1), { "op": "wait", "seconds": 180 }, blast(2)])), None);
    let tile = |w: &World, x: usize| w.locations["lake"].tile_id(x, 0).map(str::to_string);
    assert_eq!(tile(&w, 1).as_deref(), Some("rock"));
    assert_eq!(tile(&w, 2).as_deref(), Some("grass"), "the second blast waits");
    for _ in 0..179 {
        w.tick_second();
    }
    assert_eq!(tile(&w, 2).as_deref(), Some("grass"));
    // The party walked off meanwhile: it still goes off by the lake.
    let home = w.locations.keys().find(|k| *k != "lake").cloned().unwrap();
    w.travel(&home);
    w.tick_second();
    assert_eq!(tile(&w, 2).as_deref(), Some("rock"));
    assert!(w.later.is_empty());
}

/// Variables, ways by them, repeats and scripts that are named and run by name.
#[test]
fn scripts_count_choose_repeat_and_run_by_name() {
    let mut w = world_in(&lake(), (2, 2), serde_json::json!([]));
    w.apply(&steps(serde_json::json!([
        { "op": "define", "id": "boom", "effects": [{ "op": "add", "var": "blasts" }] },
        { "op": "repeat", "times": 3, "effects": [{ "op": "script", "id": "boom" }] },
        { "op": "if", "var": "blasts", "at_least": 3, "then": [{ "op": "set", "var": "door", "value": 1 }], "else": [{ "op": "set", "var": "door", "value": 0 }] }
    ])), None);
    assert_eq!(w.vars["blasts"], 3);
    assert_eq!(w.vars["door"], 1);
    // A script that runs itself stops, it does not hang the game.
    let notes = w.apply(&steps(serde_json::json!([{ "op": "define", "id": "loop", "effects": [{ "op": "script", "id": "loop" }] }, { "op": "script", "id": "loop" }])), None);
    assert!(notes.iter().any(|n| n.contains("too deep")));
}

/// A script set off by walking into a place, by an hour, by a variable.
#[test]
fn scripts_go_off_by_themselves() {
    let mut w = world_in(&lake(), (2, 2), serde_json::json!([]));
    w.apply(&steps(serde_json::json!([
        { "op": "define", "id": "welcome", "on": "enter", "place": "lake", "effects": [{ "op": "add", "var": "visits" }] },
        { "op": "define", "id": "alarm", "on": "var", "var": "noise", "at": 3, "effects": [{ "op": "set", "var": "guards", "value": 1 }] }
    ])), None);
    w.trigger("enter", "lake");
    assert_eq!(w.vars["visits"], 1);
    w.vars.insert("noise".into(), 3);
    w.tick_second();
    w.tick_second();
    assert_eq!(w.vars["guards"], 1);
    w.vars.insert("guards".into(), 0);
    w.tick_second();
    assert_eq!(w.vars["guards"], 0, "it goes off once till it is due again");
}

/// A guard says its lines by itself, in turn, every so many seconds.
#[test]
fn a_creature_says_its_lines_in_turn() {
    let mut w = world_in(&lake(), (2, 2), serde_json::json!([]));
    w.apply(&steps(serde_json::json!([{ "op": "spawn", "kind": "creature", "id": "guard", "name": "Guard", "x": 4, "y": 1, "attitude": "neutral" }, { "op": "lines", "id": "guard", "lines": ["Halt!", "Who goes there?"], "every": 5 }])), None);
    let mut said = Vec::new();
    for _ in 0..10 {
        w.tick_second();
        said.extend(w.bubbles.drain(..).map(|(_, text, _)| text));
    }
    assert_eq!(said, vec!["Halt!".to_string(), "Who goes there?".to_string()]);
    w.apply(&steps(serde_json::json!([{ "op": "say", "id": "guard", "text": "Move along." }])), None);
    assert_eq!(w.bubbles.last().map(|b| b.1.as_str()), Some("Move along."));
}

/// A boss three cells across is spawned that big.
#[test]
fn a_boss_takes_three_cells() {
    let mut w = world_in(&lake(), (2, 2), serde_json::json!([]));
    w.apply(&steps(serde_json::json!([{ "op": "spawn", "kind": "creature", "id": "ogre", "name": "Ogre", "x": 24, "y": 10, "monster": "ogre", "attitude": "hostile", "count": 1, "size": 3 }])), None);
    let ogre = w.locations["lake"].creatures.iter().find(|c| c.id == "ogre").unwrap();
    assert_eq!(ogre.size, 3);
}

/// The master makes a creature bigger and tougher, and gives a statue something to do.
#[test]
fn a_creature_is_changed_and_a_thing_given_an_action() {
    let mut w = world_in(&lake(), (2, 2), serde_json::json!([{ "id": "statue", "name": "Statue", "description": "Stone.", "x": 5, "y": 5, "blocking": true, "sprite": "rock" }]));
    w.apply(&steps(serde_json::json!([
        { "op": "spawn", "kind": "creature", "id": "wolf", "name": "Wolf", "x": 4, "y": 1, "monster": "wolf", "attitude": "neutral", "count": 1 },
        { "op": "edit", "id": "wolf", "name": "Dire wolf", "size": 2, "hp": 40, "ac": 15 },
        { "op": "action", "id": "statue", "label": "Touch the eyes", "outcome": "The statue groans.", "effects": [{ "op": "shake", "seconds": 1 }, { "op": "set", "var": "statue", "value": 1 }] }
    ])), None);
    let here = &w.locations["lake"];
    let wolf = here.creatures.iter().find(|c| c.id == "wolf").unwrap();
    assert_eq!((wolf.name.as_str(), wolf.size), ("Dire wolf", 2));
    assert_eq!(wolf.stats.as_ref().map(|s| (s.hp, s.ac)), Some((40, 15)));
    assert!(wolf.monster.is_none());
    let statue = here.props.iter().find(|p| p.id == "statue").unwrap();
    let touch = statue.actions.iter().find(|a| a.label == "Touch the eyes").unwrap();
    assert_eq!(touch.effects.len(), 2);
}

/// The master drew a sword and put it in the chest beforehand; the merchant sells a lamp.
#[test]
fn a_thing_drawn_by_the_master_waits_in_a_chest_or_on_a_merchants_shelf() {
    let mut w = world_in(&lake(), (2, 2), serde_json::json!([{ "id": "crate", "name": "Crate", "description": "Wood.", "x": 5, "y": 5, "blocking": true, "sprite": "rock" }]));
    let sword: Vec<String> = (0..48).map(|_| "0000000110000000".to_string()).collect();
    w.apply(&steps(serde_json::json!([
        { "op": "stash", "id": "crate", "name": "Moon blade", "kind": "weapon", "gear": "longsword", "size": [1, 3], "rows": sword, "bonus": 1 },
        { "op": "spawn", "kind": "creature", "id": "peddler", "name": "Peddler", "x": 4, "y": 1, "attitude": "friendly" },
        { "op": "stash", "id": "peddler", "name": "Lamp", "kind": "light", "light": 4, "price": 12 }
    ])), None);
    let here = &w.locations["lake"];
    let chest = here.props.iter().find(|p| p.id == "crate").and_then(|p| p.chest.as_ref()).unwrap();
    assert_eq!(chest.items[0].name, "Moon blade");
    assert_eq!(chest.items[0].size_cells, Some((1, 3)));
    assert_eq!(chest.items[0].rows.len(), 48);
    let peddler = here.creatures.iter().find(|c| c.id == "peddler").unwrap();
    assert!(peddler.stock.iter().any(|i| i.name == "Lamp" && i.price == 12));
}

/// A blast in the far corner is felt only by the hero near it; a wink is seen by one alone.
#[test]
fn a_sight_is_for_those_it_is_meant_for() {
    let mut w = world_in(&lake(), (2, 2), serde_json::json!([]));
    w.apply(&steps(serde_json::json!([{ "op": "shake", "seconds": 1, "x": 28, "y": 14, "radius": 3 }])), None);
    assert!(w.cues.iter().all(|c| !c.motion.starts_with("quake")), "nobody is near the blast: nothing is shown");
    w.apply(&steps(serde_json::json!([{ "op": "shake", "seconds": 1, "x": 3, "y": 2, "radius": 3 }])), None);
    assert_eq!(w.cues.last().map(|c| c.who.clone()), Some(vec!["Brak".to_string()]));
    w.apply(&steps(serde_json::json!([{ "op": "animate", "id": "party", "motion": "nod", "who": ["Brak"] }, { "op": "say", "id": "party", "text": "psst", "who": ["Brak"] }])), None);
    assert_eq!(w.cues.last().map(|c| c.who.clone()), Some(vec!["Brak".to_string()]));
    assert_eq!(w.bubbles.last().map(|b| b.2.clone()), Some(vec!["Brak".to_string()]));
    // With nobody named and no cell, everyone in the place sees it.
    w.apply(&steps(serde_json::json!([{ "op": "shake", "seconds": 1 }])), None);
    assert!(w.cues.last().is_some_and(|c| c.who.is_empty()));
}

/// A boss three cells across set down by a wall stands where all its cells are free.
#[test]
fn a_boss_is_set_down_where_all_its_cells_are_free() {
    let mut map = lake();
    map[6] = "....rrrr......................";
    let mut w = world_in(&map, (2, 2), serde_json::json!([]));
    w.apply(&steps(serde_json::json!([{ "op": "spawn", "kind": "creature", "id": "ogre", "name": "Ogre", "x": 4, "y": 4, "monster": "ogre", "attitude": "neutral", "count": 1, "size": 3 }])), None);
    let place = &w.locations["lake"];
    let ogre = place.creatures.iter().find(|c| c.id == "ogre").unwrap();
    let tiles = &w.tiles;
    for dx in 0..3 {
        for dy in 0..3 {
            let (x, y) = (ogre.x + dx, ogre.y + dy);
            let id = place.tile_id(x, y).unwrap_or("");
            assert!(tiles.get(id).is_some_and(|t| t.walkable), "({x}, {y}) under the ogre is {id}");
        }
    }
}

/// The master lets the party sleep, stops a fight with a parley, and leaves a thing on the ground.
#[test]
fn the_master_rests_the_party_makes_peace_and_drops_a_thing() {
    let mut w = world_in(&lake(), (2, 2), serde_json::json!([]));
    w.party[0].hp = 3;
    w.apply(&steps(serde_json::json!([{ "op": "rest", "kind": "long" }])), None);
    assert_eq!(w.party[0].hp, w.party[0].max_hp);
    w.apply(&steps(serde_json::json!([{ "op": "spawn", "kind": "creature", "id": "bandit", "name": "Bandit", "x": 4, "y": 2, "monster": "bandit", "attitude": "hostile", "count": 1 }, { "op": "fight", "with": ["bandit"] }])), None);
    assert!(w.fight_here());
    w.apply(&steps(serde_json::json!([{ "op": "peace" }])), None);
    assert!(!w.fight_here());
    assert!(w.locations["lake"].creatures.iter().all(|c| c.attitude != crate::Attitude::Hostile));
    w.apply(&steps(serde_json::json!([{ "op": "drop", "x": 6, "y": 2, "name": "Silver ring", "kind": "junk", "price": 40 }])), None);
    assert!(w.locations["lake"].items.iter().any(|l| l.item.name == "Silver ring" && (l.x, l.y) == (6, 2)));
}
