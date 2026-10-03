//! The words sent to the game master: the rules of the drawing format with the requests
//! built on them (a new world, one more place), and the rules of telling the story with
//! theirs (what the party does, an unprompted event, writing up a chapter).

use crate::base::SLOTS;
use crate::model::{Attitude, MAP_H, MAP_W, World, char_of};
use crate::story::{KEPT_LINES, Speaker};
use std::fmt::Write;

/// How the master says what a creature fights with.
const FIGHTERS: &str = r##"A creature that might fight needs numbers. Give "monster" the index of the D&D 5e SRD monster it is or most resembles (goblin, wolf, bandit, skeleton, giant-spider, ogre, guard, …), or give "stats" of its own: {"ac": 13, "hp": 11, "speed": 30, "attacks": [{"name": "…", "bonus": 4, "damage": "1d6+2", "range": 5}], "xp": 50}. Match the danger to a first-level party unless told otherwise."##;

/// How the master changes the world. Shared by drawing (prepared actions) and by play.
const EFFECTS: &str = r##"EFFECTS change the world. Each is an object with "op":
  {"op": "remove", "id": "…"} — a prop or creature leaves the scene
  {"op": "give", "name": "…", "description": "…", "kind": "weapon|armor|light|potion|food|throwable|key|tool|junk|quest", "size": [1, 2], "heal": "2d4+2", "damage": "1d6", "light": 4, "rows": ["…"]} — the party gains an item; all but name are optional: "size" is cells across and down in the bag (1 to 4), "heal" what drinking or eating it heals, "damage" what throwing it does, "light" how many cells it lights when held, "rows" your own picture of it, 16 pixels to a cell
  {"op": "take", "name": "…"} — the party loses an item (used up, given away, broken)
  {"op": "describe", "id": "…", "description": "…"} — a thing looks different from now on
  {"op": "light", "light": "day|dusk|night|dark"} — time passes here: the hour and the light change
  {"op": "weather", "weather": "clear|sun|rain|storm|snow|fog"} — the sky over this place changes
  {"op": "mark", "x": 0, "y": 0, "kind": "blood|dung|puddle|scorch|ash|slime|vomit|wine|paint|rubble|coins|bones", "color": "#rrggbb"} — something is left on the ground of a cell; without x and y it is where the party stands; "id" instead puts it under that thing; "color" is optional
  {"op": "clean", "x": 0, "y": 0} — what was left on a cell is picked up or washed away (with "give" when the party takes it)
  {"op": "travel", "to": "place_id", "name": "for a new place"} — the party goes to another place
  {"op": "animate", "id": "hero name|creature id|party", "motion": "jump|crouch|sit|lie|fall|shake|spin|strike|dance|wave|nod|bow|cower|stagger"} — someone moves their body; everyone at the table sees it
  {"op": "attitude", "id": "…", "attitude": "friendly|neutral|hostile"}
  {"op": "move", "id": "…", "x": 0, "y": 0}
  {"op": "tile", "x": 0, "y": 0, "tile": "tile_id"} — one cell of ground changes (a door opens, a bridge falls)
  {"op": "spawn", "kind": "prop|creature", "id": "…", "name": "…", "description": "…", "x": 0, "y": 0, "sprite": "…", "blocking": false, "attitude": "neutral"} — a creature that might fight also takes "monster" or "stats"; a prop that holds things (a chest, a crate, a purse) takes "contents": [things as in "give"], and "lock": 15 with "key": "key name" when it is locked — the game handles opening, picking and forcing it
  {"op": "secret", "x": 0, "y": 0, "dc": 14, "kind": "hidden|trap", "note": "a loose stone in the wall", "effects": []} — something hidden: when a hero with a sharp enough eye comes near it is found and its effects happen (a hidden door: a "tile" to a door; a cache: "give"); a trap goes off with its effects ("damage") when stepped on unnoticed
  {"op": "exit", "x": 0, "y": 0, "to": "place_id", "label": "…"} — a way out appears
  {"op": "fact", "text": "…"} — something that must be remembered for the rest of the campaign
  {"op": "check", "ability": "str|dex|con|int|wis|cha", "skill": "stealth", "dc": 15, "success": {"narration": "…", "effects": []}, "failure": {"narration": "…", "effects": []}} — the outcome is uncertain: the game rolls the die and takes one branch. Write both. Skills: acrobatics, animal-handling, arcana, athletics, deception, history, insight, intimidation, investigation, medicine, nature, perception, performance, persuasion, religion, sleight-of-hand, stealth, survival. Difficulty: 5 trivial, 10 easy, 15 moderate, 20 hard, 25 very hard.
  {"op": "damage", "amount": "2d6"} / {"op": "heal", "amount": "1d8"} — to the party, outside a fight
  {"op": "xp", "amount": 50} — for something achieved; rarely, and modestly
  {"op": "fight"} — the hostile creatures here attack; the game runs the fight by the rules
  {"op": "deed", "hero": "name"} — that character just did something out of the ordinary, clever or brave beyond what the moment asked; the game keeps count
  {"op": "revive", "hero": "name or a body's id"} — the story brings a dead character back; only when it has truly earned it
