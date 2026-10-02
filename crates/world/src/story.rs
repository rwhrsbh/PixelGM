//! What happens in a world after it is drawn: actions with outcomes the game master wrote
//! ahead of time, the effects those outcomes have, and the record of what has been said.

use crate::base::MARKER_SPRITE;
use crate::build::slug;
use crate::model::{Attitude, Creature, Exit, Location, MAP_H, MAP_W, Player, Prop, Stub, World};
use crate::party::{Branch, PendingCheck, Stats};
use rules::{Ability, Rng};
use crate::path::nearest;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Dice may be written as "2d6" or as a bare number.
pub(crate) fn lenient_text<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => s,
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    })
}

pub(crate) fn lenient_int<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Number(n) => n.as_f64().map_or(0, |f| f.round() as i64),
        Value::String(s) => s.trim().parse::<f64>().map_or(0, |f| f.round() as i64),
        _ => 0,
    })
}

/// One change to the world. The game master writes these; the game checks and applies them.
/// In an action that belongs to a prop or creature, the id `self` means that thing.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Effect {
    /// A prop or creature leaves the scene.
    Remove { id: String },
    /// The party gains an item.
    Give {
        name: String,
        #[serde(default)]
        description: String,
    },
    /// The party loses an item.
    Take { name: String },
    /// A thing looks different from now on.
    Describe { id: String, description: String },
    Attitude { id: String, attitude: Attitude },
    Move {
        id: String,
        #[serde(deserialize_with = "lenient_int")]
        x: i64,
        #[serde(deserialize_with = "lenient_int")]
        y: i64,
    },
    /// One cell of ground changes.
    Tile {
        #[serde(deserialize_with = "lenient_int")]
        x: i64,
        #[serde(deserialize_with = "lenient_int")]
        y: i64,
        tile: String,
    },
    /// Something new appears. `kind` is `prop` or `creature`.
    Spawn {
        kind: String,
        id: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        description: String,
        #[serde(deserialize_with = "lenient_int")]
        x: i64,
        #[serde(deserialize_with = "lenient_int")]
        y: i64,
        #[serde(default)]
        sprite: String,
        #[serde(default)]
        blocking: bool,
        #[serde(default)]
        attitude: Attitude,
        /// For a creature: the SRD monster it fights as, or numbers of its own.
        #[serde(default)]
        monster: Option<String>,
        #[serde(default)]
        stats: Option<Stats>,
    },
    /// A way out opens.
    Exit {
        #[serde(deserialize_with = "lenient_int")]
        x: i64,
        #[serde(deserialize_with = "lenient_int")]
        y: i64,
        to: String,
        #[serde(default)]
        label: String,
    },
    /// Something worth remembering for the rest of the campaign.
    Fact { text: String },
    /// The outcome is uncertain: the party rolls, and one of two prepared branches follows.
    Check {
        ability: String,
        #[serde(default)]
        skill: Option<String>,
        #[serde(deserialize_with = "lenient_int")]
        dc: i64,
        #[serde(default)]
        success: Branch,
        #[serde(default)]
        failure: Branch,
    },
    /// The party is hurt outside a fight. `amount` is dice or a number.
    Damage {
        #[serde(deserialize_with = "lenient_text")]
        amount: String,
    },
    Heal {
        #[serde(deserialize_with = "lenient_text")]
        amount: String,
    },
    /// Experience for something achieved in the story.
    Xp {
        #[serde(deserialize_with = "lenient_int")]
        amount: i64,
    },
    /// The hostile creatures here attack.
    Fight,
    /// A hero did something out of the ordinary. The game keeps count and decides whether
    /// a secret skill comes of it.
    Deed {
        #[serde(default)]
        hero: String,
    },
    /// The story brings a dead hero back.
    Revive {
        #[serde(default)]
        hero: String,
    },
}

/// Something the players can do with a thing without asking the game master: the outcome
/// was written when the thing was.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Action {
    pub label: String,
    pub outcome: String,
    pub effects: Vec<Effect>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct ActionDraft {
    pub label: String,
    pub outcome: String,
    pub effects: Vec<Value>,
}

const MAX_ACTIONS: usize = 4;
const MAX_LABEL: usize = 48;

