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
fn logs_cut_by_one_player_are_used_from_that_players_bag_by_another() {
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
    // The master is told who carries them: Bob, here. Carl, far off, is not listed.
    let asked = crate::prompt::turn(&w, "Ann's hero: we lay Bob's logs across", None);
    let carries: Vec<&str> = asked.lines().filter(|l| l.contains("Logs")).collect();
    assert!(carries.iter().any(|l| l.contains("Bob's hero")) && !asked.contains("Carl's hero"), "{carries:?}");

    // Ann lays the bridge with the logs Bob carries: they leave Bob's bag, not Carl's far off.
    w.acting = Some(0);
    w.actors = vec![0];
    let (_, notes) = answer(&mut w, "Ann's hero: we lay Bob's logs across", serde_json::json!({
        "narration": "Together you lay the logs.",
        "tiles": [{ "id": "log_bridge", "name": "Log bridge", "walkable": true, "rows": sixteen('3') }],
        "effects": [{ "op": "tile", "x": 8, "y": 8, "tile": "log_bridge", "to": [13, 8] }, { "op": "take", "name": "Logs" }]
    }));
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!((logs(&w, 0), logs(&w, 1), logs(&w, 2)), (0, 0, 1), "Bob's logs are used; Carl's stay with Carl");
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