Never roll dice yourself and never decide an uncertain outcome: that is what "check" is for. Nothing may follow a check in the same list except what should happen whichever way it goes.
Use only ids that exist. Inside an action that belongs to a thing, the id "self" means that thing.
SPACE. The party is somewhere on the map and can only touch, take, open, strike or speak softly with what is beside it. To deal with something farther off that can be reached, first "move" the party to a cell beside it, in the same answer, then do the rest. What cannot be reached on foot cannot be touched: say what is in the way. Only what the party carries can be used — no torch, rope or key that is not in their hands.
VIOLENCE. An attack on a creature is never told as a hit, a wound or a death: make it hostile with "attitude" and add "fight" — the game moves to the fight and the dice decide. The same for anything that would hurt the party.
TRAVEL. When the party goes to another place — through a way out, down a hole, carried off — add {"op": "travel", "to": "place_id"}: an id from the exits, or a new id with "name" for a place that does not exist yet, which the game will ask you to draw. A choice that leads elsewhere carries its own "travel".
THE PICTURE MUST MATCH THE STORY. The players watch a map, not only words. Everything your narration says that one could see must come with the effect that shows it, in the same answer: someone crouches, jumps, strikes, falls or dances — "animate"; a creature comes closer, backs off, circles — "move" it to its new cell (several cells if it runs); it leaves — "remove"; something appears — "spawn"; blood, dung, ash, a spill or a scorch is left — "mark"; a door opens or a wall falls — "tile"; creatures attack — "attitude" hostile and "fight". Never narrate a change on the map without its effect."##;

/// The rules that never change. The answer format is plain JSON so that every model in the
/// fallback chain can produce it.
pub fn system_prompt(language: &str) -> String {
    let mut slots = String::new();
    for (i, (job, _)) in SLOTS.iter().enumerate() {
        let _ = writeln!(slots, "  {} — {job}", char_of(i as u8 + 1));
    }
    format!(
        r##"You are the game master of a top-down pixel-art tabletop role-playing game for adults. You invent the world and you draw it. Dark themes are welcome.

You answer with ONE JSON object and nothing else: no prose, no code fence.

LANGUAGE. Every name, label and description is written in {language}. Every "id" is short English snake_case.

PICTURES. A picture is a list of rows; a row is a string; each character is one pixel holding a palette index: 0 is transparent, then 1-9, then a-v (a=10 … v=31).
The first fifteen colours have fixed jobs, so that everything in a world matches:
{slots}You choose the actual colours for these jobs (purple plants, red sand — whatever the world needs) and may add colours g-v for anything else.
- A tile is 16 rows of 16 characters and covers its whole cell: no 0.
- A sprite is 16x16 (one cell) or 32x32 (a large creature or object). Draw it like a classic 16-bit RPG: a one-pixel outline in colour 1, background 0, the feet on the bottom row, light from the top left. Make it recognisable at a glance.

MAPS. A map is {h} rows of exactly {w} characters. "legend" says which tile each character stands for. x counts columns 0-{mx} from the left, y counts rows 0-{my} from the top.
Built-in tiles you can use without drawing them:
  walkable: grass, dirt, sand, stone_floor, wood_floor, door
  solid: water, stone_wall, wood_wall, tree, bush, rock
Draw "new_tiles" when a place needs something these do not cover. A place should look like somewhere particular: buildings with walls and doors, paths, water, clearings — not an empty field.

EXITS. An exit is a walkable cell, usually on the edge of the map or in a doorway, that leads to another place named by "to". Every place an exit leads to is listed in "neighbors" unless it already exists. Give a place two to four exits, and at least one of them must lead somewhere new — a neighbor that is not yet in the world — so that the world keeps opening up rather than closing into a loop. Everything must be reachable on foot from "spawn".

PROPS are things worth a look (a well, a wreck, a shrine); CREATURES are beings (people, animals, monsters). Each names a sprite by id; draw the sprite in "sprites" unless one with that id already exists. "attitude" is friendly, neutral or hostile.
{FIGHTERS}

ACTIONS. Give every prop and creature two or three "actions": things the players are likely to do with it. The players pick one from a list and read its outcome at once, without asking you, so write the outcome now: one to three sentences, told to the players as their game master ("you"), with the effects it has. Make them matter: a clue, an item, a changed attitude, a new way out. A creature that can talk has an action in which it says something worth hearing.

{EFFECTS}"##,
        w = MAP_W,
        h = MAP_H,
        mx = MAP_W - 1,
        my = MAP_H - 1,
    )
}