/// Reads effects one by one, so a single malformed effect does not cost the rest.
pub fn read_effects(raw: &[Value], notes: &mut Vec<String>) -> Vec<Effect> {
    raw.iter()
        .filter_map(|value| match serde_json::from_value::<Effect>(value.clone()) {
            Ok(effect) => Some(effect),
            Err(e) => {
                notes.push(format!("an effect could not be read and was dropped: {e}"));
                None
            }
        })
        .collect()
}

pub fn read_actions(drafts: &[ActionDraft], notes: &mut Vec<String>) -> Vec<Action> {
    drafts
        .iter()
        .filter(|d| !d.label.trim().is_empty() && !d.outcome.trim().is_empty())
        .take(MAX_ACTIONS)
        .map(|d| Action { label: d.label.trim().chars().take(MAX_LABEL).collect(), outcome: d.outcome.trim().to_string(), effects: read_effects(&d.effects, notes) })
        .collect()
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Item {
    pub name: String,
    pub description: String,
    pub count: u32,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Speaker {
    Master,
    Player,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Line {
    pub speaker: Speaker,
    pub text: String,
}

/// Everything the game master needs to remember, apart from the map: what the party carries,
/// what was said lately, and what was said long ago in shorter words.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Story {
    pub inventory: Vec<Item>,
    /// The story since the last chapter was written, word for word.
    pub log: Vec<Line>,
    /// Older story, one detailed summary per chapter.
    pub chapters: Vec<String>,
    /// Facts that must never be forgotten; only ever added to.
    pub facts: Vec<String>,
}

const MAX_FACTS: usize = 300;
/// Lines of the newest story kept word for word when a chapter is written, so the master
/// does not lose the thread of the scene in progress.
pub const KEPT_LINES: usize = 6;

impl Story {
    pub fn say(&mut self, speaker: Speaker, text: &str) {
        let text = text.trim();
        if !text.is_empty() {
            self.log.push(Line { speaker, text: text.to_string() });
        }
    }

    /// A rough count of the tokens the word-for-word story takes in a request. Counting
    /// exactly would cost a request; three characters to a token is close for mixed text and
    /// errs on the side of compressing early.
    pub fn log_tokens(&self) -> usize {
        self.log.iter().map(|l| l.text.chars().count() + 8).sum::<usize>() / 3
    }

    /// Whether the story has grown enough that the older part should be written up.
    pub fn needs_chapter(&self, budget_tokens: usize) -> bool {
        self.log.len() > KEPT_LINES && self.log_tokens() * 5 >= budget_tokens * 4
    }

    /// Replaces all but the newest lines with a chapter the master wrote about them.
    pub fn close_chapter(&mut self, chapter: &str, facts: &[String]) {
        if !chapter.trim().is_empty() {
            self.chapters.push(chapter.trim().to_string());
            let keep_from = self.log.len().saturating_sub(KEPT_LINES);
            self.log.drain(..keep_from);
        }
        for fact in facts {
            self.remember(fact);
        }
    }

    pub fn remember(&mut self, fact: &str) {
        let fact = fact.trim();
        if !fact.is_empty() && self.facts.len() < MAX_FACTS && !self.facts.iter().any(|f| f.eq_ignore_ascii_case(fact)) {
            self.facts.push(fact.to_string());
        }
    }

    fn give(&mut self, name: &str, description: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        match self.inventory.iter_mut().find(|i| i.name.to_lowercase() == name.to_lowercase()) {
            Some(item) => item.count += 1,
            None => self.inventory.push(Item { name: name.to_string(), description: description.trim().to_string(), count: 1 }),
        }
    }

    fn take(&mut self, name: &str) -> bool {
        let wanted = name.trim().to_lowercase();
        let Some(at) = self.inventory.iter().position(|i| i.name.to_lowercase() == wanted) else { return false };
        if self.inventory[at].count > 1 {
            self.inventory[at].count -= 1;
        } else {
            self.inventory.remove(at);
        }
        true
    }
}

fn clamp_cell(x: i64, y: i64) -> (usize, usize) {
    (x.clamp(0, MAP_W as i64 - 1) as usize, y.clamp(0, MAP_H as i64 - 1) as usize)
}

/// Characters a location may use for tiles added after it was drawn.
const SPARE_SYMBOLS: &str = "!$^()[]{}<>?/|`'\"-_";

impl Location {
    fn has_thing(&self, id: &str) -> bool {
        self.props.iter().any(|p| p.id == id) || self.creatures.iter().any(|c| c.id == id)
    }

    /// Changes the tile of one cell, giving the tile a map symbol if it has none yet.
    fn set_tile(&mut self, x: usize, y: usize, tile: &str) -> bool {
        let symbol = match self.legend.iter().find(|(_, id)| id.as_str() == tile) {
            Some((symbol, _)) => *symbol,
            None => {
                let Some(free) = SPARE_SYMBOLS.chars().find(|c| !self.legend.contains_key(c)) else { return false };
                self.legend.insert(free, tile.to_string());
                free
            }
        };
        let mut row: Vec<char> = self.map[y].chars().collect();
        row[x] = symbol;
        self.map[y] = row.into_iter().collect();
        true
    }
}

impl World {
    /// Applies the master's effects to the place the party is in. `me` is the thing whose
    /// action is being carried out, for the id `self`. What could not be done is noted and
    /// skipped: an effect naming something that is not there must not stop the rest.
    pub fn apply(&mut self, effects: &[Effect], me: Option<&str>) -> Vec<String> {
        let mut notes = Vec::new();
        let here = self.current.clone();
        let resolve = |id: &str| if id == "self" { me.unwrap_or("self").to_string() } else { slug(id) };
        for (n, effect) in effects.iter().enumerate() {
            // Effects on the party itself come first: they need the whole world, not just
            // the place.
            match effect {
                Effect::Check { ability, skill, dc, success, failure } => {
                    let Some(ability) = Ability::from_index(ability.trim()) else {
                        notes.push(format!("check: there is no ability '{ability}'"));
                        continue;
                    };
                    let skill = skill.as_ref().map(|s| s.trim().to_lowercase().replace([' ', '_'], "-")).filter(|s| !s.is_empty());
                    // Everything after a check waits for the die.
                    self.pending_place = self.current.clone();
                    self.pending = Some(PendingCheck { ability, skill, difficulty: (*dc).clamp(1, 40) as i32, success: success.clone(), failure: failure.clone(), me: me.map(str::to_string), rest: effects[n + 1..].to_vec() });
                    break;
                }
                Effect::Damage { amount } => {
                    self.hurt_party(amount, &mut Rng::from_clock());
                    continue;
                }
                Effect::Heal { amount } => {
                    self.heal_party(amount, &mut Rng::from_clock());
                    continue;
                }
                Effect::Xp { amount } => {
                    let ups = self.award_story_xp(*amount);
                    self.level_ups.extend(ups);
                    continue;
                }
                Effect::Fight => {
                    if !self.start_fight(Rng::from_clock()) {
                        notes.push("fight: nothing hostile is here".to_string());
                    }
                    continue;
                }
                Effect::Deed { hero } => {
                    self.deed(hero, &mut Rng::from_clock());
                    continue;
                }
                Effect::Revive { hero } => {
                    if !self.revive(hero) {
                        notes.push(format!("revive: nobody dead is called '{hero}'"));
                    }
                    continue;
                }
                _ => {}
            }
            let tiles = &self.tiles;
            let Some(location) = self.locations.get_mut(&here) else { break };
            match effect {
                Effect::Remove { id } => {
                    let id = resolve(id);
                    if !location.has_thing(&id) {
                        notes.push(format!("remove: nothing called '{id}' is here"));
                    }
                    location.props.retain(|p| p.id != id);
                    location.creatures.retain(|c| c.id != id);
                }
                Effect::Give { name, description } => self.story.give(name, description),
                Effect::Take { name } => {
                    if !self.story.take(name) {
                        notes.push(format!("take: the party has no '{name}'"));
                    }
                }
                Effect::Describe { id, description } => {
                    let id = resolve(id);
                    if let Some(p) = location.props.iter_mut().find(|p| p.id == id) {
                        p.description = description.trim().to_string();
                    } else if let Some(c) = location.creatures.iter_mut().find(|c| c.id == id) {
                        c.description = description.trim().to_string();
                    } else {
                        notes.push(format!("describe: nothing called '{id}' is here"));
                    }
                }
                Effect::Attitude { id, attitude } => {
                    let id = resolve(id);
                    match location.creatures.iter_mut().find(|c| c.id == id) {
                        Some(c) => c.attitude = *attitude,
                        None => notes.push(format!("attitude: no creature called '{id}' is here")),
                    }
                }
                Effect::Move { id, x, y } => {
                    let id = resolve(id);
                    let wanted = clamp_cell(*x, *y);
                    if let Some(p) = location.props.iter_mut().find(|p| p.id == id) {
                        (p.x, p.y) = wanted;
                    } else if location.creatures.iter().any(|c| c.id == id) {
                        // The creature's own cell does not count as taken while it moves.
                        let mut without = location.clone();
                        without.creatures.retain(|c| c.id != id);
                        let free = |(cx, cy): (usize, usize)| !without.blocked(cx as i32, cy as i32, tiles) && without.exit_at(cx, cy).is_none();
                        if let (Some(cell), Some(c)) = (nearest(wanted, free), location.creatures.iter_mut().find(|c| c.id == id)) {
                            (c.x, c.y) = cell;
                        }
                    } else {
                        notes.push(format!("move: nothing called '{id}' is here"));
                    }
                }
                Effect::Tile { x, y, tile } => {
                    let tile = slug(tile);
                    let (x, y) = clamp_cell(*x, *y);
                    if !tiles.contains_key(&tile) {
                        notes.push(format!("tile: there is no tile '{tile}'"));
                    } else if !location.set_tile(x, y, &tile) {
                        notes.push("tile: the place cannot hold another kind of tile".to_string());
                    }
                }
                Effect::Spawn { kind, id, name, description, x, y, sprite, blocking, attitude, monster, stats } => {
                    let mut id = slug(id);
                    if id.is_empty() {
                        id = "thing".to_string();
                    }
                    let base = id.clone();
                    let mut n = 2;
                    while location.has_thing(&id) {
                        id = format!("{base}_{n}");
                        n += 1;
                    }
                    let mut sprite = slug(sprite);
                    if !self.sprites.contains_key(&sprite) {
                        notes.push(format!("spawn: sprite '{sprite}' was not drawn"));
                        sprite = MARKER_SPRITE.to_string();
                    }
                    let wanted = clamp_cell(*x, *y);
                    let name = if name.trim().is_empty() { id.clone() } else { name.trim().to_string() };
                    if kind.trim().eq_ignore_ascii_case("creature") {
                        let free = |(cx, cy): (usize, usize)| !location.blocked(cx as i32, cy as i32, tiles) && location.exit_at(cx, cy).is_none();
                        if let Some((cx, cy)) = nearest(wanted, free) {
                            location.creatures.push(Creature { id, name, description: description.trim().to_string(), x: cx, y: cy, sprite, attitude: *attitude, actions: Vec::new(), monster: monster.as_deref().map(|m| m.trim().to_lowercase().replace([' ', '_'], "-")).filter(|m| !m.is_empty()), stats: stats.clone() });
                        }
                    } else {
                        location.props.push(Prop { id, name, description: description.trim().to_string(), x: wanted.0, y: wanted.1, blocking: *blocking, sprite, actions: Vec::new() });
                    }
                }
                Effect::Exit { x, y, to, label } => {
                    let to = slug(to);
                    let (x, y) = clamp_cell(*x, *y);
                    if to.is_empty() || to == here {
                        notes.push("exit: it leads nowhere".to_string());
                    } else if location.exit_at(x, y).is_some() {
                        notes.push(format!("exit: cell ({x}, {y}) already has an exit"));
                    } else {
                        location.exits.push(Exit { x, y, to: to.clone(), label: label.trim().to_string() });
                        if !self.locations.contains_key(&to) {
                            let name = if label.trim().is_empty() { to.clone() } else { label.trim().to_string() };
                            self.stubs.entry(to).or_insert(Stub { name, description: String::new() });
                        }
                    }
                }
                Effect::Fact { text } => self.story.remember(text),
                Effect::Check { .. } | Effect::Damage { .. } | Effect::Heal { .. } | Effect::Xp { .. } | Effect::Fight | Effect::Deed { .. } | Effect::Revive { .. } => {}
            }
        }
        // Whatever changed, the party must not end up inside something.
        let (px, py) = self.player.cell();
        if let Some(location) = self.locations.get(&here)
            && location.blocked(px, py, &self.tiles)
        {
            let start = clamp_cell(px as i64, py as i64);
            let free = |(cx, cy): (usize, usize)| !location.blocked(cx as i32, cy as i32, &self.tiles);
            if let Some((cx, cy)) = nearest(start, free) {
                self.player = Player::in_cell(cx, cy);
            }
        }
        notes
    }

    /// Carries out the `index`-th prepared action of a thing: the outcome is recorded, the
    /// effects applied, and the action is used up. Returns the outcome to show.
    pub fn perform(&mut self, thing: &str, index: usize) -> Option<String> {
        let here = self.current.clone();
        let location = self.locations.get_mut(&here)?;
        let actions = match location.props.iter_mut().find(|p| p.id == thing) {
            Some(p) => &mut p.actions,
            None => &mut location.creatures.iter_mut().find(|c| c.id == thing)?.actions,
        };
        if index >= actions.len() {
            return None;
        }
        let action = actions.remove(index);
        self.story.say(Speaker::Player, &action.label);
        self.story.say(Speaker::Master, &action.outcome);
        self.apply(&action.effects, Some(thing));
        Some(action.outcome)
    }

    /// The prop or creature nearest the party within `reach` pixels: what an "interact"
    /// press refers to.
    pub fn thing_near(&self, reach: f32) -> Option<&str> {
        let here = self.here();
        let distance = |x: usize, y: usize| {
            let centre = Player::in_cell(x, y);
            (centre.x - self.player.x).hypot(centre.y - self.player.y)
        };
        let props = here.props.iter().map(|p| (p.id.as_str(), distance(p.x, p.y)));
        let creatures = here.creatures.iter().map(|c| (c.id.as_str(), distance(c.x, c.y)));
        props.chain(creatures).filter(|(_, d)| *d <= reach).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(id, _)| id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{WorldDraft, build_world};
    use crate::fake;
    use serde_json::json;

    fn world() -> World {
        let draft: WorldDraft = serde_json::from_str(&fake::world("story")).unwrap();
        build_world(&draft, "story", "English").unwrap().0
    }

    fn effects(value: Value) -> Vec<Effect> {
        let mut notes = Vec::new();
        let read = read_effects(value.as_array().unwrap(), &mut notes);
        assert!(notes.is_empty(), "{notes:?}");
        read
    }

    #[test]
    fn a_malformed_effect_is_dropped_and_the_rest_are_kept() {
        let mut notes = Vec::new();
        let read = read_effects(json!([{ "op": "fact", "text": "a" }, { "op": "explode" }, { "text": "no op" }, { "op": "give", "name": "Key" }]).as_array().unwrap(), &mut notes);
        assert_eq!(read, [Effect::Fact { text: "a".into() }, Effect::Give { name: "Key".into(), description: String::new() }]);
        assert_eq!(notes.len(), 2);
    }

    #[test]
    fn a_prepared_action_tells_its_outcome_changes_the_world_and_is_used_up() {
        let mut w = world();
        let before = w.here().creatures.len();
        assert_eq!(w.here().creatures[0].actions.len(), 2);
        let outcome = w.perform("cat", 1).unwrap();
        assert!(outcome.contains("slips away"));
        assert_eq!(w.here().creatures.len(), before - 1);
        assert_eq!(w.story.inventory, [Item { name: "Cat hair".into(), description: "A tuft of grey fur.".into(), count: 1 }]);
        assert_eq!(w.story.log.len(), 2);
        assert_eq!(w.story.log[0].speaker, Speaker::Player);
        // The cat is gone, and so are its actions.
        assert_eq!(w.perform("cat", 0), None);
    }

    #[test]
    fn an_action_that_does_not_exist_changes_nothing() {
        let mut w = world();
        let before = w.clone();
        assert_eq!(w.perform("fire", 9), None);
        assert_eq!(w.perform("nobody", 0), None);
        assert_eq!(w, before);
    }

    #[test]
    fn items_stack_and_are_taken_one_at_a_time() {
        let mut w = world();
        let notes = w.apply(&effects(json!([{ "op": "give", "name": "Arrow" }, { "op": "give", "name": "arrow" }, { "op": "take", "name": "ARROW" }, { "op": "take", "name": "Sword" }])), None);
        assert_eq!(w.story.inventory[0].count, 1);
        assert_eq!(notes, ["take: the party has no 'Sword'"]);
        w.apply(&effects(json!([{ "op": "take", "name": "Arrow" }])), None);
        assert!(w.story.inventory.is_empty());
    }

    #[test]
    fn effects_on_things_that_are_not_here_are_noted_and_skipped() {
        let mut w = world();
        let notes = w.apply(&effects(json!([{ "op": "remove", "id": "ghost" }, { "op": "attitude", "id": "ghost", "attitude": "hostile" }, { "op": "fact", "text": "The fire never dies." }])), None);
        assert_eq!(notes.len(), 2);
        assert_eq!(w.story.facts, ["The fire never dies."]);
    }

    #[test]
    fn a_spawned_creature_stands_on_free_ground_and_an_undrawn_sprite_gets_the_marker() {
        let mut w = world();
        let fire = w.here().props[0].clone();
        let notes = w.apply(&effects(json!([{ "op": "spawn", "kind": "creature", "id": "cat", "name": "Second cat", "x": fire.x, "y": fire.y, "sprite": "dragon" }])), None);
        let newcomer = w.here().creatures.iter().find(|c| c.id == "cat_2").expect("a second cat with its own id");
        assert_ne!((newcomer.x, newcomer.y), (fire.x, fire.y));
        assert_eq!(newcomer.sprite, MARKER_SPRITE);
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn a_tile_can_be_changed_to_one_the_place_did_not_use_before() {
        let mut w = world();
        assert!(!w.here().legend.values().any(|t| t == "stone_floor"));
        let notes = w.apply(&effects(json!([{ "op": "tile", "x": 3, "y": 3, "tile": "stone_floor" }, { "op": "tile", "x": 4, "y": 3, "tile": "lava" }])), None);
        assert_eq!(w.here().tile_id(3, 3), Some("stone_floor"));
        assert_eq!(notes, ["tile: there is no tile 'lava'"]);
        assert!(w.here().map.iter().all(|r| r.len() == MAP_W));
    }

    #[test]
    fn walling_the_party_in_moves_it_to_free_ground() {
        let mut w = world();
        let (x, y) = w.player.cell();
        w.apply(&effects(json!([{ "op": "tile", "x": x, "y": y, "tile": "stone_wall" }])), None);
        let (nx, ny) = w.player.cell();
        assert_ne!((nx, ny), (x, y));
        assert!(!w.here().blocked(nx, ny, &w.tiles));
    }

    #[test]
    fn a_new_exit_brings_a_place_to_go_to() {
        let mut w = world();
        let notes = w.apply(&effects(json!([{ "op": "exit", "x": 5, "y": 5, "to": "Hidden Cellar", "label": "A trapdoor" }, { "op": "exit", "x": 5, "y": 5, "to": "elsewhere" }])), None);
        assert_eq!(w.here().exit_at(5, 5).map(|e| e.to.as_str()), Some("hidden_cellar"));
        assert_eq!(w.stubs["hidden_cellar"].name, "A trapdoor");
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn the_nearest_thing_within_reach_is_the_one_interacted_with() {
        let mut w = world();
        let cat = w.here().creatures[0].clone();
        w.player = Player::in_cell(cat.x, cat.y + 1);
        assert_eq!(w.thing_near(24.0), Some("cat"));
        w.player = Player::in_cell(1, 1);
        assert_eq!(w.thing_near(24.0), None);
    }

    #[test]
    fn the_story_is_written_up_when_it_nears_the_budget_and_keeps_the_latest_lines() {
        let mut story = Story::default();
        for n in 0..40 {
            story.say(if n % 2 == 0 { Speaker::Player } else { Speaker::Master }, &"word ".repeat(30));
        }
        assert!(!story.needs_chapter(100_000));
        assert!(story.needs_chapter(2_500));
        story.close_chapter("  They talked a great deal.  ", &["The gate is locked.".into(), "the gate is locked.".into(), "".into()]);
        assert_eq!(story.chapters, ["They talked a great deal."]);
        assert_eq!(story.log.len(), KEPT_LINES);
        assert_eq!(story.facts, ["The gate is locked."]);
        // An empty chapter means the master failed to write one: nothing is thrown away.
        story.close_chapter("  ", &[]);
        assert_eq!((story.chapters.len(), story.log.len()), (1, KEPT_LINES));
    }

    #[test]
    fn a_short_story_is_never_written_up_however_small_the_budget() {
        let mut story = Story::default();
        for _ in 0..KEPT_LINES {
            story.say(Speaker::Master, &"long ".repeat(500));
        }
        assert!(!story.needs_chapter(10));
    }
}
