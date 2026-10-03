//! Reads what the game master drew and makes it playable.
//!
//! A model's answer is rarely exact: rows come out a character short, a tile is named that
//! was never drawn, an exit sits behind a wall. Every request costs scarce quota, so instead
//! of asking again the answer is repaired here, and each repair is noted. Only an answer
//! with no map at all is refused.

use crate::base::{FALLBACK_TILE, MARKER_SPRITE, base_sprites, base_tiles, default_palette};
use crate::model::{Attitude, Creature, Exit, Location, MAP_H, MAP_W, PALETTE_MAX, Player, Prop, SAVE_VERSION, Sprite, Stub, TILE, TileDef, World, index_of, TableRules};
use crate::party::Stats;
use crate::path::{Cell, elbow, nearest, reachable};
use crate::story::{ActionDraft, Story, read_actions};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const MAX_PROPS: usize = 40;
const MAX_CREATURES: usize = 20;
const MAX_EXITS: usize = 8;
/// Sprites are square and one, two or three cells on a side.
const SPRITE_SIDES: [usize; 3] = [16, 32, 48];
/// Characters a saved map may use for its tiles.
const SYMBOLS: &str = ".#~,:;=+*%@&abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Models write a coordinate as 7, 7.0 or "7"; all three are accepted.
fn lenient_int<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Number(n) => n.as_f64().map_or(0, |f| f.round() as i64),
        Value::String(s) => s.trim().parse::<f64>().map_or(0, |f| f.round() as i64),
        _ => 0,
    })
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct SpriteDraft {
    pub id: String,
    pub rows: Vec<String>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct TileDraft {
    pub id: String,
    pub name: String,
    pub walkable: bool,
    pub rows: Vec<String>,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct PropDraft {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(deserialize_with = "lenient_int")]
    pub x: i64,
    #[serde(deserialize_with = "lenient_int")]
    pub y: i64,
    pub blocking: bool,
    pub sprite: String,
    pub actions: Vec<ActionDraft>,
    /// For a container: what is in it, how hard its lock is, the name of its key.
    pub contents: Vec<crate::story::ThingDraft>,
    #[serde(deserialize_with = "lenient_int")]
    pub lock: i64,
    pub key: String,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct SecretDraft {
    #[serde(deserialize_with = "lenient_int")]
    pub x: i64,
    #[serde(deserialize_with = "lenient_int")]
    pub y: i64,
    #[serde(deserialize_with = "lenient_int")]
    pub dc: i64,
    pub kind: String,
    pub note: String,
    pub effects: Vec<Value>,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct CreatureDraft {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(deserialize_with = "lenient_int")]
    pub x: i64,
    #[serde(deserialize_with = "lenient_int")]
    pub y: i64,
    pub sprite: String,
    pub attitude: String,
    pub actions: Vec<ActionDraft>,
    pub monster: String,
    pub stats: Option<Stats>,
    #[serde(deserialize_with = "lenient_int")]
    pub count: i64,
    /// What a shopkeeper sells: things as in "give", each with a "price".
    pub shop: Vec<crate::story::ThingDraft>,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct ExitDraft {
    #[serde(deserialize_with = "lenient_int")]
    pub x: i64,
    #[serde(deserialize_with = "lenient_int")]
    pub y: i64,
    pub to: String,
    pub label: String,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct NeighborDraft {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct LocationDraft {
    /// Colours added for this place, by letters not in use yet.
    pub palette: BTreeMap<String, String>,
    pub id: String,
    pub name: String,
    pub description: String,
    pub legend: BTreeMap<String, String>,
    pub map: Vec<String>,
    pub spawn: Vec<Value>,
    pub props: Vec<PropDraft>,
    pub creatures: Vec<CreatureDraft>,
    pub exits: Vec<ExitDraft>,
    pub sprites: Vec<SpriteDraft>,
    pub new_tiles: Vec<TileDraft>,
    pub neighbors: Vec<NeighborDraft>,
    pub secrets: Vec<SecretDraft>,
    pub mood: String,
    pub light: String,
    pub weather: String,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct WorldDraft {
    pub quest: QuestDraft,
    pub title: String,
    pub summary: String,
    pub tone: String,
    pub palette: BTreeMap<String, String>,
    pub tiles: Vec<TileDraft>,
    pub location: LocationDraft,
}

/// A main quest as the master wrote it.
#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct QuestDraft {
    pub title: String,
    pub goal: String,
    pub acts: Vec<String>,
    pub ending: String,
    pub failure: String,
}

impl QuestDraft {
    pub fn quest(&self) -> crate::model::Quest {
        crate::model::Quest {
            title: self.title.trim().to_string(),
            goal: self.goal.trim().to_string(),
            acts: self.acts.iter().map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).take(6).collect(),
            ending: self.ending.trim().to_string(),
            failure: self.failure.trim().to_string(),
            ..Default::default()
        }
    }
}

/// The JSON object inside a model's answer, which may be wrapped in a code fence or prose.
pub fn extract_json(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end > start).then(|| &text[start..=end])
}

/// Fields the game reads as lists. Models sometimes write them as objects keyed by id.
const LISTS: [&str; 17] = ["tiles", "new_tiles", "sprites", "props", "creatures", "exits", "neighbors", "actions", "effects", "options", "attacks", "whispers", "feats", "texts", "facts", "contents", "secrets"];

/// Mends the commonest ways a model bends the answer format, before it is read: a list
/// written as an object keyed by id becomes a list, each entry keeping its key as its id.
pub fn loosen(value: &mut serde_json::Value) {
    use serde_json::Value;
    match value {
        Value::Object(map) => {
            for (key, inner) in map.iter_mut() {
                if LISTS.contains(&key.as_str())
                    && let Value::Object(entries) = inner
                {
                    let list = entries
                        .iter()
                        .map(|(id, entry)| {
                            let mut entry = entry.clone();
                            if let Value::Object(fields) = &mut entry {
                                fields.entry("id").or_insert_with(|| Value::String(id.clone()));
                            }
                            entry
                        })
                        .collect();
                    *inner = Value::Array(list);
                }
                loosen(inner);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(loosen),
        _ => {}
    }
}

/// Reads the master's answer, mending what can be mended first.
pub fn read_answer<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, String> {
    let json = extract_json(text).ok_or("no JSON in the answer")?;
    let mut value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    loosen(&mut value);
    serde_json::from_value(value).map_err(|e| e.to_string())
}

/// An id safe to use as a key: lower case, letters, digits and underscores only.
pub fn slug(raw: &str) -> String {
    if raw == "self" {
        return raw.to_string();
    }
    let cleaned: String = raw.trim().to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    cleaned.trim_matches('_').chars().take(48).collect()
}

fn hex_color(text: &str) -> Option<[u8; 3]> {
    let hex = text.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let part = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([part(0)?, part(2)?, part(4)?])
}

/// Makes rows into a square picture of `side` pixels: unknown characters become
/// transparent, the drawing is centred and stood on the bottom edge, the rest is cropped.
fn square(rows: &[String], side: usize, palette_len: usize) -> Sprite {
    let clean: Vec<Vec<char>> =
        rows.iter().take(side).map(|r| r.chars().take(side).map(|c| if index_of(c).is_some_and(|i| (i as usize) < palette_len) { c } else { '0' }).collect()).collect();
    let width = clean.iter().map(Vec::len).max().unwrap_or(0);
    let left = (side - width) / 2;
    let top = side - clean.len();
    let mut out = vec![vec!['0'; side]; side];
    for (y, row) in clean.iter().enumerate() {
        for (x, ch) in row.iter().enumerate() {
            out[top + y][left + x] = *ch;
        }
    }
    Sprite { rows: out.into_iter().map(|r| r.into_iter().collect()).collect() }
}

fn has_pixels(rows: &[String]) -> bool {
    rows.iter().any(|r| r.chars().any(|c| index_of(c).is_some_and(|i| i != 0)))
}

pub(crate) fn sprite_from(rows: &[String], palette_len: usize) -> Option<Sprite> {
    if !has_pixels(rows) {
        return None;
    }
    let extent = rows.len().max(rows.iter().map(|r| r.chars().count()).max().unwrap_or(0));
    let side = SPRITE_SIDES.into_iter().find(|s| *s >= extent).unwrap_or(SPRITE_SIDES[2]);
    Some(square(rows, side, palette_len))
}

/// A tile must cover its cell, so holes are filled with the colour it uses most.
pub(crate) fn tile_from(rows: &[String], palette_len: usize) -> Option<Sprite> {
    if !has_pixels(rows) {
        return None;
    }
    let mut sprite = square(rows, TILE as usize, palette_len);
    let mut counts = [0usize; PALETTE_MAX];
    for ch in sprite.rows.iter().flat_map(|r| r.chars()) {
        counts[index_of(ch).unwrap_or(0) as usize] += 1;
    }
    let commonest = (1..PALETTE_MAX).max_by_key(|i| counts[*i]).map(|i| crate::model::char_of(i as u8)).unwrap_or('1');
    for row in &mut sprite.rows {
        *row = row.replace('0', &commonest.to_string());
    }
    Some(sprite)
}

/// The pictures and tiles a world has collected so far; drawing a location adds to them.
pub struct Assets<'a> {
    pub palette_len: usize,
    pub tiles: &'a mut BTreeMap<String, TileDef>,
    pub sprites: &'a mut BTreeMap<String, Sprite>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Built {
    pub id: String,
    pub location: Location,
    /// Places the master mentioned that are not drawn yet.
    pub stubs: Vec<(String, Stub)>,
    /// Every repair that was made, for the log.
    pub notes: Vec<String>,
}

fn add_tiles(drafts: &[TileDraft], assets: &mut Assets, notes: &mut Vec<String>) {
    for draft in drafts {
        let id = slug(&draft.id);
        match tile_from(&draft.rows, assets.palette_len) {
            Some(sprite) if !id.is_empty() => {
                let name = if draft.name.trim().is_empty() { id.clone() } else { draft.name.trim().to_string() };
                assets.tiles.insert(id, TileDef { name, walkable: draft.walkable, sprite });
            }
            _ => notes.push(format!("tile '{}' had no usable picture and was skipped", draft.id)),
        }
    }
}

/// A shopkeeper's stock from what the master listed: at most thirty kinds, a few of each.
pub(crate) fn stock_of(drafts: &[crate::story::ThingDraft]) -> Vec<crate::story::Item> {
    drafts
        .iter()
        .filter_map(|d| d.item())
        .take(30)
        .map(|mut item| {
            // What is used up comes by the handful; a blade or a coat is the only one.
            item.count = if item.gear.is_some() { 1 } else { 3 };
            item
        })
        .collect()
}

fn clamp_cell(x: i64, y: i64) -> Cell {
    (x.clamp(0, MAP_W as i64 - 1) as usize, y.clamp(0, MAP_H as i64 - 1) as usize)
}

fn unique(base: String, fallback: &str, taken: &mut BTreeSet<String>) -> String {
    let base = if base.is_empty() { fallback.to_string() } else { base };
    let mut id = base.clone();
    let mut n = 2;
    while !taken.insert(id.clone()) {
        id = format!("{base}_{n}");
        n += 1;
    }
    id
}

fn on_edge((x, y): Cell) -> bool {
    x == 0 || y == 0 || x == MAP_W - 1 || y == MAP_H - 1
}

/// Turns a draft into a location that can be walked through.
///
/// `forced_id` names the place being drawn when it was already known as a stub.
/// `known` are the ids of every place the world already has. `arrived_from` is the place the
/// party walks in from: the result always has an exit leading back there.
pub fn build_location(draft: &LocationDraft, assets: &mut Assets, forced_id: Option<&str>, known: &BTreeSet<String>, arrived_from: Option<&str>) -> Result<Built, String> {
    let mut notes = Vec::new();
    if draft.map.iter().all(|r| r.trim().is_empty()) {
        return Err("the answer has no map".into());
    }

    for sprite in &draft.sprites {
        let id = slug(&sprite.id);
        match sprite_from(&sprite.rows, assets.palette_len) {
            Some(picture) if !id.is_empty() => {
                assets.sprites.insert(id, picture);
            }
            _ => notes.push(format!("sprite '{}' had no usable picture and was skipped", sprite.id)),
        }
    }
    add_tiles(&draft.new_tiles, assets, &mut notes);

    // The legend, with every character resolved to a tile that exists.
    let mut legend: BTreeMap<char, String> = BTreeMap::new();
    for (key, tile) in &draft.legend {
        let Some(ch) = key.chars().next() else { continue };
        let id = slug(tile);
        if assets.tiles.contains_key(&id) {
            legend.insert(ch, id);
        } else {
            notes.push(format!("legend names unknown tile '{tile}'; using {FALLBACK_TILE}"));
            legend.insert(ch, FALLBACK_TILE.to_string());
        }
    }

    // The grid of tile ids, exactly the size of a location.
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut given: Vec<Vec<Option<String>>> = Vec::new();
    for row in draft.map.iter().take(MAP_H) {
        let mut cells = Vec::new();
        for ch in row.chars().take(MAP_W) {
            let id = legend.get(&ch).cloned();
            if id.is_none() {
                notes.push(format!("map uses '{ch}', which the legend does not explain"));
                legend.insert(ch, FALLBACK_TILE.to_string());
            }
            cells.push(Some(id.unwrap_or_else(|| FALLBACK_TILE.to_string())));
        }
        given.push(cells);
    }
    for id in given.iter().flatten().flatten() {
        if assets.tiles.get(id).is_some_and(|t| t.walkable) {
            *counts.entry(id.as_str()).or_default() += 1;
        }
    }
    let floor = counts.iter().max_by_key(|(_, n)| **n).map(|(id, _)| id.to_string()).unwrap_or_else(|| FALLBACK_TILE.to_string());
    if !assets.tiles.get(&floor).is_some_and(|t| t.walkable) {
        return Err("no tile can be walked on".into());
    }
    let short = given.len() < MAP_H || given.iter().any(|r| r.len() < MAP_W);
    if short || draft.map.len() > MAP_H || draft.map.iter().any(|r| r.chars().count() > MAP_W) {
        notes.push(format!("map was not {MAP_W}x{MAP_H}; padded or cropped"));
    }
    let mut grid: Vec<Vec<String>> = (0..MAP_H).map(|y| (0..MAP_W).map(|x| given.get(y).and_then(|r| r.get(x)).cloned().flatten().unwrap_or_else(|| floor.clone())).collect()).collect();

    let mut taken = BTreeSet::new();
    let mut props: Vec<Prop> = Vec::new();
    for (n, p) in draft.props.iter().take(MAX_PROPS).enumerate() {
        let (x, y) = clamp_cell(p.x, p.y);
        let mut sprite = slug(&p.sprite);
        if !assets.sprites.contains_key(&sprite) {
            notes.push(format!("prop '{}' names sprite '{}', which was not drawn", p.id, p.sprite));
            sprite = MARKER_SPRITE.to_string();
        }
        props.push(Prop { id: unique(slug(&p.id), &format!("prop_{n}"), &mut taken), name: p.name.trim().to_string(), description: p.description.trim().to_string(), x, y, blocking: p.blocking, sprite, actions: read_actions(&p.actions, &mut notes), body: None, chest: (!p.contents.is_empty() || p.lock > 0 || !p.key.trim().is_empty()).then(|| crate::model::Chest { items: p.contents.iter().filter_map(crate::story::ThingDraft::item).collect(), lock: p.lock.clamp(0, 30) as i32, key: p.key.trim().to_string(), open: false }) });
    }

    let mut creatures: Vec<Creature> = Vec::new();
    for (n, c) in draft.creatures.iter().take(MAX_CREATURES).enumerate() {
        let (x, y) = clamp_cell(c.x, c.y);
        let mut sprite = slug(&c.sprite);
        if !assets.sprites.contains_key(&sprite) {
            notes.push(format!("creature '{}' names sprite '{}', which was not drawn", c.id, c.sprite));
            sprite = MARKER_SPRITE.to_string();
        }
        let attitude = match c.attitude.trim().to_lowercase().as_str() {
            "friendly" => Attitude::Friendly,
            "hostile" => Attitude::Hostile,
            _ => Attitude::Neutral,
        };
        creatures.push(Creature { id: unique(slug(&c.id), &format!("creature_{n}"), &mut taken), name: c.name.trim().to_string(), description: c.description.trim().to_string(), x, y, sprite, attitude, actions: read_actions(&c.actions, &mut notes), monster: Some(c.monster.trim().to_lowercase().replace([' ', '_'], "-")).filter(|m| !m.is_empty()), stats: c.stats.clone(), count: c.count.clamp(0, 8) as u8, mustered: false, stock: stock_of(&c.shop), wares: Vec::new(), shop_day: 0, haggle: 0, haggled: false });
    }

    let id = forced_id.map(str::to_string).unwrap_or_else(|| {
        let own = slug(&draft.id);
        if own.is_empty() { "start".to_string() } else { own }
    });

    // Exits, and the places they lead to.
    let mut stubs: Vec<(String, Stub)> = Vec::new();
    let mut stub_ids: BTreeSet<String> = BTreeSet::new();
    for n in &draft.neighbors {
        let nid = slug(&n.id);
        if nid.is_empty() || nid == id || known.contains(&nid) || !stub_ids.insert(nid.clone()) {
            continue;
        }
        let name = if n.name.trim().is_empty() { nid.clone() } else { n.name.trim().to_string() };
        stubs.push((nid, Stub { name, description: n.description.trim().to_string() }));
    }
    let mut exits: Vec<Exit> = Vec::new();
    for e in draft.exits.iter().take(MAX_EXITS) {
        let to = slug(&e.to);
        let cell = clamp_cell(e.x, e.y);
        if to.is_empty() || to == id {
            notes.push("an exit led nowhere and was dropped".to_string());
            continue;
        }
        if exits.iter().any(|x| (x.x, x.y) == cell) {
            notes.push(format!("two exits shared cell {cell:?}; the second was dropped"));
            continue;
        }
        if !known.contains(&to) && stub_ids.insert(to.clone()) {
            notes.push(format!("exit leads to '{to}', which was not described"));
            let name = if e.label.trim().is_empty() { to.clone() } else { e.label.trim().to_string() };
            stubs.push((to.clone(), Stub { name, description: String::new() }));
        }
        exits.push(Exit { x: cell.0, y: cell.1, to, label: e.label.trim().to_string() });
    }

    let spawn_given = match draft.spawn.as_slice() {
        [x, y, ..] => clamp_cell(x.as_f64().unwrap_or(0.0).round() as i64, y.as_f64().unwrap_or(0.0).round() as i64),
        _ => (MAP_W / 2, MAP_H / 2),
    };

    if let Some(from) = arrived_from
        && !exits.iter().any(|e| e.to == from)
    {
        notes.push(format!("no exit led back to '{from}'; one was added"));
        let free = |c: Cell| on_edge(c) && !exits.iter().any(|e| (e.x, e.y) == c);
        if let Some((x, y)) = nearest(spawn_given, free) {
            exits.push(Exit { x, y, to: from.to_string(), label: String::new() });
        }
    }

    // The world must go on: a new place with no way to anywhere new gets one, on the edge
    // farthest from where the party comes in, to a place the master will draw when it is reached.
    let leads_on = exits.iter().any(|e| stub_ids.contains(&e.to));
    if !leads_on && arrived_from.is_some() {
        let onward = format!("{id}_beyond");
        if !known.contains(&onward) {
            notes.push("no exit led anywhere new; one was added".to_string());
            let free = |c: Cell| on_edge(c) && !exits.iter().any(|e| (e.x, e.y) == c);
            let far = (if spawn_given.0 < MAP_W / 2 { MAP_W - 1 } else { 0 }, if spawn_given.1 < MAP_H / 2 { MAP_H - 1 } else { 0 });
            if let Some((x, y)) = nearest(far, free) {
                exits.push(Exit { x, y, to: onward.clone(), label: String::new() });
                stub_ids.insert(onward.clone());
                stubs.push((onward, Stub { name: "???".to_string(), description: String::new() }));
            }
        }
    }

    // Exits are stood on, so their cells must be open and clear.
    for exit in &exits {
        if !assets.tiles[&grid[exit.y][exit.x]].walkable {
            grid[exit.y][exit.x] = floor.clone();
        }
        for prop in props.iter_mut().filter(|p| (p.x, p.y) == (exit.x, exit.y)) {
            prop.blocking = false;
        }
    }

    let solid = |grid: &Vec<Vec<String>>, props: &Vec<Prop>, (x, y): Cell| !assets.tiles[&grid[y][x]].walkable || props.iter().any(|p| p.blocking && (p.x, p.y) == (x, y));
    let is_exit = |c: Cell| exits.iter().any(|e| (e.x, e.y) == c);

    // Nobody stands inside a wall or on a doorstep, and no two creatures share a cell.
    let mut stood: BTreeSet<Cell> = BTreeSet::new();
    for creature in &mut creatures {
        let here = (creature.x, creature.y);
        let free = |c: Cell| !solid(&grid, &props, c) && !is_exit(c) && !stood.contains(&c);
        if !free(here) {
            notes.push(format!("creature '{}' stood where it could not; moved", creature.id));
        }
        match nearest(here, free) {
            Some(cell) => {
                (creature.x, creature.y) = cell;
                stood.insert(cell);
            }
            None => return Err("the map has no room to stand".into()),
        }
    }

    let open = |c: Cell| !solid(&grid, &props, c) && !stood.contains(&c);
    let spawn = match nearest(spawn_given, |c| open(c) && !is_exit(c)) {
        Some(cell) => cell,
        None => return Err("the map has no room to stand".into()),
    };
    if spawn != spawn_given {
        notes.push("the starting cell was not free; moved to the nearest free one".to_string());
    }

    // Every exit must be reachable on foot. Where one is walled off, a path is cut to it.
    for i in 0..exits.len() {
        let exit = (exits[i].x, exits[i].y);
        let reached = reachable(spawn, |c| solid(&grid, &props, c) || stood.contains(&c));
        if reached[exit.1][exit.0] {
            continue;
        }
        notes.push(format!("exit to '{}' was walled off; a path was cut", exits[i].to));
        let Some(target) = nearest(exit, |(x, y)| reached[y][x]) else { continue };
        for (x, y) in elbow(exit, target) {
            if !assets.tiles[&grid[y][x]].walkable {
                grid[y][x] = floor.clone();
            }
            for prop in props.iter_mut().filter(|p| (p.x, p.y) == (x, y)) {
                prop.blocking = false;
            }
            if stood.remove(&(x, y)) {
                // A creature in the way steps aside to the nearest free cell off the path.
                let path = elbow(exit, target);
                let aside = nearest((x, y), |c| !solid(&grid, &props, c) && !stood.contains(&c) && !path.contains(&c) && c != spawn && !exits.iter().any(|e| (e.x, e.y) == c));
                if let (Some(cell), Some(creature)) = (aside, creatures.iter_mut().find(|c| (c.x, c.y) == (x, y))) {
                    (creature.x, creature.y) = cell;
                    stood.insert(cell);
                }
            }
        }
    }

    // Write the grid back as rows of symbols with a legend.
    let mut symbol_of: BTreeMap<&str, char> = BTreeMap::new();
    let mut symbols = SYMBOLS.chars();
    for tile in grid.iter().flatten() {
        if !symbol_of.contains_key(tile.as_str()) {
            let symbol = symbols.next().ok_or("the map uses more kinds of tile than a save can hold")?;
            symbol_of.insert(tile, symbol);
        }
    }
    let map: Vec<String> = grid.iter().map(|row| row.iter().map(|t| symbol_of[t.as_str()]).collect()).collect();
    let legend: BTreeMap<char, String> = symbol_of.iter().map(|(tile, symbol)| (*symbol, tile.to_string())).collect();

    let name = if draft.name.trim().is_empty() { id.clone() } else { draft.name.trim().to_string() };
    let location = Location { name, description: draft.description.trim().to_string(), map, legend, spawn, props, creatures, exits, light: draft.light.trim().to_lowercase(), weather: draft.weather.trim().to_lowercase(), mood: draft.mood.trim().to_lowercase(), marks: Vec::new(), seen: Vec::new(), items: Vec::new(), secrets: draft.secrets.iter().map(|s| { let (x, y) = clamp_cell(s.x, s.y); crate::model::Secret { x, y, dc: if s.dc <= 0 { 13 } else { s.dc.clamp(5, 30) as i32 }, kind: s.kind.trim().to_lowercase(), note: s.note.trim().to_string(), effects: crate::story::read_effects(&s.effects, &mut notes), found: false } }).collect() };
    Ok(Built { id, location, stubs, notes })
}

/// A new world from the master's first answer. `request` is what the host asked for and
/// `language` the language the master was told to write in; both are kept for later requests.
pub fn build_world(draft: &WorldDraft, request: &str, language: &str) -> Result<(World, Vec<String>), String> {
    let mut notes = Vec::new();
    let mut palette = default_palette();
    for (key, hex) in &draft.palette {
        let index = key.chars().next().and_then(index_of).filter(|i| *i > 0);
        match (index, hex_color(hex)) {
            (Some(index), Some(color)) => {
                let slot = index as usize - 1;
                if slot >= palette.colors.len() {
                    palette.colors.resize(slot + 1, [0, 0, 0]);
                }
                palette.colors[slot] = color;
            }
            _ => notes.push(format!("palette entry '{key}': '{hex}' could not be read")),
        }
    }
    palette.colors.truncate(PALETTE_MAX - 1);

    let mut tiles = base_tiles();
    let mut sprites = base_sprites();
    let mut assets = Assets { palette_len: palette.len(), tiles: &mut tiles, sprites: &mut sprites };
    add_tiles(&draft.tiles, &mut assets, &mut notes);
    let built = build_location(&draft.location, &mut assets, None, &BTreeSet::new(), None)?;
    notes.extend(built.notes);

    let fallback = |text: &str, other: &str| if text.trim().is_empty() { other.to_string() } else { text.trim().to_string() };
    let world = World {
        version: SAVE_VERSION,
        cues: Vec::new(),
        cue_seq: 0,
        travel: None,
        moves: Vec::new(),
        acting: None,
        quest: draft.quest.quest(),
        clock: crate::model::morning(),
        facing: (0, 0),
        title: fallback(&draft.title, &built.location.name),
        summary: draft.summary.trim().to_string(),
        tone: draft.tone.trim().to_string(),
        request: request.trim().to_string(),
        language: language.to_string(),
        palette,
        tiles,
        sprites,
        player: Player::in_cell(built.location.spawn.0, built.location.spawn.1),
        current: built.id.clone(),
        stubs: built.stubs.into_iter().collect(),
        locations: BTreeMap::from([(built.id, built.location)]),
        story: Story::default(),
        violence: true,
        sex: true,
        party: Vec::new(),
        pending: None,
        fight: None,
        owners: Vec::new(),
        present: Vec::new(),
        gm: None,
        places: Default::default(),
        host_place: None,
        fight_place: String::new(),
        pending_place: String::new(),
        table: TableRules::default(),
        offers: Vec::new(),
        secrets_due: Vec::new(),
        others: Vec::new(),
        level_ups: Vec::new(),
    };
    Ok((world, notes))
}

impl World {
    /// Takes colours the master adds for a new scene. Only letters not in use yet are taken:
    /// a colour already drawn with stays what it was, or every old picture would change.
    pub fn add_colors(&mut self, colors: &BTreeMap<String, String>) {
        let mut wanted: Vec<(usize, [u8; 3])> = colors.iter().filter_map(|(key, hex)| Some((key.chars().next().and_then(index_of).filter(|i| *i > 0)? as usize - 1, hex_color(hex)?))).collect();
        wanted.sort();
        for (slot, color) in wanted {
            if slot >= self.palette.colors.len() && slot < PALETTE_MAX - 1 {
                self.palette.colors.resize(slot, [0, 0, 0]);
                self.palette.colors.push(color);
            }
        }
    }

    /// Draws a place that was only a stub. The party is not moved.
    pub fn add_location(&mut self, draft: &LocationDraft, id: &str, arrived_from: Option<&str>) -> Result<Vec<String>, String> {
        self.add_colors(&draft.palette);
        let known: BTreeSet<String> = self.locations.keys().chain(self.stubs.keys()).cloned().collect();
        let mut assets = Assets { palette_len: self.palette.len(), tiles: &mut self.tiles, sprites: &mut self.sprites };
        let built = build_location(draft, &mut assets, Some(id), &known, arrived_from)?;
        self.stubs.remove(id);
        for (stub_id, stub) in built.stubs {
            self.stubs.entry(stub_id).or_insert(stub);
        }
        self.locations.insert(built.id, built.location);
        Ok(built.notes)
    }

    /// Where the party appears in `to` after walking in from `from`: beside the exit that
    /// leads back, or at the location's starting cell when there is none.
    pub fn arrival_cell(&self, to: &str, from: &str) -> Cell {
        let Some(location) = self.locations.get(to) else { return (MAP_W / 2, MAP_H / 2) };
        let Some(door) = location.exits.iter().find(|e| e.to == from) else { return location.spawn };
        let free = |(x, y): Cell| !location.blocked(x as i32, y as i32, &self.tiles) && location.exit_at(x, y).is_none();
        nearest((door.x, door.y), free).unwrap_or(location.spawn)
    }

    /// Moves the party through an exit into a place that is already drawn.
    pub fn travel(&mut self, to: &str) {
        let (x, y) = self.arrival_cell(to, &self.current.clone());
        self.current = to.to_string();
        self.player = Player::in_cell(x, y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake;
    use serde_json::json;

    fn draft(value: Value) -> LocationDraft {
        serde_json::from_value(value).unwrap()
    }

    /// A plain field, with whatever `extra` overrides.
    fn field(extra: Value) -> LocationDraft {
        let mut base = json!({
            "id": "field", "name": "Field", "legend": { ".": "grass", "#": "stone_wall" },
            "map": vec![".".repeat(MAP_W); MAP_H], "spawn": [5, 5]
        });
        for (key, value) in extra.as_object().unwrap() {
            base[key] = value.clone();
        }
        draft(base)
    }

    fn build(d: &LocationDraft, from: Option<&str>) -> Built {
        let (mut tiles, mut sprites) = (base_tiles(), base_sprites());
        let mut assets = Assets { palette_len: 16, tiles: &mut tiles, sprites: &mut sprites };
        build_location(d, &mut assets, None, &BTreeSet::new(), from).unwrap()
    }

    fn every_exit_is_reachable(location: &Location, tiles: &BTreeMap<String, TileDef>) -> bool {
        let seen = reachable(location.spawn, |(x, y)| location.blocked(x as i32, y as i32, tiles));
        location.exits.iter().all(|e| seen[e.y][e.x])
    }

    #[test]
    fn the_fake_masters_world_is_playable_as_built() {
        let d: WorldDraft = serde_json::from_str(&fake::world("anything")).unwrap();
        let (world, _) = build_world(&d, "anything", "English").unwrap();
        let here = world.here();
        assert_eq!(here.map.len(), MAP_H);
        assert!(here.map.iter().all(|r| r.len() == MAP_W));
        assert!(!here.blocked(here.spawn.0 as i32, here.spawn.1 as i32, &world.tiles));
        assert!(every_exit_is_reachable(here, &world.tiles));
        assert_eq!(here.exits.len(), 2);
        assert_eq!(world.stubs.len(), 2);
        assert!(world.tiles.contains_key("moss") && world.sprites.contains_key("campfire"));
        assert_eq!(world.player.cell(), (here.spawn.0 as i32, here.spawn.1 as i32));
    }

    #[test]
    fn a_ragged_map_is_squared_up_and_unknown_tiles_fall_back() {
        let d = field(json!({ "legend": { ".": "grass", "#": "obsidian" }, "map": ["..#..", ".?.", ""] }));
        let b = build(&d, None);
        assert_eq!(b.location.map.len(), MAP_H);
        assert!(b.location.map.iter().all(|r| r.len() == MAP_W));
        assert_eq!(b.location.legend.values().collect::<Vec<_>>(), ["grass"]);
        assert!(b.notes.iter().any(|n| n.contains("obsidian")));
        assert!(b.notes.iter().any(|n| n.contains("uses '?'")));
        assert!(b.notes.iter().any(|n| n.contains("padded or cropped")));
    }

    #[test]
    fn a_walled_off_exit_gets_a_path_cut_to_it() {
        let mut map = vec![".".repeat(MAP_W); MAP_H];
        for row in &mut map {
            row.replace_range(20..21, "#");
        }
        let d = field(json!({ "map": map, "exits": [{ "x": 29, "y": 3, "to": "beyond", "label": "Gate" }] }));
        let b = build(&d, None);
        assert!(every_exit_is_reachable(&b.location, &base_tiles()));
        assert!(b.notes.iter().any(|n| n.contains("walled off")));
        // Only the one cell of wall on the exit's row was opened.
        let wall = *b.location.legend.iter().find(|(_, tile)| tile.as_str() == "stone_wall").unwrap().0 as u8;
        assert_eq!(b.location.map.iter().filter(|r| r.as_bytes()[20] != wall).count(), 1);
        assert_ne!(b.location.map[3].as_bytes()[20], wall);
    }

    #[test]
    fn an_exit_drawn_inside_a_wall_is_opened() {
        let mut map = vec![".".repeat(MAP_W); MAP_H];
        map[0] = "#".repeat(MAP_W);
        let b = build(&field(json!({ "map": map, "exits": [{ "x": 7, "y": 0, "to": "north" }] })), None);
        assert_eq!(b.location.tile_id(7, 0), Some("grass"));
        assert_eq!(b.location.tile_id(8, 0), Some("stone_wall"));
        assert!(every_exit_is_reachable(&b.location, &base_tiles()));
    }

    #[test]
    fn a_start_inside_a_wall_moves_to_the_nearest_free_cell() {
        let mut map = vec![".".repeat(MAP_W); MAP_H];
        map[5].replace_range(4..7, "###");
        let b = build(&field(json!({ "map": map })), None);
        assert!(!b.location.blocked(b.location.spawn.0 as i32, b.location.spawn.1 as i32, &base_tiles()));
        assert!(b.notes.iter().any(|n| n.contains("starting cell")));
    }

    #[test]
    fn something_named_but_not_drawn_gets_the_marker_picture() {
        let d = field(json!({ "props": [{ "id": "well", "name": "Well", "x": 9, "y": 9, "blocking": true, "sprite": "well" }] }));
        let b = build(&d, None);
        assert_eq!(b.location.props[0].sprite, MARKER_SPRITE);
        assert!(b.notes.iter().any(|n| n.contains("sprite 'well'")));
    }

    #[test]
    fn walking_in_from_somewhere_always_leaves_a_way_back() {
        let b = build(&field(json!({})), Some("camp"));
        let back = b.location.exits.iter().find(|e| e.to == "camp").expect("an exit back");
        assert!(on_edge((back.x, back.y)));
        assert!(b.notes.iter().any(|n| n.contains("led back")));
        // When the master drew one itself, nothing is added.
        let own = build(&field(json!({ "exits": [{ "x": 0, "y": 4, "to": "camp" }] })), Some("camp"));
        assert_eq!(own.location.exits.len(), 1);
    }

    #[test]
    fn an_exit_to_a_place_nobody_described_still_gets_a_stub() {
        let d = field(json!({
            "exits": [{ "x": 0, "y": 2, "to": "Old Mill!", "label": "A mill" }, { "x": 0, "y": 9, "to": "cave" }],
            "neighbors": [{ "id": "cave", "name": "Cave", "description": "Dark." }]
        }));
        let b = build(&d, None);
        let stubs: BTreeMap<_, _> = b.stubs.into_iter().collect();
        assert_eq!(stubs["old_mill"].name, "A mill");
        assert_eq!(stubs["cave"].description, "Dark.");
        assert_eq!(b.location.exits[0].to, "old_mill");
    }

    #[test]
    fn creatures_do_not_stand_in_walls_on_exits_or_on_each_other() {
        let mut map = vec![".".repeat(MAP_W); MAP_H];
        map[3].replace_range(3..4, "#");
        let d = field(json!({
            "map": map,
            "exits": [{ "x": 0, "y": 8, "to": "west" }],
            "creatures": [
                { "id": "a", "x": 3, "y": 3 }, { "id": "b", "x": 0, "y": 8 }, { "id": "c", "x": 12, "y": 12 }, { "id": "c", "x": 12, "y": 12 }
            ]
        }));
        let b = build(&d, None);
        let cells: BTreeSet<Cell> = b.location.creatures.iter().map(|c| (c.x, c.y)).collect();
        assert_eq!(cells.len(), 4);
        assert!(!cells.contains(&(3, 3)) && !cells.contains(&(0, 8)));
        let ids: Vec<&str> = b.location.creatures.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "c", "c_2"]);
    }

    #[test]
    fn coordinates_are_read_as_numbers_floats_or_strings_and_kept_on_the_map() {
        let d = field(json!({ "props": [{ "id": "a", "x": "7", "y": 3.6 }, { "id": "b", "x": -4, "y": 99 }] }));
        let b = build(&d, None);
        assert_eq!((b.location.props[0].x, b.location.props[0].y), (7, 4));
        assert_eq!((b.location.props[1].x, b.location.props[1].y), (0, MAP_H - 1));
    }

    #[test]
    fn a_tile_with_holes_is_filled_and_an_oversized_sprite_becomes_two_cells() {
        let mut rows = vec!["9".repeat(16); 16];
        rows[4] = "9990999999999999".into();
        let big = vec!["1".repeat(20); 20];
        let d = field(json!({
            "new_tiles": [{ "id": "slab", "walkable": true, "rows": rows }],
            "sprites": [{ "id": "ogre", "rows": big }, { "id": "ghost", "rows": ["000", "0z0"] }]
        }));
        let (mut tiles, mut sprites) = (base_tiles(), base_sprites());
        let mut assets = Assets { palette_len: 16, tiles: &mut tiles, sprites: &mut sprites };
        let b = build_location(&d, &mut assets, None, &BTreeSet::new(), None).unwrap();
        assert!(tiles["slab"].sprite.rows.iter().all(|r| r == &"9".repeat(16)));
        assert_eq!((sprites["ogre"].width(), sprites["ogre"].height()), (32, 32));
        // Twenty columns centred in thirty-two, standing on the bottom edge.
        assert_eq!(sprites["ogre"].index_at(6, 31), 1);
        assert_eq!(sprites["ogre"].index_at(5, 31), 0);
        assert_eq!(sprites["ogre"].index_at(6, 11), 0);
        assert!(!sprites.contains_key("ghost"));
        assert!(b.notes.iter().any(|n| n.contains("sprite 'ghost'")));
    }

    #[test]
    fn an_answer_without_a_map_is_refused() {
        let (mut tiles, mut sprites) = (base_tiles(), base_sprites());
        let mut assets = Assets { palette_len: 16, tiles: &mut tiles, sprites: &mut sprites };
        let result = build_location(&draft(json!({ "name": "Nothing" })), &mut assets, None, &BTreeSet::new(), None);
        assert_eq!(result, Err("the answer has no map".to_string()));
    }

    #[test]
    fn the_master_recolours_the_fixed_slots_and_adds_its_own() {
        let d: WorldDraft = serde_json::from_value(json!({
            "palette": { "7": "#aa00ff", "g": "102030", "h": "nonsense", "0": "#ffffff" },
            "location": { "legend": { ".": "grass" }, "map": ["."] }
        }))
        .unwrap();
        let (world, notes) = build_world(&d, "", "Russian").unwrap();
        assert_eq!(world.palette.rgba(7), [0xaa, 0x00, 0xff, 255]);
        assert_eq!(world.palette.rgba(16), [0x10, 0x20, 0x30, 255]);
        assert_eq!(world.palette.rgba(1), [0x1b, 0x14, 0x12, 255]);
        assert_eq!(world.palette.len(), 17);
        assert_eq!(notes.iter().filter(|n| n.contains("palette entry")).count(), 2);
        assert_eq!(world.language, "Russian");
    }

    #[test]
    fn json_is_found_inside_a_fence_or_prose() {
        assert_eq!(extract_json("```json\n{\"a\": {\"b\": 1}}\n```"), Some("{\"a\": {\"b\": 1}}"));
        assert_eq!(extract_json("Here you go: {\"a\":1} enjoy"), Some("{\"a\":1}"));
        assert_eq!(extract_json("no braces"), None);
        // A list written as an object keyed by id is read as the list it meant.
        let draft: LocationDraft = read_answer(r#"{"name": "x", "new_tiles": {"flesh": {"name": "Flesh", "walkable": true, "rows": []}}}"#).unwrap();
        assert_eq!(draft.new_tiles.len(), 1);
        assert_eq!(draft.new_tiles[0].id, "flesh");
        assert_eq!(extract_json("} backwards {"), None);
    }

    #[test]
    fn drawing_a_stub_replaces_it_and_travel_puts_the_party_beside_the_way_back() {
        let d: WorldDraft = serde_json::from_str(&fake::world("x")).unwrap();
        let (mut world, _) = build_world(&d, "x", "English").unwrap();
        let from = world.current.clone();
        let drawn: LocationDraft = serde_json::from_str(&fake::location("east_road", "The East Road", &from)).unwrap();
        world.add_location(&drawn, "east_road", Some(&from)).unwrap();
        assert!(!world.stubs.contains_key("east_road") && world.locations.contains_key("east_road"));
        assert!(world.stubs.contains_key("east_road_beyond") && world.stubs.contains_key("south_marsh"));

        world.travel("east_road");
        let here = world.here();
        let back = here.exits.iter().find(|e| e.to == from).unwrap();
        let (x, y) = world.player.cell();
        assert!(here.exit_at(x as usize, y as usize).is_none());
        assert!(!here.blocked(x, y, &world.tiles));
        assert!((x - back.x as i32).abs() + (y - back.y as i32).abs() <= 2);
        assert!(every_exit_is_reachable(here, &world.tiles));
    }

    #[test]
    fn a_world_survives_being_saved_and_loaded() {
        let d: WorldDraft = serde_json::from_str(&fake::world("save me")).unwrap();
        let (world, _) = build_world(&d, "save me", "English").unwrap();
        let loaded: World = serde_json::from_str(&serde_json::to_string(&world).unwrap()).unwrap();
        assert_eq!(loaded, world);
    }
}