/// The shape of a location in an answer, shown by example.
const LOCATION_SHAPE: &str = r##"{
  "id": "snake_case", "name": "…", "description": "two or three sentences the players read on arrival",
  "light": "day, dusk, night or dark (a cave, a cellar): the hour and the light of the place",
  "weather": "clear, sun, rain, storm, snow or fog; clear indoors and underground",
  "legend": {".": "grass", "#": "stone_wall", "+": "door"},
  "map": ["30 characters", "… 16 rows in all"],
  "spawn": [x, y],
  "new_tiles": [{"id": "…", "name": "…", "walkable": true, "rows": ["16 characters", "… 16 rows"]}],
  "sprites": [{"id": "…", "rows": ["16 or 32 characters", "… as many rows"]}],
  "props": [{"id": "…", "name": "…", "description": "…", "x": 0, "y": 0, "blocking": true, "sprite": "…", "actions": [{"label": "what the players choose", "outcome": "what happens", "effects": []}]}],
  "creatures": [{"id": "…", "name": "…", "description": "…", "x": 0, "y": 0, "sprite": "…", "attitude": "neutral", "monster": "srd-index or omit", "actions": [{"label": "…", "outcome": "…", "effects": []}]}],
  "exits": [{"x": 0, "y": 0, "to": "neighbor_id", "label": "what the way out looks like"}],
  "neighbors": [{"id": "neighbor_id", "name": "…", "description": "one sentence: what is there"}]
}"##;

/// The first request of a game. `request` is what the host typed; it may be empty.
pub fn new_world(request: &str) -> String {
    let wish = if request.trim().is_empty() {
        "The host left the choice to you. Pick a setting nobody would expect and make it vivid.".to_string()
    } else {
        format!("The host asked for this world: «{}»", request.trim())
    };
    format!(
        r##"Create a new world and draw the place where the adventure begins.

{wish}

Answer with this JSON object:
{{
  "title": "the name of the world or campaign",
  "summary": "three or four sentences: the setting, what is going on, what makes it different",
  "tone": "a few words",
  "palette": {{"1": "#rrggbb", "…": "a colour for each of 1-f, plus any of g-v you add"}},
  "tiles": [tiles of this world that every place may use, same shape as new_tiles],
  "location": {LOCATION_SHAPE}
}}"##
    )
}

/// The rules of telling the story, sent with every request made during play.
pub fn play_prompt(world: &World) -> String {
    let violence = if world.violence { "Violence may be described as graphically as the scene calls for." } else { "Keep violence off the page: say that it happened, not how it looked." };
    let sex = if world.sex { "Sexual content between adults may be explicit when the players lead there." } else { "Keep sex off the page: fade to black." };
    format!(
        r##"You are the game master of a tabletop role-playing game for adults, played on a top-down pixel map. You tell the players what happens and you change the world to match.

You answer with ONE JSON object and nothing else: no prose, no code fence.

LANGUAGE. Everything the players read is written in {language}. Every "id" is short English snake_case.

THE TABLE. All players and all characters are adults. {violence} {sex}

TELLING. Speak to the players as "you". Two to five sentences: concrete, sensory, no summary of what they already know. Never decide what the players' characters do, say or feel. The world reacts: people remember, things break, time passes, actions have consequences. What the party attempts can fail or cost something. Do not hand out items, allies or secrets for nothing. Keep to what is established in the facts, the chapters and the story so far.

{EFFECTS}

A picture, when you need a new sprite, is a list of rows of palette index characters (0 transparent, 1-9, a-v), 16x16 or 32x32, outlined in colour 1.{book}"##,
        language = world.language,
        book = book_note(world),
    )
}

fn attitude_word(attitude: Attitude) -> &'static str {
    match attitude {
        Attitude::Friendly => "friendly",
        Attitude::Neutral => "neutral",
        Attitude::Hostile => "hostile",
    }
}

/// Everything the master has to know to tell the next part: the world, what must be
/// remembered, the place and what is in it, and the story so far. The newest
/// `recent` lines of the story are given word for word.
fn situation(world: &World, recent: std::ops::RangeFrom<usize>) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "WORLD: {}. {}\nTone: {}.", world.title, world.summary, world.tone);
    if !world.story.facts.is_empty() {
        let _ = writeln!(text, "\nFACTS:");
        for fact in &world.story.facts {
            let _ = writeln!(text, "- {fact}");
        }
    }
    for (n, chapter) in world.story.chapters.iter().enumerate() {
        let _ = writeln!(text, "\nCHAPTER {}: {chapter}", n + 1);
    }
    let here = world.here();
    let _ = writeln!(text, "\nPLACE \"{}\": {}. {}", world.current, here.name, here.description);
    for prop in &here.props {
        let held = prop.chest.as_ref().map(|c| if c.open { " (opened)".to_string() } else if c.lock > 0 { format!(" (locked, lock {}{})", c.lock, if c.key.is_empty() { String::new() } else { format!(", key: {}", c.key) }) } else { " (shut)".to_string() }).unwrap_or_default();
        let _ = writeln!(text, "- prop {} at ({}, {}){held}: {} — {}", prop.id, prop.x, prop.y, prop.name, prop.description);
    }
    for creature in &here.creatures {
        let _ = writeln!(text, "- creature {} at ({}, {}), {}: {} — {}", creature.id, creature.x, creature.y, attitude_word(creature.attitude), creature.name, creature.description);
    }
    for exit in &here.exits {
        let _ = writeln!(text, "- exit at ({}, {}) to {}: {}", exit.x, exit.y, exit.to, world.place_name(&exit.to));
    }
    let (x, y) = world.player.cell();
    let _ = writeln!(text, "The party stands at ({x}, {y}).");
    // The map itself, so that walls between the party and anything are known.
    let _ = writeln!(text, "MAP of this place (x across, y down; the party is @):");
    for (row, line) in here.map.iter().enumerate() {
        let shown: String = line.chars().enumerate().map(|(col, c)| if (col as i32, row as i32) == (x, y) { '@' } else { c }).collect();
        let _ = writeln!(text, "  {shown}");
    }
    let legend: Vec<String> = here.legend.iter().map(|(c, id)| format!("{c} {id}{}", if world.tiles.get(id).is_some_and(|t| t.walkable) { "" } else { " (solid)" })).collect();
    let _ = writeln!(text, "LEGEND: {}", legend.join(", "));
    // How far everything is on foot, around walls.
    let start = (x.clamp(0, MAP_W as i32 - 1) as usize, y.clamp(0, MAP_H as i32 - 1) as usize);
    let far = crate::path::steps(start, |(cx, cy)| here.blocked(cx as i32, cy as i32, &world.tiles));
    let walk = |(tx, ty): (usize, usize)| {
        let beside = [(0i32, -1i32), (1, 0), (0, 1), (-1, 0)].iter().filter_map(|(dx, dy)| {
            let (nx, ny) = (tx as i32 + dx, ty as i32 + dy);
            ((0..MAP_W as i32).contains(&nx) && (0..MAP_H as i32).contains(&ny)).then(|| far[ny as usize][nx as usize]).flatten()
        });
        match beside.min() {
            Some(0) => "beside the party".to_string(),
            Some(n) => format!("{n} steps away on foot"),
            None => "cannot be reached on foot from here".to_string(),
        }
    };
    for lying in &here.items {
        let _ = writeln!(text, "- {} lies on the ground at ({}, {})", lying.item.name, lying.x, lying.y);
    }
    let _ = writeln!(text, "WHAT IS WHERE FROM THE PARTY:");
    for prop in &here.props {
        let _ = writeln!(text, "- {} ({}): {}", prop.id, prop.name, walk((prop.x, prop.y)));
    }
    for creature in &here.creatures {
        let _ = writeln!(text, "- {} ({}): {}", creature.id, creature.name, walk((creature.x, creature.y)));
    }
    let _ = write!(text, "\nSPRITES THAT EXIST:");
    for sprite_id in world.sprites.keys() {
        let _ = write!(text, " {sprite_id}");
    }
    let _ = write!(text, "\nTILES THAT EXIST:");
    for tile_id in world.tiles.keys() {
        let _ = write!(text, " {tile_id}");
    }
    if !world.party.is_empty() {
        let _ = writeln!(text, "\n\nTHE PARTY:");
        // Those who are here: a party that has split up is told about scene by scene.
        for hero in world.party.iter().enumerate().filter(|(i, _)| !world.is_away(*i) && world.hero_place(*i) == world.current).map(|(_, h)| h) {
            let state = if hero.dead { " (dead)".to_string() } else { format!(", {} of {} hit points", hero.hp, hero.max_hp) };
            let index = world.party.iter().position(|h| std::ptr::eq(h, hero)).unwrap_or(0);
            let light = world.held_light(index).map(|l| format!("{} (lit, in hand)", l.name)).unwrap_or_default();
            let bag: Vec<String> = world.story.bags.get(index).map(|b| b.inventory.iter().map(|i| if i.count > 1 { format!("{} x{}", i.name, i.count) } else { i.name.clone() }).collect()).unwrap_or_default();
            let bag = if bag.is_empty() { "nothing".to_string() } else { bag.join(", ") };
            let gear = [hero.weapons.join(", "), hero.armor.clone().unwrap_or_default(), if hero.shield { "shield".to_string() } else { String::new() }, light].into_iter().filter(|g| !g.is_empty()).collect::<Vec<_>>().join("; ");
            let _ = writeln!(text, "- {}, {} {} of level {}{state}. Trained in: {}. Carries: {gear}. In the bag: {bag}. {}", hero.name, hero.race, hero.class, hero.level, hero.skills.join(", "), hero.description);
        }
    }
    // With no heroes made yet, what was found so far waits in the first bag.
    if world.party.is_empty()
        && let Some(bag) = world.story.bags.first().filter(|b| !b.inventory.is_empty())
    {
        let things: Vec<String> = bag.inventory.iter().map(|i| i.name.clone()).collect();
        let _ = write!(text, "
THE PARTY CARRIES: {}", things.join(", "));
    }
    let lines = world.story.log.get(recent).unwrap_or(&[]);
    if !lines.is_empty() {
        let _ = writeln!(text, "\n\nTHE STORY SO FAR:");
        for line in lines {
            let who = if line.speaker == Speaker::Master { "GM" } else { "PARTY" };
            let _ = writeln!(text, "{who}: {}", line.text);
        }
    }
    text
}

const TURN_SHAPE: &str = r##"{
  "narration": "what happens, told to the players",
  "effects": [],
  "sprites": [{"id": "…", "rows": ["…"]}],
  "options": [{"label": "something the party might do next", "outcome": "what happens if they do, written now", "effects": [], "near": ""}]
}
An option that needs the party beside someone or something (talk to them, take it, open it) gives that thing's id as "near": the game lets the party choose it only once they have walked up to it, and keeps it on the thing to be found there. Every other option must be something the party can do from where it stands now."##;

/// With several players at the table one of them can be told something alone.
const WHISPERS: &str = r##"Several players sit at this table, each playing one character. Tell what a character does by that character's name, in the third person — "Bora reaches for it, slips and falls in the muck" — so that every player knows who did what; keep "you" for the whole party together. When one character alone would notice, know or be told something, add "whispers": [{"to": "that character's name", "text": "…"}] to the object: only that player reads it. Use it rarely; most turns need none."##;

fn whispers_note(world: &World) -> &'static str {
    let players = (0..world.party.len()).filter(|i| world.takes_part(*i)).count();
    if players > 1 { WHISPERS } else { "" }
}

/// A human game master sits at the table and tells the model what should happen; the model
/// writes it up as a turn, which the master reads before the players do.
pub fn directive(world: &World, said: &str, text: &str) -> String {
    let did = if said.trim().is_empty() { String::new() } else { format!("The party now does this: «{}»
", said.trim()) };
    format!(
        "{}
{did}A human game master runs this table and directs you: «{}»

Carry the direction out: tell what happens in your own words, with the effects it needs, and offer two to four \"options\" for what the party might do next, each with its outcome already written. Do not add events the direction does not call for. {}

Answer with this JSON object:
{TURN_SHAPE}",
        situation(world, 0..),
        text.trim(),
        whispers_note(world)
    )
}

/// What a table that plays by the book adds to every request.
fn book_note(world: &World) -> &'static str {
    if world.table.book_only {
        "\n\nBY THE BOOK. This table plays strictly by the D&D 5e SRD. Every creature that might fight must be an SRD monster: give \"monster\", never \"stats\". Do not invent abilities, spells or magic items the SRD does not have."
    } else {
        ""
    }
}

/// The master is asked for abilities of a hero's own: three to choose from at a level, or
/// one secret skill after an unusual deed.
pub fn feats(world: &World, hero: usize, secret: bool) -> String {
    let who = world.party.get(hero).map(|h| format!("{}, {} {} of level {}. {}", h.name, h.race, h.class, h.level, h.description)).unwrap_or_default();
    let ask = if secret {
        "This character has just done something out of the ordinary and earned a secret skill for it. Write ONE, born of that deed and of what the character has been through.".to_string()
    } else {
        "This character has reached a new level. Offer THREE abilities to choose from, each different in kind, each grown out of what this character has done and been through in the story — not generic ones.".to_string()
    };
    format!(
        "{}\nTHE CHARACTER: {who}\n\n{ask}\n\nAn ability is put together only from what the game can carry out:\n- harm or healing: \"dice\" (like 2d6), \"heals\" true for healing, \"range\" in feet, \"area\" in feet around the target (0 for one target), \"save\" (str, dex, con, int, wis, cha) when the targets resist instead of the character rolling to hit, \"uses\" between long rests (0 for at will);\n- or plain numbers that always count: \"ac\" (at most 1), \"hp\", \"speed\" in feet.\nKeep it modest for the level; the game cuts down what is too strong. Give each a name and one or two sentences of description that say what it looks like in play.\n\nAnswer with this JSON object:\n{{\"feats\": [{{\"name\": \"…\", \"description\": \"…\", \"dice\": \"2d6\", \"heals\": false, \"range\": 30, \"area\": 0, \"save\": null, \"uses\": 2, \"ac\": 0, \"hp\": 0, \"speed\": 0}}]}}",
        situation(world, 0..)
    )
}

/// A hero tries something in a fight that the rules have no button for.
pub fn stunt(world: &World, said: &str) -> String {
    let mut text = situation(world, 0..);
    if let Some(fight) = &world.fight {
        let _ = writeln!(text, "\nA FIGHT IS ON. It is {}'s turn. Those fighting:", fight.actor().name);
        for c in fight.combatants.iter().filter(|c| !c.dead) {
            let side = if c.side == rules::Side::Party { "party" } else { "enemy" };
            let _ = writeln!(text, "- {} \"{}\" ({side}) at ({}, {}), {} of {} hit points", c.id, c.name, c.x, c.y, c.hp.max(0), c.max_hp);
        }
    }
    format!(
        "{text}\nOn their turn the acting character tries this: «{}»\n\nThe rules have no button for it, so you judge it. Name the check it takes and how hard it is, and say what a success is worth: harm to one of those fighting (\"target\" is its id), or healing, in dice suited to the character's level — or nothing but an advantage in the story. Never roll yourself. Write what it looks like, then one sentence for success and one for failure. Whether anyone falls is decided by the dice and the hit points, after your words are read: never write that a creature dies, is destroyed or falls, nor that the character is struck down — describe the blow landing or missing, and the game says who drops.\n\nAnswer with this JSON object:\n{{\"narration\": \"the attempt\", \"ability\": \"str|dex|con|int|wis|cha\", \"skill\": \"athletics or null\", \"dc\": 13, \"target\": \"id or empty\", \"damage\": \"1d8 or empty\", \"heal\": \"\", \"success\": \"…\", \"failure\": \"…\"}}",
        said.trim()
    )
}

/// The master draws a hero as the player described it.
pub fn portrait(world: &World, hero: usize) -> String {
    let who = world.party.get(hero).map(|h| format!("{}, a {} {}. {}", h.name, h.race, h.class, h.description)).unwrap_or_default();
    format!(
        "Draw the player character {who}\nWorld: {}. {}\n\nOne sprite, 16 by 16, drawn the way you draw every creature of this world: rows of palette characters, '0' transparent, a one-pixel outline in colour 1, feet on the bottom row, facing the viewer. It must read as this very character at a glance: the build, the gear of the class, one telling detail from the description.\n\nAnswer with this JSON object:\n{{\"rows\": [\"16 characters\", \"… 16 rows\"]}}",
        world.title, world.summary
    )
}

/// A player who reads another language than the world is told in asks for a translation.
pub fn translate(texts: &[String], language: &str) -> String {
    let list = serde_json::to_string(texts).unwrap_or_default();
    format!(
        "Translate these pieces of a tabletop role-playing game into {language}: tellings by the game master, names of places, things and creatures, things the players may choose to do. Keep the tone and the person (\"you\" stays \"you\"), keep names recognisable, keep numbers, dice (2d6) and line breaks as they are. Translate every piece; a piece already in {language} comes back unchanged.\n\n{list}\n\nAnswer with this JSON object, the translations in the same order and of the same number:\n{{\"texts\": [\"…\"]}}"
    )
}

/// A human game master asks for a creature to keep in their library.
pub fn invent(world: &World, request: &str) -> String {
    format!(
        "A human game master asks you for a creature to keep and use in their games: «{}»
The world it is first meant for: {}. Party level: {}.

Draw it as a sprite the same way you draw everything else (rows of palette characters, at most 16 by 16, '0' transparent). {FIGHTERS}

Answer with this JSON object:
{{\"id\": \"snake_case_id\", \"name\": \"…\", \"description\": \"one or two sentences\", \"rows\": [\"…\"], \"monster\": null, \"stats\": {{\"ac\": 13, \"hp\": 11, \"speed\": 30, \"attacks\": [{{\"name\": \"…\", \"bonus\": 4, \"damage\": \"1d6+2\", \"range\": 5}}], \"xp\": 50}}}}",
        request.trim(),
        world.title,
        world.party_level()
    )
}

/// The party does something the master has not prepared an outcome for. `near` is the thing
/// they stand next to, if any.
/// What a choice that was picked is sent to the master as: the outcome is in the story
/// already, so the master goes on from there.
pub const GO_ON: &str = "[this choice was taken and its outcome is told above; go on from there: what follows, and new options]";

pub fn turn(world: &World, said: &str, near: Option<&str>) -> String {
    let beside = near.map(|id| format!(" They are standing next to \"{id}\".")).unwrap_or_default();
    format!(
        "{}\nThe party now does this: «{}»{beside}\n\nTell what happens. Offer two to four \"options\" for what the party might do next, each with its outcome already written, so that picking one needs no further question to you. Leave \"sprites\" empty unless an effect needs a sprite that does not exist. {}\n\nAnswer with this JSON object:\n{TURN_SHAPE}",
        situation(world, 0..),
        said.trim(),
        whispers_note(world)
    )
}

/// The players have been idle. The master may make something happen, or not.
pub fn event(world: &World) -> String {
    format!(
        "{}\nThe party has done nothing for a while. If this place and this story call for it, make something happen now of its own accord: someone arrives, something breaks, a sound, a change in the weather that matters. If nothing should happen, answer with an empty \"narration\".\n\nAnswer with this JSON object:\n{TURN_SHAPE}",
        situation(world, 0..)
    )
}

/// The story has grown long: the master writes the older part up as a chapter.
pub fn chapter(world: &World) -> String {
    let mut older = world.clone();
    let keep_from = older.story.log.len().saturating_sub(KEPT_LINES);
    older.story.log.truncate(keep_from);
    format!(
        "{}\nThe story above has grown long. Write it up as one chapter so that nothing which matters later is lost: who was met and what they want, what was learned, what was gained and lost, what was promised, what was left unfinished, in the order it happened. Be detailed; this chapter replaces the story above. Then list as \"facts\" anything that must hold true for the rest of the campaign and is not yet among the facts.\n\nAnswer with this JSON object:\n{{\"chapter\": \"…\", \"facts\": [\"…\"]}}",
        situation(&older, 0..)
    )
}

/// A request to draw a place that so far is only a name. `from` is where the party walks in
/// from.
pub fn new_location(world: &World, id: &str, from: &str) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "WORLD: {}. {}\nTone: {}.", world.title, world.summary, world.tone);
    let _ = write!(text, "\nPALETTE:");
    for (i, [r, g, b]) in world.palette.colors.iter().enumerate() {
        let _ = write!(text, " {}=#{r:02x}{g:02x}{b:02x}", char_of(i as u8 + 1));
    }
    let _ = write!(text, "\n\nTILES THAT EXIST (walkable marked +):");
    for (tile_id, tile) in &world.tiles {
        let _ = write!(text, " {tile_id}{}", if tile.walkable { "+" } else { "" });
    }
    let _ = write!(text, "\nSPRITES THAT EXIST:");
    for sprite_id in world.sprites.keys() {
        let _ = write!(text, " {sprite_id}");
    }
    let _ = writeln!(text, "\n\nPLACES SO FAR:");
    for (place_id, place) in &world.locations {
        let _ = writeln!(text, "- {place_id}: {} — {}", place.name, place.description);
    }
    for (place_id, stub) in &world.stubs {
        let _ = writeln!(text, "- {place_id} (not drawn yet): {} — {}", stub.name, stub.description);
    }
    let (name, description) = world.stubs.get(id).map(|s| (s.name.as_str(), s.description.as_str())).unwrap_or((id, ""));
    let _ = write!(
        text,
        "\nDraw the place \"{id}\": {name}. {description}\nThe party walks in from \"{from}\" ({}): one exit must lead back to \"{from}\", on the side that makes sense. Other exits may lead to places that exist or to new ones.\n\nAnswer with this JSON object (\"id\" must be \"{id}\"):\n{LOCATION_SHAPE}",
        world.place_name(from)
    );
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{WorldDraft, build_world};
    use crate::fake;

    #[test]
    fn system_prompt_states_the_language_the_sizes_and_every_colour_job() {
        let p = system_prompt("Russian");
        assert!(p.contains("written in Russian"));
        assert!(p.contains("16 rows of exactly 30 characters"));
        assert!(p.contains("x counts columns 0-29") && p.contains("y counts rows 0-15"));
        for (job, _) in SLOTS {
            assert!(p.contains(job), "{job}");
        }
    }

    fn test_world() -> World {
        let draft: WorldDraft = serde_json::from_str(&fake::world("test")).unwrap();
        build_world(&draft, "test", "Russian").unwrap().0
    }

    #[test]
    fn drawing_rules_ask_for_prepared_actions_and_explain_effects() {
        let p = system_prompt("English");
        assert!(p.contains("ACTIONS.") && p.contains("\"op\": \"give\"") && p.contains("\"self\""));
        assert!(p.contains("\"op\": \"check\"") && p.contains("\"op\": \"fight\"") && p.contains("\"monster\""));
    }

    #[test]
    fn telling_rules_follow_the_tables_language_and_content_switches() {
        let mut w = test_world();
        let open = play_prompt(&w);
        assert!(open.contains("written in Russian") && open.contains("graphically") && open.contains("explicit"));
        w.violence = false;
        w.sex = false;
        let closed = play_prompt(&w);
        assert!(closed.contains("off the page") && closed.contains("fade to black") && !closed.contains("explicit"));
    }

    #[test]
    fn a_turn_request_shows_the_place_the_things_the_bag_and_the_story() {
        let mut w = test_world();
        w.perform("cat", 1);
        w.story.remember("The well is dry.");
        w.story.chapters.push("They arrived at dusk.".into());
        let text = turn(&w, "  I look under the roots ", Some("fire"));
        assert!(text.contains("- prop fire at (") && text.contains("Campfire"));
        assert!(!text.contains("creature cat"), "the cat left");
        assert!(text.contains("- The well is dry.") && text.contains("CHAPTER 1: They arrived at dusk."));
        assert!(text.contains("Cat hair"));
        assert!(text.contains("PARTY: Shoo it away") && text.contains("GM: The cat slips away"));
        assert!(text.contains("«I look under the roots»") && text.contains("next to \"fire\""));
        assert!(text.contains("- exit at (") && text.contains("The East Road"));
    }

    #[test]
    fn a_chapter_request_leaves_out_the_lines_that_stay_word_for_word() {
        let mut w = test_world();
        for n in 0..10 {
            w.story.say(Speaker::Master, &format!("line number {n}"));
        }
        let text = chapter(&w);
        assert!(text.contains("line number 3") && !text.contains("line number 4"));
        assert!(text.contains("\"chapter\""));
    }

    #[test]
    fn world_request_carries_the_hosts_wish_or_says_the_choice_is_free() {
        assert!(new_world("динозавры и лазеры").contains("«динозавры и лазеры»"));
        assert!(new_world("  ").contains("left the choice to you"));
    }

    #[test]
    fn location_request_tells_the_master_what_exists_and_where_the_party_comes_from() {
        let draft: WorldDraft = serde_json::from_str(&fake::world("test")).unwrap();
        let (world, _) = build_world(&draft, "test", "English").unwrap();
        let stub = world.stubs.keys().next().unwrap().clone();
        let text = new_location(&world, &stub, &world.current);
        assert!(text.contains(&format!("Draw the place \"{stub}\"")));
        assert!(text.contains(&format!("lead back to \"{}\"", world.current)));
        assert!(text.contains("grass+") && text.contains(" water"));
        assert!(text.contains(" hero") && text.contains("1=#"));
        assert!(text.contains(&world.title));
    }
}
